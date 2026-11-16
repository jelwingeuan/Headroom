//! Test helpers: temp folders, a Ctx around them, git with fixed dates, tiny scripts.

use crate::state::{wall_clock, Ctx};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static N: AtomicUsize = AtomicUsize::new(0);

pub fn tmp(name: &str) -> PathBuf {
    let n = N.fetch_add(1, Ordering::SeqCst);
    let p = std::env::temp_dir().join(format!("headroom-test-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p.canonicalize().unwrap()
}

/// A Ctx with its own home and HEADROOM_HOME; projects go in `<t>/world` (see `world`).
pub fn ctx(name: &str) -> Ctx {
    let t = tmp(name);
    for d in ["home", "state", "world"] {
        std::fs::create_dir_all(t.join(d)).unwrap();
    }
    Ctx {
        home: t.join("home"),
        state: t.join("state"),
        now: wall_clock(),
        recent_days: 14,
        path_var: "/usr/bin:/bin".into(),
        machine: false,
    }
}

pub fn world(ctx: &Ctx) -> PathBuf {
    ctx.home.parent().unwrap().join("world")
}

pub fn git(dir: &Path, args: &[&str], when: Option<u64>) {
    let mut c = Command::new("/usr/bin/git");
    c.args(args).current_dir(dir);
    for (k, v) in [
        ("GIT_AUTHOR_NAME", "T"),
        ("GIT_AUTHOR_EMAIL", "t@example.invalid"),
        ("GIT_COMMITTER_NAME", "T"),
        ("GIT_COMMITTER_EMAIL", "t@example.invalid"),
    ] {
        c.env(k, v);
    }
    c.env("GIT_CONFIG_GLOBAL", "/dev/null").env("GIT_CONFIG_NOSYSTEM", "1");
    if let Some(t) = when {
        c.env("GIT_AUTHOR_DATE", format!("@{t} +0000")).env("GIT_COMMITTER_DATE", format!("@{t} +0000"));
    }
    let o = c.output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
}

pub fn script(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

pub fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// Sets a file's (or folder's) access and modification times.
pub fn touch(path: &Path, t: u64) {
    let when = std::time::UNIX_EPOCH + std::time::Duration::from_secs(t);
    let f = std::fs::File::open(path).unwrap();
    f.set_times(std::fs::FileTimes::new().set_accessed(when).set_modified(when)).unwrap();
}
