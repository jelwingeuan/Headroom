//! Running other programs: argument arrays only (never a shell), stdin closed, a time limit,
//! and programs found on PATH only in absolute folders outside the project being worked on.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub struct Out {
    pub code: Option<i32>,
    pub stdout: String,
}

impl Out {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }
}

/// Runs `cmd` to completion or until `secs` pass (then kills it). stderr is dropped.
pub fn run(mut cmd: Command, secs: u64) -> Option<Out> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = cmd.spawn().ok()?;
    let mut so = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = so.read_to_end(&mut v);
        v
    });
    let end = Instant::now() + Duration::from_secs(secs);
    let mut nap = Duration::from_millis(2);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if Instant::now() < end => {
                std::thread::sleep(nap);
                nap = (nap * 2).min(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let bytes = reader.join().unwrap_or_default();
    Some(Out { code: status.and_then(|s| s.code()), stdout: String::from_utf8_lossy(&bytes).into_owned() })
}

/// `prog` resolved on `path_var`, skipping relative folders and folders inside `avoid`
/// (so a project can't plant its own `npm` or `docker` in front of the real one).
pub fn which(prog: &str, path_var: &str, avoid: Option<&Path>) -> Option<PathBuf> {
    if prog.contains('/') {
        return None;
    }
    path_var
        .split(':')
        .map(Path::new)
        .filter(|d| d.is_absolute() && !avoid.is_some_and(|a| d.starts_with(a)))
        .map(|d| d.join(prog))
        .find(|p| is_executable(p))
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Every running program's command line (`ps -axo command=`).
pub fn processes() -> Vec<String> {
    let mut c = Command::new("/bin/ps");
    c.args(["-axo", "command="]);
    run(c, 10).map(|o| o.stdout.lines().map(|l| l.trim().to_string()).collect()).unwrap_or_default()
}

/// The program name of a `ps` command line: "/Applications/Cursor.app/Contents/MacOS/Cursor -x" -> "Cursor".
pub fn program(line: &str) -> &str {
    let exe = match line.split_once(".app/") {
        Some((app, _)) => app,
        None => line.split_whitespace().next().unwrap_or(""),
    };
    exe.rsplit('/').next().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_skips_relative_and_project_folders() {
        let t = crate::testutil::tmp("which");
        let planted = t.join("proj/node_modules/.bin");
        std::fs::create_dir_all(&planted).unwrap();
        crate::testutil::script(&planted.join("git"), "exit 0");
        let path = format!("node_modules/.bin:{}:/usr/bin", planted.display());
        assert_eq!(which("git", &path, Some(&t.join("proj"))), Some(PathBuf::from("/usr/bin/git")));
        assert_eq!(which("git", "node_modules/.bin", None), None);
    }

    #[test]
    fn run_times_out() {
        let mut c = Command::new("/bin/sleep");
        c.arg("5");
        let t = Instant::now();
        let o = run(c, 1).unwrap();
        assert!(o.code.is_none());
        assert!(t.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn program_names() {
        assert_eq!(program("/Applications/Cursor.app/Contents/MacOS/Cursor --type=gpu"), "Cursor");
        assert_eq!(program("node server.js"), "node");
        assert_eq!(program("/usr/local/bin/claude"), "claude");
    }
}
