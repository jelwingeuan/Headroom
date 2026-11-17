//! How each deleted folder comes back, and bringing it back. Undo of a rebuildable folder is
//! running its own install or build command again; it never writes over an existing folder.

use crate::json::Json;
use crate::scan::{Cand, How};
use crate::state::{self, Ctx};
use crate::{git, obj, proc};
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub cwd: PathBuf,
    /// Commands (argument arrays) run in order; `{pins}` stands for the saved package list.
    pub steps: Vec<Vec<String>>,
    /// Tried if `steps` fail (a pinned version that no longer installs, say).
    pub fallback: Vec<Vec<String>>,
    pub lock: Option<PathBuf>,
    /// A virtualenv's exact packages, read from its dist-info folders.
    pub pins: Option<String>,
    /// Why no command is needed, when `steps` is empty.
    pub note: String,
    /// The rebuild makes a new thing in another place (a simulator gets a new UDID): undo
    /// can't check the old path.
    pub new_device: bool,
    /// What is lost for good even after undo (a simulator's test data); shown in History.
    pub loss: String,
}

impl Plan {
    pub fn describe(&self) -> String {
        match self.steps.first() {
            None => self.note.clone(),
            Some(_) => format!("rebuild: {}", self.steps.iter().map(|s| s.join(" ")).collect::<Vec<_>>().join(" && ")),
        }
    }

    pub fn note(text: &str) -> Plan {
        Plan { note: text.into(), ..Plan::default() }
    }

    pub fn run(cwd: &Path, steps: &[&[&str]]) -> Plan {
        let steps = steps.iter().map(|s| s.iter().map(|a| a.to_string()).collect()).collect();
        Plan { cwd: cwd.into(), steps, ..Plan::default() }
    }
}

fn first_existing(dir: &Path, names: &[&str]) -> Option<PathBuf> {
    names.iter().map(|n| dir.join(n)).find(|p| p.is_file())
}

pub const NODE_LOCKS: &[&str] =
    &["package-lock.json", "npm-shrinkwrap.json", "pnpm-lock.yaml", "yarn.lock", "bun.lock", "bun.lockb"];
const NODE_BUILDS: &[&str] =
    &[".next", ".nuxt", ".output", ".svelte-kit", ".angular", ".docusaurus", "dist", "build", "out"];

/// The rebuild of a folder found inside a project, or why there is none.
pub fn plan(c: &Cand) -> Result<Plan, String> {
    let base = &c.base;
    if c.how == How::Worktree {
        return worktree(c);
    }
    if c.kind == "venv" {
        return venv(c);
    }
    let Some(rule) = c.rule else { return Err("no rule for it".into()) };
    match (rule.name, c.kind.as_str()) {
        ("Node", "node_modules") => node_install(base, c.repo.as_deref()),
        ("Node", k) if NODE_BUILDS.contains(&k) => {
            let pkg = state::read_json(&base.join("package.json")).unwrap_or(Json::Null);
            if pkg.get("scripts").and_then(|s| s.get("build")).and_then(Json::str).is_none() {
                return Err("no build script in package.json to make it again".into());
            }
            if !base.join("node_modules").is_dir() && first_existing(base, NODE_LOCKS).is_none() {
                return Err("no node_modules or lockfile to build it with".into());
            }
            Ok(Plan { lock: first_existing(base, NODE_LOCKS), ..Plan::run(base, &[&["npm", "run", "build"]]) })
        }
        ("Rust", "target") => {
            let mut steps: Vec<&[&str]> = vec![];
            if c.path.join("debug").is_dir() || !c.path.join("release").is_dir() {
                steps.push(&["cargo", "build"]);
            }
            if c.path.join("release").is_dir() {
                steps.push(&["cargo", "build", "--release"]);
            }
            let lock = c.path.parent().and_then(|p| first_existing(p, &["Cargo.lock", "Cargo.toml"]));
            Ok(Plan { lock, ..Plan::run(base, &steps) })
        }
        _ if rule.rebuild.is_empty() => {
            Ok(Plan::note(&format!("a cache that {} tools refill on their own", rule.name)))
        }
        _ => {
            let mut p = Plan::run(base, rule.rebuild);
            if rule.name == "Gradle" && base.join("gradlew").is_file() {
                p.steps[0][0] = "./gradlew".into();
            }
            p.lock = rule.markers.iter().find(|m| !m.contains('*')).map(|m| base.join(m)).filter(|p| p.is_file());
            Ok(p)
        }
    }
}

