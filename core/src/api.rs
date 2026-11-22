//! `headroom_call`: the UI's requests, one JSON object in and one out (see core/API.md).

use crate::json::Json;
use crate::state::{self, Ctx, Settings, DAY};
use crate::{autopilot, obj, rebuild, sys};

fn error(msg: impl Into<String>) -> Json {
    obj! {"error" => msg.into()}
}

pub fn call(request: &str) -> Json {
    let req = match Json::parse(request) {
        Ok(j) => j,
        Err(e) => return error(format!("bad request: {e}")),
    };
    let ctx = Ctx::from_env();
    match req.s("op") {
        "status" => status(&ctx),
        "history" => history(&ctx),
        "undo" => undo(&ctx, &req),
        "settings_get" => settings_get(&ctx),
        "settings_set" => settings_set(&ctx, &req),
        "tick" => tick(&ctx),
        "digest" => digest(&ctx),
        "samples" => samples(&ctx),
        "breakdown" => breakdown(&ctx),
        op => error(format!("unknown op {op:?}")),
    }
}

fn samples(ctx: &Ctx) -> Json {
    obj! {"samples" => state::read_lines(&ctx.file("samples.jsonl"))}
}

/// What the last tick that scanned measured, as it saved it; no scan here.
fn breakdown(ctx: &Ctx) -> Json {
    state::read_json(&ctx.file("last-scan.json"))
        .unwrap_or_else(|| obj! {"entries" => Json::Null, "kinds" => Json::Null, "totals_at" => Json::Null})
}

fn status(ctx: &Ctx) -> Json {
    let s = state::settings(ctx);
    let (dry, left) = state::dry_run(ctx);
    let st = state::load_state(ctx);
    let totals = st.get("totals");
    obj! {
        "free_bytes" => sys::free_bytes(&ctx.home), "budget_bytes" => s.budget_bytes,
        "recent_days" => s.recent_days, "dry_run" => dry, "dry_run_days_left" => left,
        "next" => st.get("next").cloned(), "next_action" => st.get("next").cloned(), "snapshot_holding" => st.get("snapshot_holding").and_then(Json::bool).unwrap_or(false),
        "permission_errors" => st.get("permission_errors").and_then(Json::u64).unwrap_or(0),
        "last_tick" => st.get("last_tick").cloned(),
        "reclaimable_bytes" => totals.and_then(|t| t.get("reclaimable_bytes")).cloned(),
        "protected_bytes" => totals.and_then(|t| t.get("protected_bytes")).cloned(),
        "protected_by_reason" => totals.and_then(|t| t.get("protected_by_reason")).cloned(),
        "totals_at" => totals.and_then(|t| t.get("at")).cloned(),
    }
}

fn history(ctx: &Ctx) -> Json {
    let mut rows: Vec<Json> = state::history(ctx)
        .iter()
        .map(|a| {
            obj! {
                "id" => a.s("id"), "path" => a.s("path"), "kind" => a.s("kind"), "bytes" => a.get("bytes").cloned(),
                "at" => a.get("at").cloned(), "origin" => a.s("origin"), "reason" => a.s("why"),
                "data_loss" => a.get("data_loss").cloned(), "partial" => a.get("partial").cloned(),
                "undone" => a.get("undone").and_then(Json::bool).unwrap_or(false), "dry_run" => false,
                "rebuild" => a.get("steps").map(|s| s.arr().iter().map(|x| x.arr().iter().filter_map(Json::str).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join(" && ")),
            }
        })
        .collect();
    for (i, d) in state::read_lines(&ctx.file("dryrun.jsonl")).iter().enumerate() {
        let mut d = d.clone();
        d.set("id", format!("dry-{}-{i}", d.get("at").and_then(Json::u64).unwrap_or(0)));
        d.set("undone", false);
        rows.push(d);
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.get("at").and_then(Json::u64).unwrap_or(0)));
    rows.truncate(500);
    obj! {"actions" => rows}
}

