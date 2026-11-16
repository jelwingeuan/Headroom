//! Finding what may go, and proving it (docs/THREAT-MODEL.md, "What Headroom may delete").
//!
//! The walk lists each folder once with getattrlistbulk, never enters a folder it already
//! found (node_modules, target…), never enters .git, bundles or dependency trees, and runs on
//! all cores. Only the folders found are sized. Then every candidate is judged: git (tracked
//! files block it; ambiguous names need git to ignore them), running programs, recent use,
//! hand edits, and whether its rebuild command is known.

use crate::cold;
use crate::json::Json;
use crate::rules::{self, Rule};
use crate::state::{Ctx, DAY};
use crate::sys::{self, Entry, VDIR, VREG};
use crate::{git, obj, proc, rebuild};
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};

#[derive(Clone, Debug, PartialEq)]
pub enum How {
    /// Delete the folder (fd-relative, see delete.rs).
    Tree,
    /// Delete exactly these files (an Ollama model's manifest and the blobs only it uses).
    Files(Vec<PathBuf>),
    /// `git worktree remove` (never --force).
    Worktree,
    /// One docker command, arguments after `docker`.
    Docker(Vec<String>),
    /// `xcrun simctl delete <udid>`.
    Simctl(String),
}

#[derive(Clone, Debug)]
pub struct Cand {
    pub path: PathBuf,
    pub kind: String,
    pub how: How,
    /// Where the rebuild runs: the folder holding the marker file.
    pub base: PathBuf,
    /// The project it belongs to (git top level, else the marker folder).
    pub project: Option<PathBuf>,
    pub repo: Option<PathBuf>,
    pub rule: Option<&'static Rule>,
    pub bytes: u64,
    /// `bytes` came from the tool itself (Docker); don't walk anything.
    pub sized: bool,
    pub dev: u64,
    pub ino: u64,
    pub blocked: Option<String>,
    pub why: String,
    pub active: Option<u64>,
    /// The "used in the last N days" rule applies.
    pub recent_rule: bool,
    /// Counts toward autopilot's free space (Docker frees space inside its VM disk, not here).
    pub frees_host: bool,
    pub machine: bool,
    pub plan: Option<rebuild::Plan>,
    newest_file: Option<(i64, PathBuf)>,
}

impl Cand {
    pub fn new(path: PathBuf, kind: &str, how: How) -> Cand {
        Cand {
            base: path.parent().unwrap_or(&path).to_path_buf(),
            path,
            kind: kind.into(),
            how,
            project: None,
            repo: None,
            rule: None,
            bytes: 0,
            sized: false,
            dev: 0,
            ino: 0,
            blocked: None,
            why: String::new(),
            active: None,
            recent_rule: true,
            frees_host: true,
            machine: false,
            plan: None,
            newest_file: None,
        }
    }

    pub fn reclaimable(&self) -> bool {
        self.blocked.is_none()
    }

    pub fn reason(&self) -> &str {
        self.blocked.as_deref().unwrap_or(&self.why)
    }

    pub fn to_json(&self) -> Json {
        obj! {
            "path" => &self.path, "kind" => &self.kind, "bytes" => self.bytes,
            "reclaimable" => self.reclaimable(), "reason" => self.reason(),
            "project" => self.project.as_ref(),
        }
    }
}

// ---------------------------------------------------------------- parallel work queue

pub fn threads() -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(2, 12)
}

/// Runs `f` on every task on all cores; `f` may add more tasks. Returns everything `f` output.
pub fn par_walk<T: Send, R: Send>(init: Vec<T>, f: impl Fn(T, &mut Vec<T>, &mut Vec<R>) + Sync) -> Vec<R> {
    let q = Mutex::new((init, 0usize)); // (tasks, busy workers)
    let cv = Condvar::new();
    let results = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..threads() {
            s.spawn(|| {
                let mut out = Vec::new();
                loop {
                    let task = {
                        let mut g = q.lock().unwrap();
                        loop {
                            if let Some(t) = g.0.pop() {
                                g.1 += 1;
                                break Some(t);
                            }
                            if g.1 == 0 {
                                break None;
                            }
                            g = cv.wait(g).unwrap();
                        }
                    };
                    let Some(t) = task else {
                        cv.notify_all();
                        break;
                    };
                    let mut more = Vec::new();
                    f(t, &mut more, &mut out);
                    let mut g = q.lock().unwrap();
                    g.1 -= 1;
                    let wake = !more.is_empty() || g.1 == 0;
                    g.0.extend(more);
                    drop(g);
                    if wake {
                        cv.notify_all();
                    }
                }
                results.lock().unwrap().extend(out);
            });
        }
    });
    results.into_inner().unwrap()
}

