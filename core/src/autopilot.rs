//! One autopilot pass: keep free space on the home volume at or above the budget by
//! reclaiming the coldest provable candidates first, shrinking (clone dedupe) before deleting
//! when the coldest is expensive to rebuild, never more than the budget needs, and saying so
//! honestly when there isn't enough.

use crate::json::Json;
use crate::scan::{self, Cand};
use crate::state::{self, Ctx, DAY};
use crate::{obj, reclaim, shrink, sys};
use std::time::{Duration, Instant};

fn costly(ctx: &Ctx, c: &Cand) -> bool {
    let measured = state::history(ctx)
        .iter()
        .rev()
        .find(|a| a.s("path") == c.path.to_string_lossy())
        .and_then(|a| a.get("rebuild_secs")?.u64());
    let has_steps = c.plan.as_ref().is_some_and(|p| !p.steps.is_empty());
    has_steps
        && (measured.unwrap_or(0) >= 120 || matches!(c.kind.as_str(), "target" | "DerivedData" | ".build" | ".next"))
}

/// Waits (up to 3 s) for APFS to hand back freed blocks; returns the free bytes seen last.
fn settle(ctx: &Ctx, budget: u64) -> u64 {
    sys::sync_all();
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        let f = sys::free_bytes(&ctx.home);
        if f >= budget || Instant::now() >= end {
            return f;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Runs one tick and reports each event through `emit`. Returns false on a fatal error.
pub fn tick(ctx: &Ctx, budget: u64, emit: &mut dyn FnMut(Json)) -> bool {
    let (dry, left) = state::dry_run(ctx);
    let mut free = sys::free_bytes(&ctx.home);
    emit(
        obj! {"event" => "tick", "now" => ctx.now, "free_bytes" => free, "budget_bytes" => budget, "dry_run" => dry, "dry_run_days_left" => left},
    );
    let mut st = state::load_state(ctx);
    st.set("last_tick", ctx.now);
    if budget == 0 || free >= budget {
        st.set("next", Json::Null);
        state::locked(ctx, || state::save_state(ctx, &with_created(ctx, st)));
        return true;
    }
    let s = match scan::scan(ctx, &ctx.home) {
        Ok(s) => s,
        Err(e) => {
            emit(obj! {"event" => "error", "error" => e});
            return false;
        }
    };
    let busy: Vec<_> = s
        .cands
        .iter()
        .filter(|c| c.blocked.as_deref().is_some_and(|b| b.starts_with("in use:")))
        .filter_map(|c| Some((c.project.clone()?, ctx.now)))
        .collect();
    state::observe(ctx, &busy);
    st.set("permission_errors", s.denied);
    let mut pool: Vec<Cand> = s.cands.into_iter().filter(|c| c.reclaimable() && c.frees_host).collect();
    pool.sort_by_key(|c| (c.active.unwrap_or(0), std::cmp::Reverse(c.bytes)));

    let shrunk_lately = st.get("last_shrink").and_then(Json::u64).is_some_and(|t| t + DAY > ctx.now);
    if !dry && !shrunk_lately && pool.first().is_some_and(|c| costly(ctx, c)) {
        let dirs: Vec<_> =
            pool.iter().filter(|c| c.kind == "node_modules" || c.kind == "venv").map(|c| c.path.clone()).collect();
        let before = sys::free_bytes(&ctx.home);
        let r = shrink::dedupe(&dirs);
        free = settle(ctx, budget);
        st.set("last_shrink", ctx.now);
        emit(
            obj! {"event" => "shrink", "files" => r.files, "bytes" => r.bytes, "measured_bytes" => free.saturating_sub(before)},
        );
    }

    let mut i = 0;
    let mut failed = 0u64;
    while free < budget && i < pool.len() {
        let need = budget - free;
        let mut got = 0u64;
        while got < need && i < pool.len() {
            let c = &pool[i];
            i += 1;
            if dry {
                let e = obj! {"event" => "would_reclaim", "path" => &c.path, "kind" => &c.kind, "bytes" => c.bytes, "reason" => &c.why};
                state::log_dry_run(
                    ctx,
                    obj! {"at" => ctx.now, "path" => &c.path, "kind" => &c.kind, "bytes" => c.bytes, "dry_run" => true},
                );
                emit(e);
                got += c.bytes;
                continue;
            }
            match reclaim::reclaim(ctx, c, "autopilot") {
                Ok(a) => {
                    let bytes = a.get("bytes").and_then(Json::u64).unwrap_or(0);
                    emit(
                        obj! {"event" => "reclaim", "id" => a.s("id"), "path" => &c.path, "kind" => &c.kind, "bytes" => bytes},
                    );
                    got += bytes;
                }
                Err(e) => {
                    failed += c.bytes;
                    emit(obj! {"event" => "skip", "path" => &c.path, "kind" => &c.kind, "reason" => e});
                }
            }
        }
        free = if dry { free + got } else { settle(ctx, budget) };
    }
    if free < budget {
        // Honest: what is still eligible but wasn't reclaimed (all of it, in a dry run).
        let untouched: u64 = pool[i..].iter().map(|c| c.bytes).sum();
        let eligible = if dry { pool.iter().map(|c| c.bytes).sum() } else { untouched + failed };
        emit(
            obj! {"event" => "shortfall", "needed_bytes" => budget - free, "eligible_bytes" => eligible, "dry_run" => dry},
        );
        let mut n = st.get("shortfalls").map(|a| a.arr().to_vec()).unwrap_or_default();
        n.push(Json::from(ctx.now));
        let skip = n.len().saturating_sub(50);
        st.set("shortfalls", Json::Arr(n[skip..].to_vec()));
    }
    let next = pool.get(i).map(|c| obj! {"path" => &c.path, "kind" => &c.kind, "bytes" => c.bytes, "reason" => &c.why});
    st.set("next", next);
    st.set("snapshot_holding", crate::machine::snapshot_holding());
    state::locked(ctx, || state::save_state(ctx, &with_created(ctx, st)));
    true
}

/// Keeps `created` (and anything else another process wrote meanwhile) when saving.
fn with_created(ctx: &Ctx, mut st: Json) -> Json {
    let disk = state::load_state(ctx);
    for k in ["created", "observed"] {
        if let Some(v) = disk.get(k) {
            st.set(k, v.clone());
        }
    }
    st
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{ctx, git, touch, write};
    use std::path::{Path, PathBuf};

    fn project(ctx: &Ctx, name: &str, when: u64) -> PathBuf {
        let p = ctx.home.join("code").join(name);
        write(&p.join("package.json"), "{}");
        write(&p.join("package-lock.json"), "{}");
        write(&p.join(".gitignore"), "node_modules/\n");
        git(&p, &["init", "-q"], Some(when));
        git(&p, &["add", "-A"], Some(when));
        git(&p, &["commit", "-qm", "c"], Some(when));
        std::fs::create_dir_all(p.join("node_modules/x")).unwrap();
        std::fs::write(p.join("node_modules/x/blob.js"), vec![7u8; 4 << 20]).unwrap();
        write(&p.join("node_modules/.package-lock.json"), "{}");
        touch(&p.join("node_modules/x/blob.js"), when);
        p
    }

    fn run(ctx: &Ctx, budget: u64) -> Vec<Json> {
        let mut ev = vec![];
        assert!(tick(ctx, budget, &mut |e| ev.push(e)));
        ev
    }

    fn kinds(ev: &[Json]) -> Vec<&str> {
        ev.iter().map(|e| e.s("event")).collect()
    }

    #[test]
    fn no_pressure_no_scan_no_deletes() {
        let ctx = ctx("ap-calm");
        let p = project(&ctx, "a", 1_500_000_000);
        let ev = run(&ctx, 1);
        assert_eq!(kinds(&ev), ["tick"]);
        assert!(p.join("node_modules").exists());
    }

    #[test]
    fn the_first_week_is_a_dry_run() {
        let ctx = ctx("ap-dry");
        let p = project(&ctx, "a", 1_500_000_000);
        let ev = run(&ctx, sys::free_bytes(&ctx.home) + (1 << 30));
        assert!(ev[0].get("dry_run") == Some(&Json::Bool(true)));
        assert!(kinds(&ev).contains(&"would_reclaim"), "{ev:?}");
        assert!(!kinds(&ev).contains(&"reclaim"));
        assert!(p.join("node_modules/x/blob.js").exists());
    }

    #[test]
    fn reclaims_coldest_first_never_recent_and_says_when_it_runs_out() {
        let mut ctx = ctx("ap-live");
        crate::state::created(&ctx);
        ctx.now += 8 * DAY;
        let coldest = project(&ctx, "coldest", 1_500_000_000);
        let cold = project(&ctx, "cold", 1_600_000_000);
        let recent = project(&ctx, "recent", ctx.now - DAY);
        // Far more than everything eligible: it must take both cold ones, coldest first, then
        // report a shortfall with nothing eligible left, and leave the recent project alone.
        let ev = run(&ctx, sys::free_bytes(&ctx.home) + (1 << 40));
        let reclaimed: Vec<PathBuf> =
            ev.iter().filter(|e| e.s("event") == "reclaim").map(|e| PathBuf::from(e.s("path"))).collect();
        assert_eq!(reclaimed, [coldest.join("node_modules"), cold.join("node_modules")]);
        let short = ev.iter().find(|e| e.s("event") == "shortfall").expect("a shortfall");
        assert_eq!(short.get("eligible_bytes"), Some(&Json::Int(0)));
        assert!(recent.join("node_modules/x/blob.js").exists());
        assert!(Path::new(&recent).join("package.json").exists());
        assert_eq!(crate::state::history(&ctx).len(), 2);
    }
}