fn node_install(base: &Path, repo: Option<&Path>) -> Result<Plan, String> {
    let lock = first_existing(base, NODE_LOCKS);
    let cmd: &[&str] = match lock.as_ref().and_then(|l| l.file_name()).and_then(|n| n.to_str()) {
        Some("package-lock.json" | "npm-shrinkwrap.json") => &["npm", "ci"],
        Some("pnpm-lock.yaml") => &["pnpm", "install", "--frozen-lockfile"],
        Some("yarn.lock") if base.join(".yarnrc.yml").exists() => &["yarn", "install", "--immutable"],
        Some("yarn.lock") => &["yarn", "install", "--frozen-lockfile"],
        Some(_) => &["bun", "install", "--frozen-lockfile"],
        None => {
            let stop = repo.unwrap_or(base);
            if let Some(up) = base
                .ancestors()
                .skip(1)
                .take_while(|a| a.starts_with(stop))
                .find(|a| first_existing(a, NODE_LOCKS).is_some())
            {
                return Err(format!(
                    "workspace package: its lockfile is in {}, and reinstalling there touches every package",
                    up.display()
                ));
            }
            &["npm", "install"]
        }
    };
    Ok(Plan { lock: lock.or_else(|| Some(base.join("package.json"))), ..Plan::run(base, &[cmd]) })
}

/// Virtualenvs come back with the exact packages they held (read from their dist-info
/// folders, nothing executed), falling back to the project's own dependency list.
fn venv(c: &Cand) -> Result<Plan, String> {
    let cfg = std::fs::read_to_string(c.path.join("pyvenv.cfg")).unwrap_or_default();
    let key = |k: &str| {
        cfg.lines().find_map(|l| l.split_once('=').filter(|(a, _)| a.trim() == k).map(|(_, v)| v.trim().to_string()))
    };
    let python = key("executable")
        .filter(|p| Path::new(p).is_file())
        .or_else(|| key("home").map(|h| format!("{h}/python3")).filter(|p| Path::new(p).is_file()))
        .unwrap_or_else(|| "python3".into());
    let venv = c.path.to_string_lossy().into_owned();
    let vpy = format!("{venv}/bin/python");
    let pins = pins(&c.path);
    let uv = key("uv").is_some();
    let make: Vec<String> = if uv {
        vec!["uv".into(), "venv".into(), "--python".into(), python.clone(), venv.clone()]
    } else {
        vec![python.clone(), "-m".into(), "venv".into(), venv.clone()]
    };
    let install = |args: &[&str]| -> Vec<String> {
        let mut v: Vec<String> = if uv {
            vec!["uv".into(), "pip".into(), "install".into(), "--python".into(), vpy.clone()]
        } else {
            vec![vpy.clone(), "-m".into(), "pip".into(), "install".into(), "-q".into()]
        };
        v.extend(args.iter().map(|a| a.to_string()));
        v
    };
    let base = &c.base;
    let reqs = std::fs::read_dir(base).ok().and_then(|rd| {
        rd.flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("requirements") && n.ends_with(".txt"))
            .min()
    });
    let fallback = match (&reqs, base.join("pyproject.toml").is_file() || base.join("setup.py").is_file()) {
        (Some(r), _) => vec![make.clone(), install(&["-r", r])],
        (None, true) => vec![make.clone(), install(&["-e", "."])],
        _ => vec![],
    };
    let lock = first_existing(
        base,
        &["uv.lock", "poetry.lock", "Pipfile.lock", "requirements.txt", "pyproject.toml", "Pipfile", "setup.py"],
    );
    match pins {
        Some(p) => Ok(Plan {
            cwd: base.clone(),
            steps: vec![make, install(&["-r", "{pins}"])],
            fallback,
            lock,
            pins: Some(p),
            ..Plan::default()
        }),
        None if !fallback.is_empty() => Ok(Plan { cwd: base.clone(), steps: fallback, lock, ..Plan::default() }),
        None => Err("nothing records what was installed in it".into()),
    }
}