pub fn par_map<T: Send, R: Send>(items: Vec<T>, f: impl Fn(T) -> R + Sync) -> Vec<R> {
    par_walk(items, |t, _, out| out.push(f(t)))
}

// ---------------------------------------------------------------- where scanning is allowed

/// System folders, the folders above home, and ~/Library are refused; home, temp folders and
/// external volumes are fine.
pub fn refuse(ctx: &Ctx, root: &Path) -> Result<(), String> {
    if ctx.home.starts_with(root) && root != ctx.home {
        return Err(format!("{} holds the home folder; scan a folder inside it", root.display()));
    }
    if root.starts_with(ctx.home.join("Library")) {
        return Err("~/Library is only cleaned through the places Headroom knows".into());
    }
    let temp = std::env::temp_dir().canonicalize().unwrap_or_default();
    let ok = root.starts_with(&ctx.home)
        || (temp.components().count() > 2 && root.starts_with(&temp))
        || root.starts_with("/Volumes/");
    if ok {
        Ok(())
    } else {
        Err(format!("{} is a system folder", root.display()))
    }
}

// ---------------------------------------------------------------- the walk

struct Task {
    dir: PathBuf,
    repo: Option<PathBuf>,
    project: Option<PathBuf>,
    depth: u32,
}

const MAX_DEPTH: u32 = 40;
const AGENT_WORKTREES: &[&str] = &[".claude/worktrees", ".codex/worktrees", ".cursor/worktrees", ".worktrees"];

fn names(entries: &[Entry]) -> Vec<String> {
    entries.iter().map(|e| e.name_str().into_owned()).collect()
}

/// Candidates directly in `dir`, and the folders to look into next.
fn visit(
    ctx: &Ctx,
    dir: &Path,
    entries: &[Entry],
    repo: Option<&Path>,
    project: Option<&Path>,
    depth: u32,
    out: &mut Vec<Cand>,
) -> Vec<Task> {
    let names = names(entries);
    let entry = |n: &str| entries.iter().find(|e| e.name == n.as_bytes());
    let git_here = entry(".git");
    let repo = if git_here.is_some() { Some(dir.to_path_buf()) } else { repo.map(Path::to_path_buf) };
    if depth > 0 && names.iter().any(|n| n == "pyvenv.cfg") {
        let parent = dir.parent().unwrap_or(dir);
        let mut c = Cand::new(dir.into(), "venv", How::Tree);
        c.project = Some(repo.clone().unwrap_or_else(|| parent.into()));
        c.repo = repo;
        out.push(c);
        return vec![];
    }
    let worktree = git_here.is_some_and(|e| e.kind == VREG)
        && AGENT_WORKTREES.iter().any(|w| dir.parent().is_some_and(|p| p.ends_with(w)));
    if worktree {
        if let Some(main) = git::git_dir(dir).and_then(|g| main_checkout(&g)) {
            let mut c = Cand::new(dir.into(), "worktree", How::Worktree);
            c.base = main;
            c.project = Some(dir.into());
            c.repo = Some(dir.into());
            out.push(c);
        }
    }
    let mut taken: Vec<&str> = vec![];
    let mut marked = false;
    for rule in rules::rules_for(&names) {
        marked = true;
        for d in rule.dirs {
            let p = dir.join(d);
            let is_dir = if d.contains('/') {
                std::fs::symlink_metadata(&p).is_ok_and(|m| m.is_dir())
            } else {
                entry(d).is_some_and(|e| e.kind == VDIR)
            };
            if !is_dir || taken.contains(d) {
                continue;
            }
            taken.push(d);
            let mut c = Cand::new(p, d, How::Tree);
            c.base = dir.into();
            c.project = Some(repo.clone().unwrap_or_else(|| dir.into()));
            c.repo = repo.clone();
            c.rule = Some(rule);
            out.push(c);
        }
    }
    let project = repo.clone().or_else(|| marked.then(|| dir.to_path_buf())).or_else(|| project.map(Path::to_path_buf));
    let at_home = dir == ctx.home;
    if depth >= MAX_DEPTH {
        return vec![];
    }
    entries
        .iter()
        .filter(|e| e.kind == VDIR && e.error == 0)
        .map(|e| e.name_str().into_owned())
        .filter(|n| {
            !taken.contains(&n.as_str())
                && !rules::never_enter(n)
                && !(at_home && (n.starts_with('.') || n == "Library"))
        })
        .map(|n| Task { dir: dir.join(n), repo: repo.clone(), project: project.clone(), depth: depth + 1 })
        .collect()
}

