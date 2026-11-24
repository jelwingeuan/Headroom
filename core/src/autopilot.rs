//! One autopilot pass: keep free space on the home volume at or above the budget by
//! reclaiming the coldest provable candidates first, shrinking (clone dedupe) before deleting
//! when the coldest is expensive to rebuild, never more than the budget needs, and saying so
//! honestly when there isn't enough.

use crate::json::Json;
use crate::scan::{self, Cand};
use crate::state::{self, Ctx, DAY};
use crate::{obj, reclaim, shrink, sys};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

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
    state::sample(ctx, free, budget);
    let mut st = state::load_state(ctx);
    st.set("last_tick", ctx.now);
    if budget == 0 || free >= budget {
        st.set("next", Json::Null);
        // No pressure: nothing is deleted, but the menu's totals are refreshed once a day.
        let stale = st.get("totals").and_then(|t| t.get("at")).and_then(Json::u64).map_or(true, |t| t + DAY <= ctx.now);
        let mut last = None;
        if stale {
            match scan::scan(ctx, &ctx.home) {
                Ok(mut s) => {
                    st.set("permission_errors", s.denied);
                    scan::size_kept(&mut s.cands);
                    let mut list = kept(&s.cands);
                    list.extend(s.cands.iter().filter(|c| c.reclaimable() && c.frees_host).map(Measured::free));
                    st.set("totals", totals(&list, ctx.now));
                    last = Some(last_scan(list, ctx.now));
                }
                Err(e) => emit(obj! {"event" => "error", "error" => e}),
            }
        }
        save(ctx, st, last);
        return true;
    }
    let mut s = match scan::scan(ctx, &ctx.home) {
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
    scan::size_kept(&mut s.cands);
    let mut list = kept(&s.cands);
    let groups = shrink::duplicate_groups(&s.cands);
    let mut pool: Vec<Cand> = s.cands.into_iter().filter(|c| c.reclaimable() && c.frees_host).collect();
    pool.sort_by_key(|c| (c.active.unwrap_or(0), std::cmp::Reverse(c.bytes)));

    // Shrink before deleting: identical copies of one project's dependencies become clones.
    let shrunk_lately = st.get("last_shrink").and_then(Json::u64).is_some_and(|t| t + DAY > ctx.now);
    if !dry && !shrunk_lately && !groups.is_empty() {
        let (r, seen) = shrink::shrink(&ctx.home, &groups);
        free = settle(ctx, budget);
        st.set("last_shrink", ctx.now);
        emit(obj! {"event" => "shrink", "files" => r.files, "bytes" => seen, "private_bytes" => r.bytes});
    }

    let mut i = 0;
    let mut failed = 0u64;
    let mut freed_paths: Vec<std::path::PathBuf> = vec![];
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
                    freed_paths.push(c.path.clone());
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
    // For the menu: what could still come back, and what is kept and why.
    list.extend(pool.iter().filter(|c| !freed_paths.contains(&c.path)).map(Measured::free));
    st.set("totals", totals(&list, ctx.now));
    st.set("snapshot_holding", crate::machine::snapshot_holding());
    save(ctx, st, Some(last_scan(list, ctx.now)));
    true
}

/// One folder a tick's totals count: `free` when autopilot could still reclaim it.
struct Measured {
    path: PathBuf,
    kind: String,
    bytes: u64,
    free: bool,
    reason: String,
    project: Option<PathBuf>,
}

impl Measured {
    fn free(c: &Cand) -> Measured {
        Measured {
            path: c.path.clone(),
            kind: c.kind.clone(),
            bytes: c.bytes,
            free: true,
            reason: c.reason().into(),
            project: c.project.clone(),
        }
    }
}

/// Every kept candidate, nothing counted twice: a folder inside another candidate is skipped,
/// and a kept folder holding ones that may go (a worktree with its node_modules) counts only
/// the rest.
fn kept(cands: &[Cand]) -> Vec<Measured> {
    let mut out = vec![];
    for k in cands.iter().filter(|c| c.blocked.is_some() && c.frees_host) {
        let reason = k.reason();
        let nested = cands.iter().any(|o| o.path != k.path && k.path.starts_with(&o.path) && o.frees_host);
        if reason == "goes with its worktree" || nested {
            continue;
        }
        let inner: u64 = cands
            .iter()
            .filter(|o| o.reclaimable() && o.path != k.path && o.path.starts_with(&k.path))
            .map(|o| o.bytes)
            .sum();
        let bytes = k.bytes.saturating_sub(inner);
        out.push(Measured {
            path: k.path.clone(),
            kind: k.kind.clone(),
            bytes,
            free: false,
            reason: reason.into(),
            project: k.project.clone(),
        });
    }
    out
}

/// The totals `status` reports, as kept in state.
fn totals(list: &[Measured], at: u64) -> Json {
    let mut by = obj! {"recent" => 0u64, "in_use" => 0u64, "history" => 0u64, "other" => 0u64};
    for m in list.iter().filter(|m| !m.free) {
        let key = scan::kept_key(&m.reason);
        let now = by.get(key).and_then(Json::u64).unwrap_or(0);
        by.set(key, now + m.bytes);
    }
    let sum = |free: bool| list.iter().filter(|m| m.free == free).map(|m| m.bytes).sum::<u64>();
    obj! {"reclaimable_bytes" => sum(true), "protected_bytes" => sum(false), "protected_by_reason" => by, "at" => at}
}

