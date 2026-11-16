//! Everything Headroom remembers, all inside HEADROOM_HOME: settings, autopilot memory,
//! the history of deletions (with how to rebuild each one) and the dry-run log.
//!
//! History is a JSON Lines file rewritten atomically under a lock, not SQLite: it holds a few
//! hundred small records, needs no queries, and this way there's no C library to link.

use crate::json::Json;
use crate::obj;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const DAY: u64 = 86_400;
pub const DRY_RUN_DAYS: u64 = 7;

/// Where Headroom looks and what time it is. Tests build one around temp folders.
#[derive(Clone)]
pub struct Ctx {
    pub home: PathBuf,
    pub state: PathBuf,
    pub now: u64,
    pub recent_days: u64,
    pub path_var: String,
    /// Include machine-wide places (Xcode, models, Docker, AI-tool leftovers) in a scan.
    pub machine: bool,
}

pub fn wall_clock() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Ctx {
    pub fn from_env() -> Ctx {
        // Real paths, so prefix checks agree with canonicalized scan roots (/var -> /private/var).
        let real = |p: PathBuf| p.canonicalize().unwrap_or(p);
        let home = real(PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/".into())));
        let state = std::env::var_os("HEADROOM_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("Library/Application Support/Headroom"));
        let _ = fs::create_dir_all(&state);
        let state = real(state);
        let mut c = Ctx {
            home,
            state,
            now: wall_clock(),
            recent_days: 14,
            path_var: std::env::var("PATH").unwrap_or_default(),
            machine: true,
        };
        c.recent_days = settings(&c).recent_days;
        c
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.state.join(name)
    }
}

/// Writes `data` to `path` via a temporary file and rename, so readers never see half a file.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let tmp =
        dir.join(format!(".{}.tmp-{}", path.file_name().unwrap_or_default().to_string_lossy(), std::process::id()));
    let mut f = File::create(&tmp)?;
    f.write_all(data)?;
    f.sync_all()?;
    fs::rename(&tmp, path)
}

/// Runs `f` holding HEADROOM_HOME's lock, so the app, the agent and the CLI never interleave
/// a read-modify-write of the same file.
pub fn locked<T>(ctx: &Ctx, f: impl FnOnce() -> T) -> T {
    let _ = fs::create_dir_all(&ctx.state);
    let lock = OpenOptions::new().create(true).truncate(false).write(true).open(ctx.file(".lock"));
    if let Ok(l) = &lock {
        let _ = crate::sys::lock(l);
    }
    f()
}

pub fn read_json(path: &Path) -> Option<Json> {
    Json::parse(&fs::read_to_string(path).ok()?).ok()
}

pub fn read_lines(path: &Path) -> Vec<Json> {
    fs::read_to_string(path).map(|t| t.lines().filter_map(|l| Json::parse(l).ok()).collect()).unwrap_or_default()
}

fn write_lines(path: &Path, rows: &[Json]) -> std::io::Result<()> {
    let mut text = String::new();
    for r in rows {
        text.push_str(&r.to_string());
        text.push('\n');
    }
    write_atomic(path, text.as_bytes())
}

// ---------------------------------------------------------------- settings

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// 0 means not chosen yet: autopilot leaves the disk alone.
    pub budget_bytes: u64,
    pub recent_days: u64,
    /// None: dry run for the first 7 days, then live. Some(x): the user's explicit choice.
    pub dry_run: Option<bool>,
}

pub fn settings(ctx: &Ctx) -> Settings {
    let j = read_json(&ctx.file("settings.json")).unwrap_or(Json::Null);
    Settings {
        budget_bytes: j.get("budget_bytes").and_then(Json::u64).unwrap_or(0),
        recent_days: j.get("recent_days").and_then(Json::u64).filter(|d| *d > 0).unwrap_or(14),
        dry_run: j.get("dry_run").and_then(Json::bool),
    }
}

pub fn save_settings(ctx: &Ctx, s: &Settings) -> std::io::Result<()> {
    let j = obj! {"budget_bytes" => s.budget_bytes, "recent_days" => s.recent_days, "dry_run" => s.dry_run};
    write_atomic(&ctx.file("settings.json"), j.to_string().as_bytes())
}

// ---------------------------------------------------------------- autopilot memory

pub fn load_state(ctx: &Ctx) -> Json {
    match read_json(&ctx.file("state.json")) {
        Some(j @ Json::Obj(_)) => j,
        _ => obj! {},
    }
}

pub fn save_state(ctx: &Ctx, j: &Json) {
    if let Err(e) = write_atomic(&ctx.file("state.json"), j.to_string().as_bytes()) {
        eprintln!("headroom: could not save state: {e}");
    }
}

