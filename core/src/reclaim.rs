//! Deleting one candidate for real and recording how to bring it back; `clean` does it for
//! everything a scan marks reclaimable.

use crate::json::Json;
use crate::scan::{self, Cand, How};
use crate::state::{self, Ctx};
use crate::{delete, git, obj, proc, sha256};
use std::path::Path;
use std::process::Command;

/// Rechecks `c` from scratch, deletes it, records it. Returns the history action.
pub fn reclaim(ctx: &Ctx, c: &Cand, origin: &str) -> Result<Json, String> {
    if let Some(why) = delete::forbidden(ctx, &c.path) {
        return Err(format!("never deleted: {why}"));
    }
    let f = scan::recheck(ctx, c)?;
    let plan = f.plan.clone().unwrap_or_default();
    let lock_sha = plan.lock.as_ref().and_then(|l| sha256::file(l).ok());
    // The last rebuild Headroom measured for this folder, if any.
    let build_secs = state::history(ctx)
        .iter()
        .rev()
        .find(|a| a.s("path") == f.path.to_string_lossy() && a.get("rebuild_secs").is_some())
        .and_then(|a| a.get("rebuild_secs")?.u64());
    let pins = match &plan.pins {
        Some(text) => {
            let file = ctx.file(&format!(
                "pins/{}-{}.txt",
                ctx.now,
                sha256::hex(f.path.to_string_lossy().as_bytes()).get(..12).unwrap_or("x")
            ));
            state::write_atomic(&file, text.as_bytes()).map_err(|e| format!("could not save the package list: {e}"))?;
            file.to_string_lossy().into_owned()
        }
        None => String::new(),
    };
    // Err((why, partly)): `partly` when something was already removed before it failed.
    let deleted: Result<(), (String, bool)> = match &f.how {
        How::Tree => delete::delete_tree_counted(&f.path, f.dev, f.ino).map_err(|(e, n)| (e.to_string(), n > 0)),
        How::Files(files) => files.iter().enumerate().try_for_each(|(i, file)| {
            delete::delete_file(file).map_err(|e| (format!("{}: {e}", file.display()), i > 0))
        }),
        _ => Ok(()),
    };
    let partial = match deleted {
        Ok(()) => None,
        Err((e, false)) => return Err(e),
        Err((e, true)) => Some(e),
    };
    match &f.how {
        How::Tree | How::Files(_) => {}
        How::Worktree => {
            let path = f.path.to_string_lossy().into_owned();
            let o = git::run(&f.base, &["worktree", "remove", &path]).ok_or("git did not run")?;
            if !o.ok() {
                return Err("git worktree remove refused".into());
            }
        }
        How::Docker(args) => {
            let bin = proc::which("docker", &ctx.path_var, None).ok_or("docker is gone from PATH")?;
            let mut cmd = Command::new(bin);
            cmd.args(args);
            if !proc::run(cmd, 900).is_some_and(|o| o.ok()) {
                return Err(format!("docker {} failed", args.join(" ")));
            }
        }
        How::Simctl(udid) => {
            let mut cmd = Command::new("/usr/bin/xcrun");
            cmd.args(["simctl", "delete", udid]);
            if !proc::run(cmd, 300).is_some_and(|o| o.ok()) {
                return Err("xcrun simctl delete failed".into());
            }
        }
    }
    let strings =
        |v: &[Vec<String>]| Json::Arr(v.iter().map(|s| Json::Arr(s.iter().map(Json::from).collect())).collect());
    let mut action = obj! {
        "at" => ctx.now, "origin" => origin, "path" => &f.path, "kind" => &f.kind, "bytes" => f.bytes,
        "frees_host" => f.frees_host, "cwd" => &plan.cwd, "steps" => strings(&plan.steps),
        "fallback" => strings(&plan.fallback), "pins" => pins, "note" => &plan.note,
        "lock_file" => plan.lock.as_ref(), "lock_sha256" => lock_sha, "build_secs" => build_secs,
        "why" => &f.why, "dev" => f.dev, "ino" => f.ino, "undone" => false,
    };
    if plan.new_device {
        action.set("new_device", true);
    }
    if !plan.loss.is_empty() {
        action.set("data_loss", plan.loss.as_str());
    }
    if let Some(e) = &partial {
        action.set("partial", true);
        action.set("error", e.as_str());
    }
    let id = state::record(ctx, action.clone());
    action.set("id", id);
    match partial {
        // Recorded first: rebuild --all and undo bring a half-deleted folder back too.
        Some(e) => Err(format!("{e} (partly deleted; recorded, so rebuild --all brings it back)")),
        None => Ok(action),
    }
}

pub fn summary(a: &Json) -> Json {
    obj! {"id" => a.s("id"), "path" => a.s("path"), "kind" => a.s("kind"), "bytes" => a.get("bytes").and_then(Json::u64).unwrap_or(0)}
}

/// `clean <root>`: everything reclaimable under root and machine-wide. With `apply` false it
/// only lists what would go.
pub fn clean(ctx: &Ctx, root: &Path, apply: bool, origin: &str) -> Result<Json, String> {
    let s = scan::scan(ctx, root)?;
    let todo: Vec<&Cand> = s.cands.iter().filter(|c| c.reclaimable()).collect();
    if !apply {
        let list: Vec<Json> =
            todo.iter().map(|c| obj! {"path" => &c.path, "kind" => &c.kind, "bytes" => c.bytes}).collect();
        return Ok(obj! {"freed_bytes" => 0, "dry_run" => true, "actions" => list});
    }
    let (mut actions, mut errors, mut freed) = (vec![], vec![], 0u64);
    for c in todo {
        match reclaim(ctx, c, origin) {
            Ok(a) => {
                if c.frees_host {
                    freed += a.get("bytes").and_then(Json::u64).unwrap_or(0);
                }
                actions.push(summary(&a));
            }
            Err(e) => {
                eprintln!("headroom: kept {}: {e}", c.path.display());
                errors.push(obj! {"path" => &c.path, "error" => e});
            }
        }
    }
    // Copies of one project's dependencies that stay (kept for a hand edit, say) can still
    // share their blocks. Only what statfs saw come free is counted.
    let left: Vec<Cand> = s.cands.iter().filter(|c| c.path.exists()).cloned().collect();
    let groups = crate::shrink::duplicate_groups(&left);
    let (r, seen) = if groups.is_empty() { Default::default() } else { crate::shrink::shrink(&ctx.home, &groups) };
    freed += seen;
    Ok(obj! {
        "freed_bytes" => freed, "actions" => actions, "errors" => errors,
        "dedupe" => obj! {"files" => r.files, "bytes" => seen},
        "snapshot_holding" => crate::machine::snapshot_holding(),
    })
}