/// "name==version" for every package in a venv's site-packages (pip itself left out).
fn pins(venv: &Path) -> Option<String> {
    let lib = std::fs::read_dir(venv.join("lib"))
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().starts_with("python"))?;
    let mut out = vec![];
    for e in std::fs::read_dir(lib.path().join("site-packages")).ok()?.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        let Some(stem) = n.strip_suffix(".dist-info") else { continue };
        let Some((name, version)) = stem.split_once('-') else { continue };
        if name.eq_ignore_ascii_case("pip") {
            continue;
        }
        let direct = state::read_json(&e.path().join("direct_url.json"));
        let editable = direct.as_ref().and_then(|d| d.get("dir_info")?.get("editable")?.bool()).unwrap_or(false);
        match direct.as_ref().map(|d| d.s("url").to_string()).and_then(|u| crate::cold::file_uri(&u)) {
            Some(p) if editable => out.push(format!("-e {}", p.display())),
            _ => out.push(format!("{name}=={version}")),
        }
    }
    out.sort();
    (!out.is_empty()).then(|| out.join("\n") + "\n")
}

fn worktree(c: &Cand) -> Result<Plan, String> {
    let gd = git::git_dir(&c.path).ok_or("not a worktree")?;
    let head = std::fs::read_to_string(gd.join("HEAD")).map_err(|e| e.to_string())?;
    let path = c.path.to_string_lossy().into_owned();
    let step = match head.trim().strip_prefix("ref: refs/heads/") {
        Some(branch) => vec!["git".into(), "worktree".into(), "add".into(), path, branch.to_string()],
        None => vec!["git".into(), "worktree".into(), "add".into(), "--detach".into(), path, head.trim().to_string()],
    };
    Ok(Plan { cwd: c.base.clone(), steps: vec![step], ..Plan::default() })
}

// ---------------------------------------------------------------- bringing things back

pub enum Outcome {
    Restored(u64),
    AlreadyThere,
    NotRestorable(String),
    Failed(String),
}

fn strings(j: Option<&Json>) -> Vec<Vec<String>> {
    j.map(Json::arr)
        .unwrap_or(&[])
        .iter()
        .map(|s| s.arr().iter().filter_map(|a| a.str().map(String::from)).collect())
        .filter(|s: &Vec<String>| !s.is_empty())
        .collect()
}

/// Runs one history action's rebuild. Never over an existing folder.
pub fn restore(ctx: &Ctx, a: &Json) -> Outcome {
    let path = PathBuf::from(a.s("path"));
    if a.s("kind") == "archive" {
        return match crate::shrink::restore(Path::new(a.s("archive"))) {
            Ok(_) => Outcome::Restored(0),
            Err(e) => Outcome::Failed(e),
        };
    }
    let new_device = a.get("new_device") == Some(&Json::Bool(true));
    if let Some(m) = std::fs::symlink_metadata(&path).ok().filter(|_| !new_device) {
        // What a failed delete of ours left behind is ours to rebuild over; anything else
        // at the path is left alone.
        use std::os::unix::fs::MetadataExt;
        let ours = a.get("partial") == Some(&Json::Bool(true))
            && Some(m.dev()) == a.get("dev").and_then(Json::u64)
            && Some(m.ino()) == a.get("ino").and_then(Json::u64);
        if !ours {
            return Outcome::AlreadyThere;
        }
    }
    let steps = strings(a.get("steps"));
    if steps.is_empty() {
        let why = a.s("note");
        return Outcome::NotRestorable(if why.is_empty() { "nothing to run".into() } else { why.into() });
    }
    if new_device
        && !steps.iter().all(|s| {
            matches!(s.iter().map(String::as_str).collect::<Vec<_>>()[..], ["xcrun", "simctl", "create", n, k, r]
                if crate::machine::sim_create_ok(n, k, r))
        })
    {
        return Outcome::Failed("refused: the recorded simctl command isn't a plain create".into());
    }
    let cwd = PathBuf::from(a.s("cwd"));
    let pins = a.s("pins");
    let t = Instant::now();
    let mut result = run_all(ctx, &cwd, &steps, pins);
    if result.is_err() {
        eprintln!("headroom: retrying {}", path.display());
        remove_partial(&path);
        result = run_all(ctx, &cwd, &steps, pins);
    }
    let fallback = strings(a.get("fallback"));
    if result.is_err() && !fallback.is_empty() {
        eprintln!("headroom: pinned rebuild failed, using the project's own dependency list");
        remove_partial(&path);
        result = run_all(ctx, &cwd, &fallback, pins);
    }
    match result {
        Err(e) => Outcome::Failed(e),
        Ok(()) if !new_device && path.is_absolute() && std::fs::symlink_metadata(&path).is_err() => {
            Outcome::Failed(format!("the commands ran but {} is still missing", path.display()))
        }
        Ok(()) => Outcome::Restored(t.elapsed().as_secs()),
    }
}