fn undo(ctx: &Ctx, req: &Json) -> Json {
    let id = match req.get("id") {
        Some(Json::Str(s)) => s.clone(),
        Some(j) => j.to_string(),
        None => return error("undo needs an id"),
    };
    if id.starts_with("dry-") {
        return error("that was a dry run: nothing was deleted");
    }
    if !state::history(ctx).iter().any(|a| a.s("id") == id) {
        return error("no such action");
    }
    let (j, ok) = rebuild::undo_all(ctx, Some(&id));
    if ok {
        obj! {"ok" => true, "result" => j}
    } else {
        let msg = j.get("failed").and_then(|f| f.arr().first()).map(|f| f.s("error").to_string()).unwrap_or_default();
        error(msg)
    }
}

fn settings_get(ctx: &Ctx) -> Json {
    let s = state::settings(ctx);
    obj! {"budget_bytes" => s.budget_bytes, "recent_days" => s.recent_days, "dry_run" => state::dry_run(ctx).0, "dry_run_choice" => s.dry_run}
}

/// Only fields present change. `dry_run` is the user's choice only when it differs from what
/// is in effect (the UI sends the whole form back each time).
fn settings_set(ctx: &Ctx, req: &Json) -> Json {
    let mut s: Settings = state::settings(ctx);
    if let Some(b) = req.get("budget_bytes") {
        match b.u64() {
            Some(b) => s.budget_bytes = b,
            None => return error("budget_bytes must be a whole number of bytes"),
        }
    }
    if let Some(d) = req.get("recent_days") {
        match d.u64().filter(|d| (1..=3650).contains(d)) {
            Some(d) => s.recent_days = d,
            None => return error("recent_days must be between 1 and 3650"),
        }
    }
    if let Some(dry) = req.get("dry_run").and_then(Json::bool) {
        if dry != state::dry_run(ctx).0 {
            s.dry_run = Some(dry);
        }
    }
    match state::save_settings(ctx, &s) {
        Ok(()) => settings_get(ctx),
        Err(e) => error(format!("could not save settings: {e}")),
    }
}

fn tick(ctx: &Ctx) -> Json {
    let s = state::settings(ctx);
    let mut ctx = ctx.clone();
    ctx.recent_days = s.recent_days;
    let mut events = vec![];
    autopilot::tick(&ctx, s.budget_bytes, &mut |e| events.push(e));
    obj! {"events" => events}
}

pub fn human(n: u64) -> String {
    let units = ["bytes", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1000.0 && i < units.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} bytes")
    } else {
        format!("{v:.1} {}", units[i])
    }
}