/// When this HEADROOM_HOME was first used; recorded on the first call that asks.
pub fn created(ctx: &Ctx) -> u64 {
    locked(ctx, || {
        let mut st = load_state(ctx);
        if let Some(t) = st.get("created").and_then(Json::u64) {
            return t;
        }
        st.set("created", ctx.now);
        save_state(ctx, &st);
        ctx.now
    })
}

/// Dry run (and how many days of it are left) as of `ctx.now`.
pub fn dry_run(ctx: &Ctx) -> (bool, u64) {
    let start = created(ctx);
    let end = start + DRY_RUN_DAYS * DAY;
    let left = end.saturating_sub(ctx.now).div_ceil(DAY);
    match settings(ctx).dry_run {
        Some(x) => (x, if x { left } else { 0 }),
        None => (ctx.now < end, left),
    }
}

/// Projects Headroom saw in use (a process working in them, a build writing into them).
pub fn observed(ctx: &Ctx) -> Vec<(PathBuf, u64)> {
    match load_state(ctx).get("observed") {
        Some(Json::Obj(kv)) => kv.iter().filter_map(|(k, v)| Some((PathBuf::from(k), v.u64()?))).collect(),
        _ => vec![],
    }
}

pub fn observe(ctx: &Ctx, seen: &[(PathBuf, u64)]) {
    if seen.is_empty() {
        return;
    }
    locked(ctx, || {
        let mut st = load_state(ctx);
        let mut map = match st.get("observed") {
            Some(Json::Obj(kv)) => kv.clone(),
            _ => vec![],
        };
        for (p, t) in seen {
            let k = p.to_string_lossy().into_owned();
            match map.iter_mut().find(|(x, _)| *x == k) {
                Some(slot) => slot.1 = Json::from(slot.1.u64().unwrap_or(0).max(*t)),
                None => map.push((k, Json::from(*t))),
            }
        }
        map.retain(|(_, v)| v.u64().unwrap_or(0) + 365 * DAY > ctx.now);
        st.set("observed", Json::Obj(map));
        save_state(ctx, &st);
    })
}

// ---------------------------------------------------------------- history

pub fn history(ctx: &Ctx) -> Vec<Json> {
    read_lines(&ctx.file("history.jsonl"))
}

/// Appends one action and returns its id.
pub fn record(ctx: &Ctx, mut action: Json) -> String {
    locked(ctx, || {
        let mut rows = history(ctx);
        let id = format!("{}-{}", ctx.now, rows.len() + 1);
        action.set("id", id.as_str());
        rows.push(action);
        if let Err(e) = write_lines(&ctx.file("history.jsonl"), &rows) {
            eprintln!("headroom: could not write history: {e}");
        }
        id
    })
}

/// Rewrites one action in place.
pub fn update_action(ctx: &Ctx, id: &str, f: impl FnOnce(&mut Json)) {
    locked(ctx, || {
        let mut rows = history(ctx);
        if let Some(a) = rows.iter_mut().find(|a| a.s("id") == id) {
            f(a);
            if let Err(e) = write_lines(&ctx.file("history.jsonl"), &rows) {
                eprintln!("headroom: could not write history: {e}");
            }
        }
    })
}

pub fn log_dry_run(ctx: &Ctx, entry: Json) {
    locked(ctx, || {
        let mut rows = read_lines(&ctx.file("dryrun.jsonl"));
        rows.push(entry);
        let skip = rows.len().saturating_sub(500);
        let _ = write_lines(&ctx.file("dryrun.jsonl"), &rows[skip..]);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_lasts_seven_days_from_first_use() {
        let mut ctx = crate::testutil::ctx("state-dry");
        let t0 = ctx.now;
        assert_eq!(dry_run(&ctx), (true, 7));
        ctx.now = t0 + 6 * DAY + 3600;
        assert_eq!(dry_run(&ctx), (true, 1));
        ctx.now = t0 + 7 * DAY;
        assert_eq!(dry_run(&ctx), (false, 0));
        save_settings(&ctx, &Settings { budget_bytes: 1, recent_days: 14, dry_run: Some(true) }).unwrap();
        assert!(dry_run(&ctx).0, "an explicit choice wins");
    }

    #[test]
    fn history_round_trip() {
        let ctx = crate::testutil::ctx("state-hist");
        let id = record(&ctx, obj! {"path" => "/x", "undone" => false});
        update_action(&ctx, &id, |a| a.set("undone", true));
        let h = history(&ctx);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].get("undone"), Some(&Json::Bool(true)));
    }
}