/// A worktree's git dir is `<main>/.git/worktrees/<name>`; returns `<main>`.
fn main_checkout(gitdir: &Path) -> Option<PathBuf> {
    let dotgit = gitdir.parent()?.parent()?;
    (gitdir.parent()?.file_name()? == "worktrees" && dotgit.file_name()? == ".git")
        .then(|| dotgit.parent().map(Path::to_path_buf))?
}

fn walk(ctx: &Ctx, root: &Path) -> (Vec<Cand>, usize) {
    let repo = git::toplevel(root);
    let first = Task { dir: root.into(), project: repo.clone(), repo, depth: 0 };
    let res: Vec<Result<Cand, ()>> = par_walk(vec![first], |t: Task, more, out: &mut Vec<Result<Cand, ()>>| {
        let Ok(entries) = sys::list_path(&t.dir) else {
            out.push(Err(()));
            return;
        };
        let mut found = vec![];
        more.extend(visit(ctx, &t.dir, &entries, t.repo.as_deref(), t.project.as_deref(), t.depth, &mut found));
        out.extend(found.into_iter().map(Ok));
    });
    let denied = res.iter().filter(|r| r.is_err()).count();
    (res.into_iter().flatten().collect(), denied)
}

// ---------------------------------------------------------------- sizes

#[derive(Clone, Copy, PartialEq)]
enum Track {
    No,
    /// The top of a node_modules: dot folders in it (.cache, .vite) are tools' caches.
    NodeTop,
    Node,
}

const INSTALL_RECORDS: &[&str] = &[".package-lock.json", ".yarn-integrity", ".modules.yaml", ".yarn-state.yml"];

#[derive(Default)]
struct Sized {
    bytes: u64,
    hard: Vec<(u64, u32, u64)>,
    newest: Option<(i64, PathBuf)>,
}

/// Private allocated bytes of every candidate (hard links counted only when all their links
/// are inside), plus the newest file of each node_modules for the hand-edit check.
fn size_all(cands: &mut [Cand]) {
    let mut tasks = vec![];
    for (i, c) in cands.iter_mut().enumerate() {
        if c.sized || c.blocked.is_some() {
            continue;
        }
        match &c.how {
            How::Tree | How::Worktree => {
                tasks.push((i, c.path.clone(), if c.kind == "node_modules" { Track::NodeTop } else { Track::No }))
            }
            How::Files(files) => c.bytes = files_bytes(files),
            _ => {}
        }
    }
    let parts = par_walk(tasks, |(i, dir, track): (usize, PathBuf, Track), more, out: &mut Vec<(usize, Sized)>| {
        let Ok(entries) = sys::list_path(&dir) else { return };
        let mut s = Sized::default();
        for e in entries.iter().filter(|e| e.error == 0) {
            let name = e.name_str();
            if e.kind == VDIR {
                let t = match track {
                    Track::NodeTop if name.starts_with('.') => Track::No,
                    Track::NodeTop => Track::Node,
                    t => t,
                };
                more.push((i, dir.join(&*name), t));
                continue;
            }
            if e.nlink > 1 && e.kind == VREG {
                s.hard.push((e.ino, e.nlink, e.private));
            } else {
                s.bytes += e.private;
            }
            let record = track == Track::NodeTop && INSTALL_RECORDS.contains(&&*name);
            if e.kind == VREG && track != Track::No && !record && s.newest.as_ref().map_or(true, |(t, _)| e.mtime > *t)
            {
                s.newest = Some((e.mtime, dir.join(&*name)));
            }
        }
        out.push((i, s));
    });
    let mut links: HashMap<(usize, u64), (u32, u32, u64)> = HashMap::new();
    for (i, s) in parts {
        let c = &mut cands[i];
        c.bytes += s.bytes;
        if let Some(n) = s.newest {
            if c.newest_file.as_ref().map_or(true, |(t, _)| n.0 > *t) {
                c.newest_file = Some(n);
            }
        }
        for (ino, nlink, private) in s.hard {
            let e = links.entry((i, ino)).or_insert((nlink, 0, private));
            e.1 += 1;
            if e.1 == e.0 {
                c.bytes += e.2;
            }
        }
    }
}

