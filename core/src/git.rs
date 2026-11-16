//! Git, called so that it can never run the repository's own code (THREAT-MODEL invariant 2):
//! an absolute git binary, an argument array, no hooks, no fsmonitor, no filters defined by
//! the repository's config, no optional locks (so `.git/index` is never rewritten), stdin closed.

use crate::proc::{self, Out};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The real git: the developer tools' copy resolved through xcrun, else /usr/bin/git.
pub fn git_path() -> &'static Path {
    static GIT: OnceLock<PathBuf> = OnceLock::new();
    GIT.get_or_init(|| {
        let mut c = Command::new("/usr/bin/xcrun");
        c.args(["-f", "git"]);
        proc::run(c, 20)
            .filter(Out::ok)
            .map(|o| PathBuf::from(o.stdout.trim()))
            .filter(|p| p.is_absolute() && p.is_file())
            .unwrap_or_else(|| PathBuf::from("/usr/bin/git"))
    })
}

/// A git command for `dir`, hardened as above. Filters named in the repo's own config are
/// switched off; the user's global ones (git-lfs) are theirs and stay.
pub fn command(dir: &Path) -> Command {
    let mut c = Command::new(git_path());
    c.args(["--no-pager", "-c", "core.fsmonitor=false", "-c", "core.hooksPath=/dev/null"]);
    c.args(["-c", "log.showSignature=false", "-c", "core.untrackedCache=false", "-c", "protocol.allow=never"]);
    for name in local_filters(dir) {
        for key in ["clean", "smudge", "process"] {
            c.arg("-c").arg(format!("filter.{name}.{key}="));
        }
    }
    c.arg("-C").arg(dir);
    c.env("GIT_OPTIONAL_LOCKS", "0").env("GIT_TERMINAL_PROMPT", "0");
    for v in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_CEILING_DIRECTORIES",
        "GIT_LITERAL_PATHSPECS",
    ] {
        c.env_remove(v);
    }
    c
}

pub fn run(dir: &Path, args: &[&str]) -> Option<Out> {
    let mut c = command(dir);
    c.args(args);
    proc::run(c, 120)
}

/// The git directory of a work tree: `<dir>/.git`, or where a worktree's `.git` file points.
pub fn git_dir(top: &Path) -> Option<PathBuf> {
    let dot = top.join(".git");
    let m = std::fs::symlink_metadata(&dot).ok()?;
    if m.is_dir() {
        return Some(dot);
    }
    let text = std::fs::read_to_string(&dot).ok()?;
    let p = PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim());
    Some(if p.is_absolute() { p } else { top.join(p) })
}