/// last-scan.json, the list behind the totals: per-kind totals over all of it, and its 2,000
/// largest entries.
fn last_scan(mut list: Vec<Measured>, at: u64) -> Json {
    let mut by: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    for m in &list {
        let k = by.entry(&m.kind).or_default();
        if m.free {
            k.0 += m.bytes;
        } else {
            k.1 += m.bytes;
        }
    }
    let kinds = by.into_iter().map(|(k, (f, p))| (k.to_string(), obj! {"free_bytes" => f, "protected_bytes" => p}));
    let kinds = Json::Obj(kinds.collect());
    list.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    list.truncate(2000);
    let entries: Vec<Json> = list
        .iter()
        .map(|m| {
            obj! {"path" => &m.path, "kind" => &m.kind, "bytes" => m.bytes, "free" => m.free, "reason" => &m.reason, "project" => m.project.as_ref()}
        })
        .collect();
    obj! {"entries" => entries, "kinds" => kinds, "totals_at" => at}
}

/// Saves the autopilot memory, and last-scan.json when this tick scanned, under one lock so
/// both always come from the same tick.
fn save(ctx: &Ctx, st: Json, last: Option<Json>) {
    state::locked(ctx, || {
        if let Some(j) = last {
            if let Err(e) = state::write_atomic(&ctx.file("last-scan.json"), j.to_string().as_bytes()) {
                eprintln!("headroom: could not save last-scan.json: {e}");
            }
        }
        state::save_state(ctx, &with_created(ctx, st))
    });
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
    fn no_pressure_no_deletes() {
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
        let ino = |p: &Path| std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(p).unwrap());
        let recent_blob = recent.join("node_modules/x/blob.js");
        let recent_ino = ino(&recent_blob);
        // Far more than everything eligible: it must take both cold ones, coldest first, then
        // report a shortfall with nothing eligible left, and leave the recent project alone.
        let ev = run(&ctx, sys::free_bytes(&ctx.home) + (1 << 40));
        // The two cold copies share a lockfile: shrunk (cloned) before anything is deleted.
        let shrink = ev.iter().position(|e| e.s("event") == "shrink").expect("a shrink event");
        assert_eq!(ev[shrink].get("files"), Some(&Json::Int(1)));
        assert!(ev.iter().position(|e| e.s("event") == "reclaim").unwrap() > shrink);
        assert_eq!(ino(&recent_blob), recent_ino, "a recent project's files are never swapped");
        let reclaimed: Vec<PathBuf> =
            ev.iter().filter(|e| e.s("event") == "reclaim").map(|e| PathBuf::from(e.s("path"))).collect();
        assert_eq!(reclaimed, [coldest.join("node_modules"), cold.join("node_modules")]);
        let short = ev.iter().find(|e| e.s("event") == "shortfall").expect("a shortfall");
        assert_eq!(short.get("eligible_bytes"), Some(&Json::Int(0)));
        assert!(recent.join("node_modules/x/blob.js").exists());
        assert!(Path::new(&recent).join("package.json").exists());
        assert_eq!(crate::state::history(&ctx).len(), 2);
        // What it saved for Insights leaves out what it reclaimed and keeps what it protected.
        let last = state::read_json(&ctx.file("last-scan.json")).expect("last-scan.json");
        let listed: Vec<&str> = last.get("entries").unwrap().arr().iter().map(|e| e.s("path")).collect();
        assert!(reclaimed.iter().all(|p| !listed.contains(&&*p.to_string_lossy())), "{last}");
        assert!(listed.contains(&&*recent.join("node_modules").to_string_lossy()), "{last}");
    }

    #[test]
    fn last_scan_lists_the_2000_largest_and_totals_every_kind() {
        let kind = |i: u64| if i % 2 == 0 { "target" } else { "node_modules" };
        let list = (0..2500u64)
            .map(|i| Measured {
                path: PathBuf::from(format!("/p/{i}")),
                kind: kind(i).into(),
                bytes: i,
                free: i % 3 == 0,
                reason: String::new(),
                project: None,
            })
            .collect();
        let j = last_scan(list, 7);
        let bytes: Vec<u64> =
            j.get("entries").unwrap().arr().iter().map(|e| e.get("bytes").unwrap().u64().unwrap()).collect();
        assert_eq!(bytes, (500..2500).rev().collect::<Vec<_>>());
        let want = |k: &str, free: bool| (0..2500u64).filter(|i| kind(*i) == k && (i % 3 == 0) == free).sum::<u64>();
        for k in ["target", "node_modules"] {
            let t = j.get("kinds").and_then(|x| x.get(k)).unwrap();
            assert_eq!(t.get("free_bytes").and_then(Json::u64), Some(want(k, true)));
            assert_eq!(t.get("protected_bytes").and_then(Json::u64), Some(want(k, false)));
        }
        assert_eq!(j.get("totals_at"), Some(&Json::Int(7)));
    }
}