/// Private bytes of single files, listing each parent folder once.
pub fn files_bytes(files: &[PathBuf]) -> u64 {
    let mut by_dir: HashMap<&Path, Vec<&[u8]>> = HashMap::new();
    for f in files {
        if let (Some(d), Some(n)) = (f.parent(), f.file_name()) {
            by_dir.entry(d).or_default().push(std::os::unix::ffi::OsStrExt::as_bytes(n));
        }
    }
    by_dir
        .iter()
        .map(|(d, want)| {
            sys::list_path(d)
                .map_or(0, |es| es.iter().filter(|e| want.contains(&e.name.as_slice())).map(|e| e.private).sum())
        })
        .sum()
}

// ---------------------------------------------------------------- running programs

/// Shells and coding agents sit in project folders without needing their dependencies; the
/// dev servers and watchers they start show up on their own.
const QUIET: &[&str] = &[
    "zsh",
    "bash",
    "fish",
    "sh",
    "-zsh",
    "-bash",
    "-sh",
    "login",
    "tmux",
    "screen",
    "nu",
    "claude",
    "disclaimer",
    "codex",
    "cursor-agent",
    "gemini",
    "aider",
    "lsof",
    "headroom",
    "Headroom",
];

/// Parts of a command line that mark a coding agent even when it runs as `node` (Claude Code
/// installed with npm, say).
const AGENT_HINTS: &[&str] = &["claude-code", "/claude", "codex", "cursor-agent", "gemini", "aider"];

/// A shell or coding agent, by its `ps` command line.
pub fn known_agent(cmdline: &str) -> bool {
    QUIET.contains(&proc::program(cmdline)) || AGENT_HINTS.iter().any(|h| cmdline.contains(h))
}

/// (program, working folder) of every running process but this one, or None when `lsof`
/// couldn't tell (missing, failed, timed out): then nothing counts as idle.
pub fn cwds() -> Option<Vec<(String, PathBuf)>> {
    cwds_with(Path::new("/usr/sbin/lsof"))
}

pub fn cwds_with(lsof: &Path) -> Option<Vec<(String, PathBuf)>> {
    let mut c = std::process::Command::new(lsof);
    c.args(["-nP", "-w", "-d", "cwd", "-Fpcn"]);
    let text = proc::run(c, 30).filter(|o| o.code.is_some())?.stdout;
    let me = std::process::id().to_string();
    // The parent (whatever started Headroom) is skipped only if it is a shell or an agent:
    // an MCP client sits in its project without needing its dependencies; `node` doesn't.
    let ppid = std::os::unix::process::parent_id();
    let mut ps = std::process::Command::new("/bin/ps");
    ps.args(["-o", "command=", "-p", &ppid.to_string()]);
    let parent = proc::run(ps, 10).filter(|o| known_agent(o.stdout.trim())).map(|_| ppid.to_string());
    let (mut pid, mut cmd, mut out, mut seen) = (String::new(), String::new(), vec![], false);
    for line in text.lines() {
        match line.split_at_checked(1) {
            Some(("p", v)) => {
                pid = v.into();
                seen = true;
            }
            Some(("c", v)) => cmd = v.into(),
            Some(("n", v)) if pid != me && Some(&pid) != parent.as_ref() && !QUIET.contains(&cmd.as_str()) => {
                out.push((cmd.clone(), PathBuf::from(v)))
            }
            _ => {}
        }
    }
    // lsof always sees at least this process; no process at all means it didn't work.
    seen.then_some(out)
}

/// Programs working inside `project`, or in a project folder that encloses it (a dev server
/// at a monorepo's root uses every package's dependencies).
fn busy_in(cwds: &[(String, PathBuf)], enclosing: &[PathBuf], project: &Path) -> Option<String> {
    let mut names: Vec<&str> = cwds
        .iter()
        .filter(|(_, d)| d.starts_with(project) || (project.starts_with(d) && enclosing.contains(d)))
        .map(|(n, _)| n.as_str())
        .collect();
    names.sort_unstable();
    names.dedup();
    (!names.is_empty()).then(|| names.join(", "))
}

fn is_project_dir(d: &Path) -> bool {
    let names: Vec<String> = std::fs::read_dir(d)
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.iter().any(|n| n == ".git") || rules::rules_for(&names).next().is_some()
}