fn digest(ctx: &Ctx) -> Json {
    let since = ctx.now.saturating_sub(7 * DAY);
    let recent = |j: &Json| j.get("at").and_then(Json::u64).unwrap_or(0) >= since;
    let acts: Vec<Json> = state::history(ctx).into_iter().filter(recent).collect();
    let freed: u64 = acts
        .iter()
        .filter(|a| a.get("frees_host").and_then(Json::bool).unwrap_or(true))
        .filter_map(|a| a.get("bytes")?.u64())
        .sum();
    let undone = acts.iter().filter(|a| a.get("undone").and_then(Json::bool).unwrap_or(false)).count();
    let would: u64 = state::read_lines(&ctx.file("dryrun.jsonl"))
        .iter()
        .filter(|d| recent(d))
        .filter_map(|d| d.get("bytes")?.u64())
        .sum();
    let st = state::load_state(ctx);
    let shortfalls =
        st.get("shortfalls").map(Json::arr).unwrap_or(&[]).iter().filter(|t| t.u64().unwrap_or(0) >= since).count();
    let s = state::settings(ctx);
    let mut parts = vec![];
    if acts.is_empty() {
        parts.push("Headroom didn't need to delete anything this week.".to_string());
    } else {
        parts.push(format!("Headroom freed {} from {} folders it can rebuild.", human(freed), acts.len()));
    }
    if undone > 0 {
        parts.push(format!("{undone} were brought back."));
    }
    if would > 0 {
        parts.push(format!("In the dry run it would have freed {}.", human(would)));
    }
    if shortfalls > 0 {
        parts.push(format!("{shortfalls} times there wasn't enough it could safely reclaim."));
    }
    if s.budget_bytes > 0 {
        parts.push(format!("Free now: {} (budget {}).", human(sys::free_bytes(&ctx.home)), human(s.budget_bytes)));
    }
    obj! {"title" => "Your week with Headroom", "body" => parts.join(" "), "freed_bytes" => freed, "actions" => acts.len(), "would_bytes" => would, "shortfalls" => shortfalls}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reports_the_last_ticks_totals_and_never_scans() {
        use crate::testutil::{git, touch, write};
        let mut ctx = crate::testutil::ctx("api-totals");
        let mk = |name: &str, when: u64| {
            let p = ctx.home.join("code").join(name);
            write(&p.join("package.json"), "{}");
            write(&p.join("package-lock.json"), "{}");
            write(&p.join(".gitignore"), "node_modules/\n");
            git(&p, &["init", "-q"], Some(when));
            git(&p, &["add", "-A"], Some(when));
            git(&p, &["commit", "-qm", "c"], Some(when));
            std::fs::create_dir_all(p.join("node_modules/x")).unwrap();
            std::fs::write(p.join("node_modules/x/blob.js"), vec![1u8; 1 << 20]).unwrap();
            write(&p.join("node_modules/.package-lock.json"), "{}");
            touch(&p.join("node_modules/x/blob.js"), when);
            p
        };
        mk("cold", 1_600_000_000);
        mk("recent", ctx.now - DAY);
        write(&ctx.home.join(".claude/projects/x/a.jsonl"), &"history\n".repeat(1000));
        ctx.machine = true;
        let before = crate::scan::SCANS.with(|n| n.get());
        let s = status(&ctx);
        for k in ["reclaimable_bytes", "protected_bytes", "protected_by_reason", "totals_at"] {
            assert_eq!(s.get(k), Some(&Json::Null), "{k} before any tick");
        }
        // A dry-run tick under pressure scans (and deletes nothing).
        crate::autopilot::tick(&ctx, sys::free_bytes(&ctx.home) + (1 << 40), &mut |_| {});
        assert!(ctx.home.join("code/cold/node_modules/x/blob.js").exists());
        let mid = crate::scan::SCANS.with(|n| n.get());
        assert_eq!(mid, before + 1);
        let s = status(&ctx);
        assert_eq!(crate::scan::SCANS.with(|n| n.get()), mid, "status never scans");
        let bytes = |k: &str| s.get(k).and_then(Json::u64).unwrap();
        assert!(bytes("reclaimable_bytes") >= 1 << 20, "{s}");
        assert!(bytes("protected_bytes") >= (1 << 20) + 8000, "{s}");
        let by = s.get("protected_by_reason").unwrap();
        assert!(by.get("recent").and_then(Json::u64).unwrap() >= 1 << 20, "{s}");
        assert!(by.get("history").and_then(Json::u64).unwrap() >= 8000, "{s}");
        assert_eq!(s.get("totals_at").and_then(Json::u64), Some(ctx.now));
    }

    #[test]
    fn totals_come_daily_even_with_plenty_of_space_and_no_budget() {
        use crate::testutil::{git, touch, write};
        let mut ctx = crate::testutil::ctx("api-totals-calm");
        ctx.machine = true; // AI history is a machine-wide place
        let p = ctx.home.join("code/cold");
        write(&p.join("package.json"), "{}");
        write(&p.join("package-lock.json"), "{}");
        write(&p.join(".gitignore"), "node_modules/\n");
        git(&p, &["init", "-q"], Some(1_600_000_000));
        git(&p, &["add", "-A"], Some(1_600_000_000));
        git(&p, &["commit", "-qm", "c"], Some(1_600_000_000));
        std::fs::create_dir_all(p.join("node_modules/x")).unwrap();
        std::fs::write(p.join("node_modules/x/blob.js"), vec![1u8; 1 << 20]).unwrap();
        write(&p.join("node_modules/.package-lock.json"), "{}");
        touch(&p.join("node_modules/x/blob.js"), 1_600_000_000);
        write(&ctx.home.join(".claude/projects/x/a.jsonl"), &"history\n".repeat(1000));
        let scans = || crate::scan::SCANS.with(|n| n.get());
        for budget in [0, 1] {
            // budget 0 (none chosen) and a budget far below free space: no pressure either way.
            let mut c = ctx.clone();
            c.state = ctx.state.join(format!("b{budget}"));
            let before = scans();
            crate::autopilot::tick(&c, budget, &mut |_| {});
            assert_eq!(scans(), before + 1, "no saved totals: one read-only scan");
            assert!(p.join("node_modules/x/blob.js").exists(), "nothing deleted");
            let s = status(&c);
            assert!(s.get("reclaimable_bytes").and_then(Json::u64).unwrap() >= 1 << 20, "{s}");
            assert!(s.get("protected_bytes").and_then(Json::u64).unwrap() >= 8000, "{s}");
            assert_eq!(s.get("totals_at").and_then(Json::u64), Some(c.now));
            // Within 24 h: no new scan.
            c.now += 23 * 3600;
            crate::autopilot::tick(&c, budget, &mut |_| {});
            assert_eq!(scans(), before + 1, "fresh totals are not rescanned");
            // A day later they are refreshed.
            c.now += 3600;
            crate::autopilot::tick(&c, budget, &mut |_| {});
            assert_eq!(scans(), before + 2);
        }
    }

    #[test]
    fn breakdown_adds_up_to_status_and_lists_what_scan_reclaims() {
        use crate::testutil::{git, script, touch, world, write};
        let mut ctx = crate::testutil::ctx("api-breakdown");
        ctx.machine = true; // AI history and Docker are machine-wide places
        for (name, when) in [("cold", 1_600_000_000), ("recent", ctx.now - DAY)] {
            let p = ctx.home.join("code").join(name);
            write(&p.join("package.json"), "{}");
            write(&p.join("package-lock.json"), "{}");
            write(&p.join(".gitignore"), "node_modules/\n");
            git(&p, &["init", "-q"], Some(when));
            git(&p, &["add", "-A"], Some(when));
            git(&p, &["commit", "-qm", "c"], Some(when));
            std::fs::create_dir_all(p.join("node_modules/x")).unwrap();
            std::fs::write(p.join("node_modules/x/blob.js"), vec![1u8; 1 << 20]).unwrap();
            write(&p.join("node_modules/.package-lock.json"), "{}");
            touch(&p.join("node_modules/x/blob.js"), when);
        }
        write(&ctx.home.join(".claude/projects/x/a.jsonl"), &"history\n".repeat(1000));
        // Docker's build cache may go, but its space stays inside Docker's VM disk.
        let docker = world(&ctx).join("docker");
        script(&docker, r#"[ "$1 $2" = "system df" ] && echo '{"Type":"Build Cache","Reclaimable":"1.5GB"}'"#);
        ctx.path_var = format!("{}:/usr/bin:/bin", world(&ctx).display());
        let scans = || crate::scan::SCANS.with(|n| n.get());
        let n = |j: &Json, k: &str| j.get(k).and_then(Json::u64).unwrap_or_else(|| panic!("{k} in {j}"));
        // Under pressure (a dry run: nothing is deleted) and with no budget chosen.
        for budget in [sys::free_bytes(&ctx.home) + (1 << 40), 0] {
            let mut c = ctx.clone();
            c.state = ctx.state.join(format!("b{budget}"));
            let b = breakdown(&c);
            for k in ["entries", "kinds", "totals_at"] {
                assert_eq!(b.get(k), Some(&Json::Null), "{k} before any tick");
            }
            crate::autopilot::tick(&c, budget, &mut |_| {});
            let before = scans();
            let (s, b, rows) = (status(&c), breakdown(&c), samples(&c));
            assert_eq!(scans(), before, "status, breakdown and samples never scan");
            let row = &rows.get("samples").unwrap().arr()[0];
            assert_eq!((n(row, "at"), n(row, "budget")), (c.now, budget));
            assert!(n(row, "free") > 0);

            // The per-kind totals are exactly the status totals.
            assert_eq!(b.get("totals_at"), s.get("totals_at"));
            let Some(Json::Obj(kinds)) = b.get("kinds") else { panic!("{b}") };
            let sum = |k: &str| kinds.iter().map(|(_, t)| n(t, k)).sum::<u64>();
            assert_eq!(sum("free_bytes"), n(&s, "reclaimable_bytes"), "{b}");
            assert_eq!(sum("protected_bytes"), n(&s, "protected_bytes"), "{b}");
            assert!(n(&s, "reclaimable_bytes") >= 1 << 20 && n(&s, "protected_bytes") >= (1 << 20) + 8000, "{s}");

            // UI equals CLI: `scan --json` prints these objects (cli.rs).
            let entries = b.get("entries").unwrap().arr();
            let listed: Vec<Json> =
                crate::scan::scan(&c, &c.home).unwrap().cands.iter().map(crate::scan::Cand::to_json).collect();
            assert!(listed.iter().any(|e| e.s("kind") == "docker_build_cache"), "the fake docker was seen");
            for e in &listed {
                let found = entries.iter().find(|x| x.s("path") == e.s("path"));
                if e.s("kind").starts_with("docker_") {
                    assert!(found.is_none(), "Docker frees nothing on this volume: {e}");
                } else if e.get("reclaimable") == Some(&Json::Bool(true)) {
                    let f = found.unwrap_or_else(|| panic!("{e} missing from {b}"));
                    assert_eq!(f.get("free"), Some(&Json::Bool(true)), "{f}");
                    assert_eq!((f.get("bytes"), f.s("reason")), (e.get("bytes"), e.s("reason")), "{f}");
                } else {
                    // Kept: by path only, since `scan` leaves kept folders unsized (a speed choice).
                    let f = found.unwrap_or_else(|| panic!("{e} missing from {b}"));
                    assert_eq!(f.get("free"), Some(&Json::Bool(false)), "{f}");
                }
            }
            assert_eq!(entries.len(), listed.len() - 1, "nothing listed that scan doesn't list");
        }
    }

    #[test]
    fn settings_round_trip_keeps_the_automatic_dry_run() {
        let ctx = crate::testutil::ctx("api-settings");
        // The UI sends the whole form back, dry_run included, after changing only the budget.
        let r = settings_set(
            &ctx,
            &obj! {"op" => "settings_set", "budget_bytes" => 20_000_000_000u64, "recent_days" => 14, "dry_run" => true},
        );
        assert_eq!(r.get("budget_bytes").and_then(Json::u64), Some(20_000_000_000));
        assert_eq!(state::settings(&ctx).dry_run, None, "still automatic: it ends after 7 days");
        let r = settings_set(&ctx, &obj! {"dry_run" => false});
        assert_eq!(r.get("dry_run"), Some(&Json::Bool(false)));
        assert!(settings_set(&ctx, &obj! {"recent_days" => 0}).get("error").is_some());
    }

    #[test]
    fn history_lists_actions_and_dry_runs_and_undo_refuses_dry_runs() {
        let ctx = crate::testutil::ctx("api-history");
        state::record(
            &ctx,
            obj! {"at" => 10, "path" => "/x/node_modules", "kind" => "node_modules", "bytes" => 5, "undone" => false},
        );
        state::log_dry_run(
            &ctx,
            obj! {"at" => 20, "path" => "/y/target", "kind" => "target", "bytes" => 7, "dry_run" => true},
        );
        let h = history(&ctx);
        let rows = h.get("actions").unwrap().arr();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get("dry_run"), Some(&Json::Bool(true)));
        assert!(undo(&ctx, &obj! {"id" => rows[0].s("id")}).get("error").is_some());
        assert!(undo(&ctx, &obj! {"id" => "nope"}).get("error").is_some());
        let d = digest(&ctx);
        assert!(!d.s("body").is_empty());
    }
}
