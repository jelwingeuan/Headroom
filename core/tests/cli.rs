//! The CLI contract (judge/CONTRACT.md), end to end through the real binary: JSON on stdout,
//! exit 0, state only in HEADROOM_HOME, and scan/clean/autopilot under the judge's
//! deny-network sandbox.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const PROFILE: &str = "(version 1)(allow default)(deny network*)(allow network* (local unix))";
const OLD: &str = "@1600000000 +0000";

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("headroom-it-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p.canonicalize().unwrap()
}

fn sh(dir: &Path, script: &str) {
    let ok = Command::new("/bin/sh")
        .args(["-c", script])
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "T")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_NAME", "T")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .env("GIT_AUTHOR_DATE", OLD)
        .env("GIT_COMMITTER_DATE", OLD)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()
        .unwrap()
        .success();
    assert!(ok, "{script}");
}

struct World {
    home: PathBuf,
    state: PathBuf,
    bin: PathBuf,
    root: PathBuf,
}

/// One cold Node project with a fake `npm` on PATH that "installs" by writing the folder.
fn world(name: &str) -> World {
    let t = tmp(name);
    let (home, state, bin) = (t.join("home"), t.join("state"), t.join("bin"));
    let root = home.join("world");
    let p = root.join("app");
    for d in [&state, &bin, &p] {
        std::fs::create_dir_all(d).unwrap();
    }
    sh(&p, "echo '{\"name\":\"app\"}' > package.json && echo '{}' > package-lock.json && echo node_modules/ > .gitignore && git init -q && git add -A && git commit -qm init");
    let install = "mkdir -p node_modules/left-pad && echo x > node_modules/left-pad/index.js && echo '{}' > node_modules/.package-lock.json && touch -t 202001010000 node_modules/left-pad/index.js node_modules/.package-lock.json";
    sh(&p, install);
    std::fs::write(bin.join("npm"), format!("#!/bin/sh\n{install}\n")).unwrap();
    sh(&bin, "chmod +x npm");
    World { home, state, bin, root }
}

fn run(w: &World, args: &[&str], sandbox: bool) -> Output {
    let exe = env!("CARGO_BIN_EXE_headroom");
    let mut c = if sandbox {
        let mut c = Command::new("/usr/bin/sandbox-exec");
        c.args(["-p", PROFILE, exe]);
        c
    } else {
        Command::new(exe)
    };
    c.args(args)
        .env("HOME", &w.home)
        .env("HEADROOM_HOME", &w.state)
        .env("PATH", format!("{}:/usr/bin:/bin", w.bin.display()))
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn json(o: &Output) -> headroom_core::json::Json {
    assert!(o.status.success(), "exit {:?}: {}", o.status.code(), String::from_utf8_lossy(&o.stderr));
    headroom_core::json::Json::parse(String::from_utf8_lossy(&o.stdout).trim()).expect("stdout is only JSON")
}

#[test]
fn scan_clean_rebuild_follow_the_contract_without_network() {
    let w = world("contract");
    let r = w.root.to_string_lossy().into_owned();
    let nm = w.root.join("app/node_modules");
    let items = json(&run(&w, &["scan", &r, "--json"], true));
    let item = items.arr().iter().find(|i| i.s("path") == nm.to_string_lossy()).expect("node_modules listed");
    for k in ["path", "kind", "bytes", "reclaimable", "reason"] {
        assert!(item.get(k).is_some(), "{k} missing");
    }
    assert_eq!(item.get("reclaimable"), Some(&headroom_core::json::Json::Bool(true)), "{item}");
    assert!(nm.exists(), "scan changes nothing");

    let out = json(&run(&w, &["clean", &r, "--apply", "--json"], true));
    assert!(out.get("freed_bytes").and_then(|b| b.u64()).unwrap() > 0, "{out}");
    let a = &out.get("actions").unwrap().arr()[0];
    assert_eq!((a.s("path"), a.s("kind")), (&*nm.to_string_lossy(), "node_modules"));
    assert!(!nm.exists());
    assert!(w.state.join("history.jsonl").is_file(), "state lives in HEADROOM_HOME");

    json(&run(&w, &["rebuild", "--all"], false));
    assert!(nm.join("left-pad/index.js").exists());
    // Nothing left to undo: undo --all succeeds and changes nothing.
    json(&run(&w, &["undo", "--all"], false));
}

#[test]
fn autopilot_prints_json_lines_and_dry_runs_its_first_week() {
    let w = world("autopilot");
    let budget = (1u64 << 62).to_string();
    let o = run(
        &w,
        &["autopilot", "--once", "--now", "1800000000", "--budget-bytes", &budget, "--recent-days", "14"],
        true,
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = String::from_utf8_lossy(&o.stdout);
    let events: Vec<_> =
        text.lines().map(|l| headroom_core::json::Json::parse(l).expect("each line is JSON")).collect();
    let kinds: Vec<&str> = events.iter().map(|e| e.s("event")).collect();
    assert_eq!(kinds.first(), Some(&"tick"));
    assert!(kinds.contains(&"would_reclaim") && !kinds.contains(&"reclaim"), "{kinds:?}");
    assert!(w.root.join("app/node_modules").exists());
    // A week later the same budget reclaims for real and says what's left.
    let o = run(
        &w,
        &["autopilot", "--once", "--now", "1800700000", "--budget-bytes", &budget, "--recent-days", "14"],
        true,
    );
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.contains(r#""event":"reclaim""#), "{text}");
    assert!(text.contains(r#""event":"shortfall""#) && text.contains(r#""eligible_bytes":0"#), "{text}");
    json(&run(&w, &["undo", "--all"], false));
    assert!(w.root.join("app/node_modules/left-pad/index.js").exists());
}

#[test]
fn mcp_answers_on_stdio() {
    let w = world("mcp");
    let mut c = Command::new(env!("CARGO_BIN_EXE_headroom"));
    c.arg("mcp").current_dir(w.root.join("app")).env("HOME", &w.home).env("HEADROOM_HOME", &w.state);
    let mut child = c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
    {
        use std::io::Write;
        let mut i = child.stdin.take().unwrap();
        writeln!(i, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2025-06-18"}}}}"#)
            .unwrap();
        writeln!(i, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
        writeln!(i, r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"scan","arguments":{{}}}}}}"#)
            .unwrap();
    }
    let o = child.wait_with_output().unwrap();
    let lines: Vec<String> = String::from_utf8_lossy(&o.stdout).lines().map(String::from).collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].contains(r#""serverInfo":{"name":"headroom""#));
    assert!(lines[1].contains("node_modules"));
}

#[test]
fn agent_idles_until_terminated() {
    let w = world("agent");
    let exe = env!("CARGO_BIN_EXE_headroom");
    let mut child = Command::new("/usr/bin/sandbox-exec")
        .args(["-p", PROFILE, exe, "agent", "--idle"])
        .env("HOME", &w.home)
        .env("HEADROOM_HOME", &w.state)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert!(child.try_wait().unwrap().is_none(), "the agent must stay alive");
    let ps = Command::new("/bin/ps").args(["-o", "time=", "-p", &child.id().to_string()]).output().unwrap();
    let _ = child.kill();
    let _ = child.wait();
    // CPU time used in 3 s of idling, "M:SS.ss": well under a tenth of a second.
    let t = String::from_utf8_lossy(&ps.stdout).trim().to_string();
    let secs: f64 = t.rsplit(':').next().unwrap_or("9").parse().unwrap_or(9.0);
    assert!(secs < 0.3, "idle agent used {t} of CPU");
}