/// `[filter "x"]` sections in the repository's own config files.
fn local_filters(dir: &Path) -> Vec<String> {
    let Some(top) = toplevel(dir) else { return vec![] };
    let Some(gd) = git_dir(&top) else { return vec![] };
    let mut configs = vec![gd.join("config"), gd.join("config.worktree")];
    if let Ok(common) = std::fs::read_to_string(gd.join("commondir")) {
        configs.push(gd.join(common.trim()).join("config"));
    }
    let mut names = vec![];
    for f in configs {
        for line in std::fs::read_to_string(f).unwrap_or_default().lines() {
            let l = line.trim();
            if let Some(rest) = l.strip_prefix("[filter") {
                let name = rest.trim().trim_end_matches(']').trim().trim_matches('"');
                if !name.is_empty() && !names.iter().any(|n| n == name) {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

/// The nearest folder at or above `p` that holds a `.git` (folder or worktree file).
pub fn toplevel(p: &Path) -> Option<PathBuf> {
    p.ancestors().find(|a| std::fs::symlink_metadata(a.join(".git")).is_ok()).map(Path::to_path_buf)
}

/// The time of the last thing that happened in this checkout: the newest reflog entry of
/// HEAD (commits, checkouts, merges), else the HEAD commit's date.
pub fn last_activity(top: &Path) -> Option<u64> {
    let gd = git_dir(top)?;
    if let Ok(log) = std::fs::read_to_string(gd.join("logs/HEAD")) {
        let newest = log.lines().filter_map(reflog_time).max();
        if newest.is_some() {
            return newest;
        }
    }
    run(top, &["log", "-1", "--format=%ct", "HEAD"]).filter(Out::ok).and_then(|o| o.stdout.trim().parse().ok())
}

/// "<old> <new> Name <mail> 1700000000 +0100\tmessage" -> 1700000000.
fn reflog_time(line: &str) -> Option<u64> {
    let head = line.split('\t').next()?;
    let mut parts = head.rsplitn(3, ' ');
    let _tz = parts.next()?;
    parts.next()?.parse().ok()
}

/// Of `rels` (paths relative to `top`), the ones with at least one tracked file inside, and
/// the ones git ignores. `None` when git could not answer.
pub fn tracked_and_ignored(top: &Path, rels: &[String]) -> Option<(Vec<bool>, Vec<bool>)> {
    let mut tracked = vec![false; rels.len()];
    let mut ignored = vec![false; rels.len()];
    for (chunk_i, chunk) in rels.chunks(200).enumerate() {
        let base = chunk_i * 200;
        // Literal: a folder named ":(top)x" is a folder, not pathspec magic. (check-ignore
        // refuses literal mode, and refuses magic too, so it fails safe below.)
        let mut args = vec!["--literal-pathspecs", "ls-files", "-z", "--"];
        args.extend(chunk.iter().map(String::as_str));
        let o = run(top, &args).filter(Out::ok)?;
        for f in o.stdout.split('\0').filter(|f| !f.is_empty()) {
            for (i, r) in chunk.iter().enumerate() {
                if f == r || (f.starts_with(r.as_str()) && f.as_bytes().get(r.len()) == Some(&b'/')) {
                    tracked[base + i] = true;
                }
            }
        }
        // check-ignore takes -z only with --stdin, and stdin stays closed: one path per line.
        let mut args = vec!["-c", "core.quotePath=false", "check-ignore", "--"];
        args.extend(chunk.iter().map(String::as_str));
        let o = run(top, &args)?;
        if !matches!(o.code, Some(0 | 1)) {
            return None;
        }
        for f in o.stdout.lines().filter(|f| !f.is_empty()) {
            if let Some(i) = chunk.iter().position(|r| r == f.trim_end_matches('/')) {
                ignored[base + i] = true;
            }
        }
    }
    Some((tracked, ignored))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{git, tmp};

    #[test]
    fn reads_reflog_time() {
        let l = "0000 1111 Judge <j@x> 1700000123 +0000\tcommit (initial): Initial commit";
        assert_eq!(reflog_time(l), Some(1_700_000_123));
    }

    #[test]
    fn never_runs_repo_hooks_fsmonitor_or_filters() {
        let t = tmp("git-hardened");
        let repo = t.join("r");
        std::fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q"], None);
        let mark = t.join("ran");
        let evil = t.join("evil.sh");
        crate::testutil::script(&evil, &format!("touch '{}'; cat", mark.display()));
        let cfg = repo.join(".git/config");
        let mut text = std::fs::read_to_string(&cfg).unwrap();
        text += &format!("[core]\n\tfsmonitor = {0}\n[filter \"x\"]\n\tclean = {0}\n\tsmudge = {0}\n", evil.display());
        std::fs::write(&cfg, text).unwrap();
        std::fs::write(repo.join(".gitattributes"), "* filter=x\n").unwrap();
        std::fs::write(repo.join("a.txt"), "a").unwrap();
        // Planted hooks too: none of the calls below may run anything.
        std::fs::create_dir_all(repo.join(".git/hooks")).unwrap();
        crate::testutil::script(&repo.join(".git/hooks/post-checkout"), &format!("touch '{}'", mark.display()));
        let (tracked, _) = tracked_and_ignored(&repo, &["a.txt".into()]).unwrap();
        assert_eq!(tracked, [false]);
        run(&repo, &["status", "--porcelain"]).unwrap();
        assert!(!mark.exists(), "git ran the repository's own code");
        assert!(git_path().is_absolute());
    }

    #[test]
    fn tracked_and_ignored_paths() {
        let t = tmp("git-ti");
        git(&t, &["init", "-q"], None);
        std::fs::write(t.join(".gitignore"), "build/\n").unwrap();
        for d in ["build", "dist", "vendor"] {
            std::fs::create_dir(t.join(d)).unwrap();
            std::fs::write(t.join(d).join("f"), "x").unwrap();
        }
        git(&t, &["add", ".gitignore", "vendor"], None);
        git(&t, &["commit", "-qm", "c"], Some(1_600_000_000));
        let rels = ["build", "dist", "vendor"].map(String::from);
        let (tracked, ignored) = tracked_and_ignored(&t, &rels).unwrap();
        assert_eq!(tracked, [false, false, true]);
        assert_eq!(ignored, [true, false, false]);
        assert_eq!(last_activity(&t), Some(1_600_000_000));
    }
}