// ---------------------------------------------------------------- judging

struct GitFacts {
    tracked: bool,
    ignored: bool,
}

/// Decides, for every candidate, whether it may go and why, and sizes the ones that may.
/// Kept folders aren't walked (their `bytes` stay 0): listing a million files costs seconds
/// even warm, and only what may go needs an honest size.
pub fn judge(ctx: &Ctx, cands: &mut [Cand], cwds: Option<&[(String, PathBuf)]>) {
    for c in cands.iter_mut() {
        if let Ok(m) = std::fs::symlink_metadata(&c.path) {
            (c.dev, c.ino) = (m.dev(), m.ino());
        }
    }
    // git: two calls per repository for all its candidates.
    let mut by_repo: HashMap<PathBuf, Vec<(usize, String)>> = HashMap::new();
    for (i, c) in cands.iter().enumerate() {
        if let (Some(r), How::Tree, false) = (&c.repo, &c.how, c.machine) {
            if let Ok(rel) = c.path.strip_prefix(r) {
                by_repo.entry(r.clone()).or_default().push((i, rel.to_string_lossy().into_owned()));
            }
        }
    }
    let facts: HashMap<usize, Option<GitFacts>> =
        par_map(by_repo.into_iter().collect(), |(repo, items): (PathBuf, Vec<(usize, String)>)| {
            let rels: Vec<String> = items.iter().map(|(_, r)| r.clone()).collect();
            let res = git::tracked_and_ignored(&repo, &rels);
            items
                .iter()
                .enumerate()
                .map(|(k, (i, _))| (*i, res.as_ref().map(|(t, ig)| GitFacts { tracked: t[k], ignored: ig[k] })))
                .collect::<Vec<_>>()
        })
        .into_iter()
        .flatten()
        .collect();
    let worktrees: HashMap<usize, Result<(), String>> = par_map(
        cands.iter().enumerate().filter(|(_, c)| c.how == How::Worktree).map(|(i, c)| (i, c.clone())).collect(),
        |(i, c): (usize, Cand)| (i, worktree_ok(&c)),
    )
    .into_iter()
    .collect();
    let sig = cold::signals(ctx);
    let mut projects: Vec<(PathBuf, Option<PathBuf>)> =
        cands.iter().filter_map(|c| Some((c.project.clone()?, c.repo.clone()))).collect();
    projects.sort();
    projects.dedup_by(|a, b| a.0 == b.0);
    let active: HashMap<PathBuf, Option<u64>> = par_map(projects, |(p, r): (PathBuf, Option<PathBuf>)| {
        let t = cold::last_active(ctx, &sig, &p, r.as_deref());
        (p, t)
    })
    .into_iter()
    .collect();
    let mut enclosing: Vec<PathBuf> = cwds
        .unwrap_or(&[])
        .iter()
        .map(|(_, d)| d.clone())
        .filter(|d| d.starts_with(&ctx.home) && *d != ctx.home)
        .collect();
    enclosing.sort();
    enclosing.dedup();
    enclosing.retain(|d| is_project_dir(d));
    let cutoff = ctx.now.saturating_sub(ctx.recent_days * DAY);
    for (i, c) in cands.iter_mut().enumerate() {
        if let Some(p) = &c.project {
            c.active = c.active.max(active.get(p).copied().flatten());
        }
        if c.blocked.is_some() {
            continue;
        }
        c.blocked = decide(ctx, c, facts.get(&i), worktrees.get(&i), cwds, &enclosing, cutoff).err();
    }
    // `git worktree remove` takes everything inside a worktree with it.
    let going: Vec<PathBuf> =
        cands.iter().filter(|c| c.how == How::Worktree && c.reclaimable()).map(|c| c.path.clone()).collect();
    for c in cands.iter_mut().filter(|c| c.how != How::Worktree && c.blocked.is_none()) {
        if going.iter().any(|w| c.path.starts_with(w)) {
            c.blocked = Some("goes with its worktree".into());
        }
    }
    size_all(cands);
    for c in cands.iter_mut().filter(|c| c.kind == "node_modules" && c.blocked.is_none()) {
        c.blocked = hand_edit(c).err();
    }
}