/// Removes what a failed rebuild of ours left at `path`, so the retry starts clean.
fn remove_partial(path: &Path) {
    if let Ok(m) = std::fs::symlink_metadata(path) {
        use std::os::unix::fs::MetadataExt;
        if m.is_dir() {
            let _ = crate::delete::delete_tree(path, m.dev(), m.ino());
        }
    }
}

fn run_all(ctx: &Ctx, cwd: &Path, steps: &[Vec<String>], pins: &str) -> Result<(), String> {
    for s in steps {
        run_step(ctx, cwd, s, pins)?;
    }
    Ok(())
}

fn run_step(ctx: &Ctx, cwd: &Path, step: &[String], pins: &str) -> Result<(), String> {
    let args: Vec<String> = step[1..].iter().map(|a| a.replace("{pins}", pins)).collect();
    let prog = &step[0];
    let mut cmd = if prog == "git" {
        git::command(cwd)
    } else {
        let exe = if prog.starts_with('/') {
            PathBuf::from(prog)
        } else if prog.contains('/') {
            cwd.join(prog)
        } else {
            proc::which(prog, &ctx.path_var, Some(cwd)).ok_or_else(|| format!("`{prog}` not found on PATH"))?
        };
        Command::new(exe)
    };
    cmd.args(&args).current_dir(cwd).stdin(Stdio::null());
    // stdout is reserved for Headroom's JSON: the tools' chatter goes to stderr.
    if let Ok(fd) = std::io::stderr().as_fd().try_clone_to_owned() {
        cmd.stdout(Stdio::from(fd));
    }
    eprintln!("headroom: {} $ {} {}", cwd.display(), prog, args.join(" "));
    let server = (prog == "ollama").then(|| ollama_server(ctx)).flatten();
    let status = cmd.status().map_err(|e| format!("{prog}: {e}"));
    if let Some(mut s) = server {
        let _ = s.kill();
        let _ = s.wait();
    }
    match status? {
        s if s.success() => Ok(()),
        s => Err(format!("`{} {}` exited {}", prog, args.join(" "), s.code().unwrap_or(-1))),
    }
}

/// `ollama pull` needs a server; start one for the pull if none answers.
fn ollama_server(ctx: &Ctx) -> Option<std::process::Child> {
    let exe = proc::which("ollama", &ctx.path_var, None)?;
    let up = || {
        let mut c = Command::new(&exe);
        c.arg("list");
        proc::run(c, 10).is_some_and(|o| o.ok())
    };
    if up() {
        return None;
    }
    let child = Command::new(&exe)
        .arg("serve")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    for _ in 0..60 {
        if up() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    Some(child)
}

/// Rebuilds (and marks undone) every action not undone yet. Prints one JSON summary.
pub fn undo_all(ctx: &Ctx, only: Option<&str>) -> (Json, bool) {
    let mut todo: Vec<Json> = state::history(ctx)
        .into_iter()
        .filter(|a| !a.get("undone").and_then(Json::bool).unwrap_or(false))
        .filter(|a| only.map_or(true, |id| a.s("id") == id))
        .collect();
    // Dependencies before builds (node_modules before .next, a venv before anything using
    // it), otherwise in the order they were deleted. The sort is stable.
    todo.sort_by_key(|a| !matches!(a.s("kind"), "node_modules" | "venv" | "Pods" | "vendor" | "deps"));
    let (mut done, mut failed, mut gone) = (vec![], vec![], vec![]);
    for a in &todo {
        let id = a.s("id").to_string();
        let mark = |secs: Option<u64>| {
            let now = ctx.now;
            state::update_action(ctx, &id, |x| {
                x.set("undone", true);
                x.set("undone_at", now);
                if let Some(s) = secs {
                    x.set("rebuild_secs", s);
                }
            })
        };
        match restore(ctx, a) {
            Outcome::Restored(s) => {
                mark(Some(s));
                done.push(obj! {"id" => &id, "path" => a.s("path"), "secs" => s});
            }
            Outcome::AlreadyThere => {
                mark(None);
                done.push(obj! {"id" => &id, "path" => a.s("path"), "already_there" => true});
            }
            Outcome::NotRestorable(why) => {
                mark(None);
                gone.push(obj! {"id" => &id, "path" => a.s("path"), "why" => why});
            }
            Outcome::Failed(e) => {
                eprintln!("headroom: could not rebuild {}: {e}", a.s("path"));
                failed.push(obj! {"id" => &id, "path" => a.s("path"), "error" => e});
            }
        }
    }
    let ok = failed.is_empty();
    (obj! {"restored" => done, "failed" => failed, "not_restorable" => gone}, ok)
}