/// A file in node_modules newer than the install record was changed by hand (or by a tool
/// after the install): a rebuild would silently drop it.
fn hand_edit(c: &Cand) -> Result<(), String> {
    let installed = INSTALL_RECORDS
        .iter()
        .filter_map(|r| std::fs::symlink_metadata(c.path.join(r)).ok())
        .map(|m| m.mtime())
        .max()
        .unwrap_or(i64::MAX);
    match &c.newest_file {
        Some((t, f)) if *t > installed + 2 => {
            let rel = f.strip_prefix(&c.path).unwrap_or(f);
            Err(format!("a file changed after the install ({}): kept", rel.display()))
        }
        _ => Ok(()),
    }
}

fn days_ago(now: u64, t: u64) -> u64 {
    now.saturating_sub(t) / DAY
}

fn decide(
    ctx: &Ctx,
    c: &mut Cand,
    git: Option<&Option<GitFacts>>,
    worktree: Option<&Result<(), String>>,
    cwds: Option<&[(String, PathBuf)]>,
    enclosing: &[PathBuf],
    cutoff: u64,
) -> Result<(), String> {
    let name = c.path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let mut proof = vec![];
    if !c.machine {
        let facts = match git {
            Some(Some(f)) => Some(f),
            Some(None) => return Err("git could not check this folder".into()),
            None => None,
        };
        if facts.is_some_and(|f| f.tracked) {
            return Err("git tracks files inside it".into());
        }
        let ignored = facts.is_some_and(|f| f.ignored);
        if rules::is_maybe(&c.kind) && !ignored {
            return Err(format!("`{name}` can be real work; only cleaned when git ignores it"));
        }
        if let Some(r) = worktree {
            r.clone()?;
        }
        if c.kind == "venv" {
            let has = |d: &Path| {
                let n: Vec<String> = std::fs::read_dir(d)
                    .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
                    .unwrap_or_default();
                rules::PY_MANIFESTS.iter().any(|m| rules::matches(&n, m))
            };
            if !has(&c.base) && !c.project.as_deref().is_some_and(has) {
                return Err("virtualenv with no dependency list (requirements.txt, pyproject.toml…) beside it".into());
            }
            proof.push("a dependency list beside it".to_string());
        } else if let Some(r) = c.rule {
            proof.push(format!(
                "{} project ({})",
                r.name,
                r.markers.iter().find(|m| !m.contains('*')).unwrap_or(&r.markers[0])
            ));
        }
        if ignored {
            proof.push("git ignores it".into());
        } else if c.repo.is_some() {
            proof.push("nothing in it is tracked".into());
        }
    }
    if let Some(p) = &c.project {
        let Some(cwds) = cwds else {
            return Err("in use? lsof couldn't list running programs, so nothing in a project is touched".into());
        };
        if let Some(who) = busy_in(cwds, enclosing, p) {
            return Err(format!("in use: {who} is working in {}", p.display()));
        }
    }
    if c.recent_rule {
        match c.active {
            Some(t) if t > cutoff => {
                return Err(format!(
                    "used {} days ago (recent: the last {} days)",
                    days_ago(ctx.now, t),
                    ctx.recent_days
                ))
            }
            Some(t) => proof.push(format!("last used {} days ago", days_ago(ctx.now, t))),
            None => return Err("no sign of when it was last used".into()),
        }
    }
    if c.kind == "node_modules" && !INSTALL_RECORDS.iter().any(|r| std::fs::symlink_metadata(c.path.join(r)).is_ok()) {
        return Err(
            "no install record (node_modules/.package-lock.json) to tell hand edits from installed files".into()
        );
    }
    if !c.machine {
        let plan = rebuild::plan(c)?;
        proof.push(plan.describe());
        c.plan = Some(plan);
    }
    if !proof.is_empty() {
        let mut why = proof.join("; ");
        if !c.why.is_empty() {
            why = format!("{}; {why}", c.why);
        }
        c.why = why;
    }
    Ok(())
}

/// Agent worktree rule: clean, merged or with nothing unpushed, and every ignored file in it
/// rebuildable (`git worktree remove` deletes ignored files without asking).
fn worktree_ok(c: &Cand) -> Result<(), String> {
    let wt = &c.path;
    let ask = |args: &[&str]| {
        git::run(wt, args)
            .filter(proc::Out::ok)
            .map(|o| o.stdout)
            .ok_or_else(|| "git could not check this worktree".to_string())
    };
    if git::git_dir(wt).is_some_and(|g| g.join("locked").exists()) {
        return Err("the worktree is locked".into());
    }
    if !ask(&["status", "--porcelain", "-z", "--untracked-files=all", "--ignore-submodules=none"])?.is_empty() {
        return Err("the worktree has uncommitted or untracked changes".into());
    }
    for item in ask(&["ls-files", "-z", "--others", "--ignored", "--exclude-standard", "--directory"])?
        .split('\0')
        .filter(|s| !s.is_empty())
    {
        let p = wt.join(item.trim_end_matches('/'));
        let name = p.file_name().unwrap_or_default().to_string_lossy();
        if name == ".DS_Store" || rebuildable_dir(&p) {
            continue;
        }
        return Err(format!("ignored `{item}` in it that nothing can rebuild"));
    }
    let head = ask(&["rev-parse", "HEAD"])?;
    let main_head =
        git::run(&c.base, &["rev-parse", "HEAD"]).filter(proc::Out::ok).map(|o| o.stdout.trim().to_string());
    let merged = main_head
        .is_some_and(|m| git::run(wt, &["merge-base", "--is-ancestor", head.trim(), &m]).is_some_and(|o| o.ok()));
    let unpushed = ask(&["rev-list", "--count", "HEAD", "--not", "--remotes"])?.trim().parse::<u64>().unwrap_or(1);
    if !merged && unpushed > 0 {
        return Err(format!("{unpushed} commits not merged or pushed anywhere"));
    }
    Ok(())
}

/// A folder some rule here would rebuild (used for a worktree's ignored entries).
fn rebuildable_dir(p: &Path) -> bool {
    let Some(parent) = p.parent() else { return false };
    let name = p.file_name().unwrap_or_default().to_string_lossy();
    if std::fs::symlink_metadata(p.join("pyvenv.cfg")).is_ok() {
        return true;
    }
    let names: Vec<String> = std::fs::read_dir(parent)
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir()) && rules::rules_for(&names).any(|r| r.dirs.contains(&&*name))
}

// ---------------------------------------------------------------- entry points

pub struct Scan {
    pub cands: Vec<Cand>,
    /// Folders the walk could not read (Full Disk Access missing, usually).
    pub denied: usize,
}

/// Everything under `root` plus the machine-wide places, judged, largest first.
pub fn scan(ctx: &Ctx, root: &Path) -> Result<Scan, String> {
    let root = root.canonicalize().map_err(|e| format!("{}: {e}", root.display()))?;
    refuse(ctx, &root)?;
    let ((mut cands, denied), cwds) = std::thread::scope(|s| {
        let busy = s.spawn(cwds);
        let machine = s.spawn(|| if ctx.machine { crate::machine::discover(ctx, None) } else { vec![] });
        let (mut c, denied) = walk(ctx, &root);
        for m in machine.join().unwrap_or_default() {
            if !c.iter().any(|x| x.path == m.path) {
                c.push(m);
            }
        }
        ((c, denied), busy.join().ok().flatten())
    });
    judge(ctx, &mut cands, cwds.as_deref());
    cands.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    Ok(Scan { cands, denied })
}

/// The same candidate found and judged again from scratch, right before it is deleted
/// (THREAT-MODEL invariant 3). Errors say why it may no longer go.
pub fn recheck(ctx: &Ctx, c: &Cand) -> Result<Cand, String> {
    let mut fresh: Vec<Cand> = if c.machine {
        crate::machine::discover(ctx, Some(&c.kind))
            .into_iter()
            .filter(|x| x.path == c.path && x.how == c.how)
            .collect()
    } else {
        let dir = if c.kind == "venv" || c.how == How::Worktree { c.path.clone() } else { c.base.clone() };
        let repo = git::toplevel(&dir);
        let entries = sys::list_path(&dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        let mut out = vec![];
        visit(ctx, &dir, &entries, repo.as_deref(), repo.as_deref(), 1, &mut out);
        out.retain(|x| x.path == c.path && x.how == c.how);
        out
    };
    let Some(mut f) = fresh.pop() else { return Err("no longer matches a rule".into()) };
    judge(ctx, std::slice::from_mut(&mut f), cwds().as_deref());
    if matches!(f.how, How::Tree | How::Worktree) && (f.dev, f.ino) != (c.dev, c.ino) {
        return Err("the folder changed since the scan".into());
    }
    match f.blocked.take() {
        Some(why) => Err(why),
        None => Ok(f),
    }
}

#[cfg(test)]
mod tests;
