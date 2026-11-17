//! Machine-wide places, each with what it must never delete.

use super::*;
use crate::scan::judge;
use crate::testutil::{ctx, git, script, touch, write};

const OLD: u64 = 1_600_000_000;

fn digest(n: u8) -> String {
    format!("sha256:{}", format!("{n:02x}").repeat(32))
}

fn ollama_model(home: &Path, name: &str, layers: &[(u8, &str)], config: u8, from: Option<&str>) {
    let root = home.join(".ollama/models");
    let ls: Vec<String> = layers
        .iter()
        .map(|(d, mt)| {
            let from = from.map(|f| format!(r#","from":"{f}""#)).unwrap_or_default();
            format!(r#"{{"mediaType":"application/vnd.ollama.image.{mt}","digest":"{}","size":4{from}}}"#, digest(*d))
        })
        .collect();
    let m = format!(r#"{{"schemaVersion":2,"config":{{"digest":"{}"}},"layers":[{}]}}"#, digest(config), ls.join(","));
    write(&root.join(format!("manifests/registry.ollama.ai/library/{name}/latest")), &m);
    for d in layers.iter().map(|l| l.0).chain([config]) {
        let p = root.join("blobs").join(digest(d).replace(':', "-"));
        if !p.exists() {
            write(&p, &"x".repeat(5000));
            touch(&p, OLD);
        }
    }
    touch(&root.join(format!("manifests/registry.ollama.ai/library/{name}/latest")), OLD);
}

#[test]
fn ollama_keeps_created_models_and_every_blob_they_share() {
    let mut ctx = ctx("m-ollama");
    let h = ctx.home.clone();
    // A pulled model and an `ollama create` model on top of it (same weights and license).
    ollama_model(&h, "all-minilm", &[(1, "model"), (2, "license"), (3, "params")], 4, None);
    ollama_model(&h, "judge-custom", &[(1, "model"), (2, "license"), (5, "params")], 6, None);
    // A pulled model sharing nothing but the license blob: it may go, without that blob.
    ollama_model(&h, "tinyllama", &[(7, "model"), (2, "license")], 8, None);
    // A library name built locally from something else.
    ollama_model(&h, "llama3", &[(9, "model")], 10, Some("/Users/me/llama.gguf"));
    ctx.machine = true;
    let mut c = discover(&ctx, Some("ollama_model"));
    judge(&ctx, &mut c, Some(&[]));
    let by = |n: &str| c.iter().find(|x| x.path.to_string_lossy().contains(&format!("/{n}/"))).unwrap();
    assert!(by("judge-custom").blocked.as_deref().unwrap().contains("not a known registry model"));
    assert!(by("all-minilm").blocked.as_deref().unwrap().contains("shares its weights with judge-custom"));
    assert!(by("llama3").blocked.as_deref().unwrap().contains("built locally"));
    let t = by("tinyllama");
    assert!(t.reclaimable(), "{}", t.reason());
    let How::Files(files) = &t.how else { panic!() };
    let names: Vec<String> = files.iter().map(|f| f.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(names.len(), 3, "{names:?}"); // manifest + its own weights + its own config
    assert!(!names.contains(&digest(2).replace(':', "-")), "the shared license blob stays");
    assert_eq!(
        t.plan.as_ref().unwrap().steps,
        vec![vec!["ollama".to_string(), "pull".into(), "tinyllama:latest".into()]]
    );
}

#[test]
fn an_unreadable_ollama_manifest_blocks_every_model() {
    let mut ctx = ctx("m-ollama-bad");
    ollama_model(&ctx.home, "tinyllama", &[(7, "model"), (2, "license")], 8, None);
    let mystery = ctx.home.join(".ollama/models/manifests/registry.ollama.ai/library/mystery/latest");
    ctx.machine = true;
    let mut c = discover(&ctx, Some("ollama_model"));
    judge(&ctx, &mut c, Some(&[]));
    assert!(c[0].reclaimable(), "{}", c[0].reason());
    // A manifest Headroom can't parse may reference tinyllama's blobs: touch nothing.
    write(&mystery, "{not json");
    let mut c = discover(&ctx, Some("ollama_model"));
    judge(&ctx, &mut c, Some(&[]));
    assert_eq!(c.len(), 1);
    assert!(c[0].blocked.as_deref().unwrap_or("").contains("can't read the Ollama manifest"), "{:?}", c[0].blocked);
    // A digest in an unexpected form is just as unreadable.
    write(&mystery, r#"{"layers":[{"digest":"md5:abc"}]}"#);
    let mut c = discover(&ctx, Some("ollama_model"));
    judge(&ctx, &mut c, Some(&[]));
    assert!(c[0].blocked.as_deref().unwrap_or("").contains("can't read"), "{:?}", c[0].blocked);
}

#[test]
fn docker_only_runs_allowed_commands_and_never_touches_volumes() {
    let mut ctx = ctx("m-docker");
    let bin = ctx.home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = ctx.home.join("docker.log");
    script(
        &bin.join("docker"),
        &format!(
            r#"echo "$@" >> '{}'
case "$1 $2" in
  "images --digests") echo '{{"Repository":"judge/api","Tag":"latest","ID":"4f1a2b3c4d5e","Size":"1.21GB"}}'; echo '{{"Repository":"<none>","Tag":"<none>","ID":"9e8d7c6b5a4f","Size":"845MB"}}';;
  "system df") echo '{{"Type":"Local Volumes","Reclaimable":"3.2GB (100%)"}}'; echo '{{"Type":"Build Cache","Reclaimable":"1.5GB"}}';;
esac"#,
            log.display()
        ),
    );
    ctx.path_var = format!("{}:/usr/bin:/bin", bin.display());
    ctx.machine = true;
    let mut c = discover(&ctx, Some("docker_images"));
    judge(&ctx, &mut c, Some(&[]));
    assert_eq!(c.len(), 2);
    assert_eq!(c[0].bytes, 845_000_000);
    assert_eq!(c[1].bytes, 1_500_000_000);
    assert!(c.iter().all(|x| !x.frees_host && x.reclaimable()));
    for x in &c {
        crate::reclaim::reclaim(&ctx, x, "clean").unwrap();
    }
    let calls = std::fs::read_to_string(&log).unwrap();
    assert!(calls.contains("image rm 9e8d7c6b5a4f\n"), "{calls}");
    assert!(calls.contains("builder prune -f\n"), "{calls}");
    assert!(!calls.contains("volume") && !calls.contains("system prune") && !calls.contains("4f1a2b3c4d5e"), "{calls}");
}

#[test]
fn derived_data_follows_its_projects_activity() {
    let mut ctx = ctx("m-derived");
    ctx.machine = true;
    let dd = ctx.home.join("Library/Developer/Xcode/DerivedData");
    // Like the judge's world: the folder is named after the package folder (swift-01), the
    // scheme is the package's (SwiftAlpha); Xcode's build log index says which scheme built it.
    let mk = |folder: &str, scheme: &str, when: u64| {
        let proj = ctx.home.join("code").join(folder);
        write(&proj.join("Package.swift"), "// swift-tools-version:5.9\n");
        git(&proj, &["init", "-q"], Some(when));
        git(&proj, &["add", "-A"], Some(when));
        git(&proj, &["commit", "-qm", "c"], Some(when));
        let d = dd.join(format!("{folder}-abcdefghijkl"));
        write(
            &d.join("info.plist"),
            &format!("<plist><dict><key>WorkspacePath</key>\n<string>{}</string></dict></plist>", proj.display()),
        );
        let log = |id: &str, name: &str, stop: f64| {
            format!("<key>{id}</key><dict><key>schemeIdentifier-schemeName</key><string>{name}</string><key>timeStoppedRecording</key><real>{stop}</real></dict>")
        };
        write(
            &d.join("Logs/Build/LogStoreManifest.plist"),
            &format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\"><dict><key>logs</key><dict>{}{}</dict></dict></plist>",
                log("A", "OldScheme", 1000.0),
                log("B", scheme, 2000.0)
            ),
        );
        std::fs::create_dir_all(d.join("Build/Products/Debug")).unwrap();
        (proj, d)
    };
    let (cold_proj, cold) = mk("swift-01", "SwiftAlpha", OLD);
    let (_, warm) = mk("swift-02", "SwiftBravo", ctx.now - 2 * DAY);
    std::fs::create_dir_all(dd.join("ModuleCache.noindex")).unwrap();
    let mut c = discover(&ctx, Some("DerivedData"));
    judge(&ctx, &mut c, Some(&[]));
    assert_eq!(c.len(), 2, "shared ModuleCache.noindex is nobody's to delete");
    let a = c.iter().find(|x| x.path == cold).unwrap();
    assert!(a.reclaimable(), "{}", a.reason());
    let plan = a.plan.as_ref().unwrap();
    assert_eq!(plan.cwd, cold_proj);
    assert_eq!(plan.steps[0].join(" "), "xcodebuild -scheme SwiftAlpha -destination platform=macOS -quiet build");
    let b = c.iter().find(|x| x.path == warm).unwrap();
    assert!(b.blocked.as_deref().unwrap().contains("days ago"));
}

#[test]
fn hugging_face_comes_back_with_its_exact_revision_files_and_refs() {
    let mut ctx = ctx("m-hf");
    ctx.machine = true;
    let repo = ctx.home.join(".cache/huggingface/hub/models--org--tiny");
    write(&repo.join("refs/main"), "abc123\n");
    write(&repo.join("blobs/b1"), "weights");
    std::fs::create_dir_all(repo.join("snapshots/abc123")).unwrap();
    std::os::unix::fs::symlink("../../blobs/b1", repo.join("snapshots/abc123/model.bin")).unwrap();
    // Old like the judge's fixture: every entry, symlinks themselves included (`touch -h`).
    let ok = std::process::Command::new("/usr/bin/find")
        .arg(&repo)
        .args(["-exec", "/usr/bin/touch", "-h", "-t", "202009130000", "{}", "+"])
        .status()
        .unwrap();
    assert!(ok.success());
    // Without python3 there is no way back: kept.
    ctx.path_var = "/nonexistent".into();
    let mut c = discover(&ctx, Some("hf_model"));
    judge(&ctx, &mut c, Some(&[]));
    assert!(c[0].blocked.as_deref().unwrap().contains("python3"));
    // No hf CLI needed (the judge's world has none): python3 is enough. Newer huggingface_hub
    // links blobs into a store shared by every repo (hub/blobs/xx/…), written just now: that
    // store isn't this folder, so its times don't make the snapshot look used.
    let store = ctx.home.join(".cache/huggingface/hub/blobs/f0/f094");
    write(&store, "shared content");
    std::os::unix::fs::symlink(&store, repo.join("blobs/b2")).unwrap();
    let ok = std::process::Command::new("/usr/bin/touch")
        .args(["-h", "-t", "202009130000"])
        .args([repo.join("blobs/b2"), repo.join("blobs")])
        .status();
    assert!(ok.unwrap().success());
    ctx.path_var = "/usr/bin:/bin".into();
    let mut c = discover(&ctx, Some("hf_model"));
    judge(&ctx, &mut c, Some(&[]));
    assert!(c[0].reclaimable(), "{}", c[0].reason());
    let steps = &c[0].plan.as_ref().unwrap().steps;
    assert_eq!(steps[0][..3], ["python3", "-m", "venv"]);
    let last = &steps[2];
    assert_eq!(last[3..6], ["org/tiny", "model", &*ctx.home.join(".cache/huggingface/hub").to_string_lossy()]);
    assert_eq!(last[6..], ["abc123", r#"{"main":"abc123"}"#, "model.bin"]);
}

#[test]
fn ai_history_is_never_reclaimable_and_never_deletable() {
    let mut ctx = ctx("m-history");
    ctx.machine = true;
    write(&ctx.home.join(".claude/projects/x/a.jsonl"), "{}\n");
    write(&ctx.home.join(".codex/sessions/a.jsonl"), "{}\n");
    for v in ["2.1.8", "2.1.9", "2.1.10", "2.1.11"] {
        write(&ctx.home.join(".local/share/claude/versions").join(v), "binary");
    }
    // The launcher points at 2.1.9 (not the newest): both it and 2.1.11 stay.
    std::fs::create_dir_all(ctx.home.join(".local/bin")).unwrap();
    std::os::unix::fs::symlink(ctx.home.join(".local/share/claude/versions/2.1.9"), ctx.home.join(".local/bin/claude"))
        .unwrap();
    let mut c = discover(&ctx, Some("ai_history"));
    judge(&ctx, &mut c, Some(&[]));
    let hist: Vec<&Cand> = c.iter().filter(|x| x.kind == "ai_history").collect();
    assert_eq!(hist.len(), 2);
    assert!(hist.iter().all(|x| !x.reclaimable()));
    for x in &hist {
        assert!(crate::reclaim::reclaim(&ctx, x, "clean").is_err());
    }
    assert!(ctx.home.join(".claude/projects/x/a.jsonl").exists());
    let old: Vec<String> = c
        .iter()
        .filter(|x| x.kind == "claude_code_old")
        .map(|x| x.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(old, ["2.1.8", "2.1.10"], "the current and the newest versions stay");
}

#[test]
fn archive_dates_and_docker_sizes() {
    assert_eq!(date_secs("1970-01-02"), Some(86_400));
    assert_eq!(date_secs("2024-03-09"), Some(1_709_942_400));
    assert_eq!(date_secs("nope"), None);
    assert_eq!(docker_size("1.21GB"), 1_210_000_000);
    assert_eq!(docker_size("1.5GB (100%)"), 1_500_000_000);
    assert_eq!(docker_size("0B"), 0);
    assert_eq!(docker_size("12.5kB"), 12_500);
}

#[test]
fn xcode_leftovers_keep_the_newest_device_support_and_recent_archives() {
    let mut ctx = ctx("m-xcode");
    ctx.machine = true;
    let x = ctx.home.join("Library/Developer/Xcode");
    for (v, t) in [("17.0 (21A1)", OLD), ("18.0 (22A1)", OLD + DAY)] {
        std::fs::create_dir_all(x.join("iOS DeviceSupport").join(v)).unwrap();
        touch(&x.join("iOS DeviceSupport").join(v), t);
    }
    std::fs::create_dir_all(x.join("Archives/2020-01-01/Old.xcarchive")).unwrap();
    // Dated in the future: certainly newer than six months.
    std::fs::create_dir_all(x.join("Archives/2999-01-01/New.xcarchive")).unwrap();
    let mut c = discover(&ctx, Some("device_support"));
    judge(&ctx, &mut c, Some(&[]));
    let names: Vec<String> = c.iter().map(|x| x.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert!(names.contains(&"17.0 (21A1)".to_string()) && !names.contains(&"18.0 (22A1)".to_string()), "{names:?}");
    assert!(names.contains(&"Old.xcarchive".to_string()) && !names.contains(&"New.xcarchive".to_string()), "{names:?}");
    // Listed, but an archive can't be rebuilt: never reclaimable.
    let old = c.iter().find(|x| x.path.ends_with("Old.xcarchive")).unwrap();
    assert!(old.blocked.as_deref().unwrap_or("").contains("archives can't be rebuilt"), "{:?}", old.blocked);
    assert!(crate::reclaim::reclaim(&ctx, old, "clean").is_err());
    assert!(old.path.exists());
}

fn sim(udid: &str, name: &str, state: &str, last_used: Option<&str>, available: bool) -> Json {
    let mut d = obj! {
        "udid" => udid, "name" => name, "state" => state, "isAvailable" => available,
        "deviceTypeIdentifier" => "com.apple.CoreSimulator.SimDeviceType.iPhone-16",
        "runtime" => "com.apple.CoreSimulator.SimRuntime.iOS-18-2",
    };
    if let Some(t) = last_used {
        d.set("lastUsedAt", t);
    }
    d
}

#[test]
fn cold_simulators_go_but_never_booted_recent_or_the_newest_of_their_type() {
    let mut ctx = ctx("m-sims");
    ctx.machine = true;
    let dir = ctx.home.join("Library/Developer/CoreSimulator/Devices");
    let u = |n: u8| format!("{:08X}-0000-4000-8000-{:012X}", n, n);
    let devices = vec![
        sim(&u(1), "cold-1", "Shutdown", None, true), // never used, made 140 days ago
        sim(&u(2), "cold-2", "Shutdown", None, true), // never used, made 75 days ago
        sim(&u(3), "newest", "Shutdown", None, true), // made today: the newest
        sim(&u(4), "booted", "Booted", None, true),   // made long ago but running
        sim(&u(5), "used", "Shutdown", Some("2020-01-01T00:00:00Z"), true), // old use stamp
        sim(&u(6), "gone-runtime", "Shutdown", None, false), // its runtime is gone
    ];
    for (i, days) in [(1, 140), (2, 75), (3, 0), (4, 200), (5, 300), (6, 10)] {
        let d = dir.join(u(i));
        write(&d.join("data/Containers/Data/Application/X/Documents/test-data.bin"), "test data");
        write(&d.join("device.plist"), "<plist/>");
        if days > 0 {
            for p in [d.join("device.plist"), d.clone()] {
                touch(&p, ctx.now - days * DAY);
            }
        }
    }
    let cache = obj! {"at" => crate::state::wall_clock(), "devices" => devices};
    crate::state::write_atomic(&ctx.file("cache-simctl.json"), cache.to_string().as_bytes()).unwrap();
    let mut c = simulators(&ctx, false);
    for x in &mut c {
        x.machine = true;
    }
    judge(&ctx, &mut c, Some(&[]));
    let by = |n: u8| c.iter().find(|x| x.path == dir.join(u(n))).unwrap();
    for n in [1, 2] {
        let x = by(n);
        assert!(x.reclaimable(), "{}", x.reason());
        assert!(x.bytes > 0, "sized: private bytes of the device folder");
        assert!(x.reason().contains("test data are deleted"), "{}", x.reason());
        let p = x.plan.as_ref().unwrap();
        assert!(p.new_device && !p.loss.is_empty());
        assert_eq!(
            p.steps[0].join(" "),
            format!("xcrun simctl create cold-{n} com.apple.CoreSimulator.SimDeviceType.iPhone-16 com.apple.CoreSimulator.SimRuntime.iOS-18-2")
        );
    }
    // The one device really used is the newest of its type, however old the use; a device made
    // today but never used doesn't outrank it (it's kept anyway: younger than 30 days).
    assert!(by(5).blocked.as_deref().unwrap().contains("newest"), "{:?}", by(5).blocked);
    assert!(!by(3).reclaimable() && !by(3).blocked.as_deref().unwrap().contains("newest"), "{:?}", by(3).blocked);
    assert!(by(4).blocked.as_deref().unwrap().contains("booted"), "{:?}", by(4).blocked);
    assert!(by(6).reclaimable(), "{}", by(6).reason());
    // Used ten days ago: kept for 30 days.
    let mut recent = devices_with_use(&ctx, &u(2), 10);
    judge(&ctx, &mut recent, Some(&[]));
    let x = recent.iter().find(|x| x.path == dir.join(u(2))).unwrap();
    // Now also the most recently used of its type, which wins the reason; either way it's kept.
    assert!(!x.reclaimable(), "{:?}", x.blocked);
}

/// The cached device list with one device's lastUsedAt set `days` ago.
fn devices_with_use(ctx: &Ctx, udid: &str, days: u64) -> Vec<Cand> {
    let mut j = crate::state::read_json(&ctx.file("cache-simctl.json")).unwrap();
    let mut list = j.get("devices").unwrap().arr().to_vec();
    let t = ctx.now - days * DAY;
    let stamp = format!("{}T00:00:00Z", civil(t));
    for d in &mut list {
        if d.s("udid") == udid {
            d.set("lastUsedAt", stamp.as_str());
        }
    }
    j.set("devices", Json::Arr(list));
    crate::state::write_atomic(&ctx.file("cache-simctl.json"), j.to_string().as_bytes()).unwrap();
    let mut c = simulators(ctx, false);
    for x in &mut c {
        x.machine = true;
    }
    c
}

/// Seconds since 1970 -> "YYYY-MM-DD" (UTC), by search: test-only.
fn civil(t: u64) -> String {
    let mut y = 1970;
    while date_secs(&format!("{}-01-01", y + 1)).is_some_and(|s| s <= t) {
        y += 1;
    }
    let mut m = 1;
    while m < 12 && date_secs(&format!("{y}-{:02}-01", m + 1)).is_some_and(|s| s <= t) {
        m += 1;
    }
    let d = 1 + (t - date_secs(&format!("{y}-{m:02}-01")).unwrap()) / DAY;
    format!("{y}-{m:02}-{d:02}")
}

#[test]
fn undoing_a_simulator_makes_a_blank_one_with_the_same_name_type_and_runtime() {
    let mut ctx = ctx("m-sim-undo");
    // Outside home: the rebuild never runs a tool from inside the folder it works in.
    let bin = ctx.home.parent().unwrap().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = bin.join("xcrun.log");
    script(&bin.join("xcrun"), &format!("echo \"$@\" >> '{}'", log.display()));
    // Only the fake xcrun is reachable: a real `simctl create` must never run from a test.
    ctx.path_var = bin.to_string_lossy().into_owned();
    let gone = ctx.home.join("Library/Developer/CoreSimulator/Devices/OLD-UDID");
    let a = obj! {
        "path" => &gone, "kind" => "simulator", "new_device" => true, "cwd" => &ctx.home,
        "steps" => vec![Json::Arr(["xcrun", "simctl", "create", "cold-1", "com.apple.CoreSimulator.SimDeviceType.iPhone-16", "com.apple.CoreSimulator.SimRuntime.iOS-18-2"].iter().map(|s| Json::from(*s)).collect())],
    };
    let r = crate::rebuild::restore(&ctx, &a);
    assert!(matches!(r, crate::rebuild::Outcome::Restored(_)), "the old path never comes back; a new device does");
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "simctl create cold-1 com.apple.CoreSimulator.SimDeviceType.iPhone-16 com.apple.CoreSimulator.SimRuntime.iOS-18-2\n");
}

#[test]
fn electron_caches_go_only_while_their_app_is_closed_and_app_data_never() {
    let mut ctx = ctx("m-electron");
    ctx.machine = true;
    let support = ctx.home.join("Library/Application Support");
    for app in ["Claude", "Codex", "Cursor"] {
        for d in ["Cache/Cache_Data/f_000001", "Code Cache/js/abc_0", "GPUCache/data_0"] {
            write(&support.join(app).join(d), "cache");
        }
        for d in ["Local Storage/leveldb/000003.log", "IndexedDB/x.leveldb/000003.log", "Preferences", "config.json"] {
            write(&support.join(app).join(d), "user data");
        }
    }
    // A running "Codex": a sleep started through a symlink named Codex.
    let fake = ctx.home.join("bin/Codex");
    std::fs::create_dir_all(fake.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink("/bin/sleep", &fake).unwrap();
    let mut codex = std::process::Command::new(&fake).arg("30").spawn().unwrap();
    let mut c = discover(&ctx, Some("electron_cache"));
    judge(&ctx, &mut c, Some(&[]));
    let _ = codex.kill();
    let _ = codex.wait();
    let caches: Vec<&Cand> = c.iter().filter(|x| x.kind == "electron_cache").collect();
    assert_eq!(caches.len(), 9, "Cache, Code Cache and GPUCache of three apps, nothing else");
    for x in &caches {
        let name = x.path.file_name().unwrap().to_string_lossy();
        assert!(["Cache", "Code Cache", "GPUCache"].contains(&&*name), "{}", x.path.display());
    }
    let procs = crate::proc::processes().unwrap();
    for x in &caches {
        let app = x.path.parent().unwrap().file_name().unwrap().to_string_lossy().into_owned();
        let up = app == "Codex" || running(&procs, &[&app]);
        assert_eq!(!x.reclaimable(), up, "{}: {}", x.path.display(), x.reason());
    }
    // Deleting a closed app's cache leaves its data alone.
    if let Some(x) = caches.iter().find(|x| x.reclaimable()) {
        crate::reclaim::reclaim(&ctx, x, "clean").unwrap();
        let app = x.path.parent().unwrap();
        assert!(!x.path.exists());
        for d in ["Local Storage/leveldb/000003.log", "IndexedDB/x.leveldb/000003.log", "Preferences", "config.json"] {
            assert_eq!(std::fs::read_to_string(app.join(d)).unwrap(), "user data");
        }
    }
}

#[test]
fn simctl_create_arguments_cannot_be_options() {
    let (t, r) = ("com.apple.CoreSimulator.SimDeviceType.iPhone-16", "com.apple.CoreSimulator.SimRuntime.iOS-18-2");
    assert!(sim_create_ok("iPhone 16", t, r));
    assert!(!sim_create_ok("-rf", t, r));
    assert!(!sim_create_ok("  ", t, r));
    assert!(!sim_create_ok("ok", "--set", r));
    assert!(!sim_create_ok("ok", t, "com.apple.CoreSimulator.SimRuntime.iOS 18; rm"));
    // An undo whose recorded command was edited into something else is refused.
    let mut ctx = ctx("m-sim-tamper");
    let bin = ctx.home.parent().unwrap().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = bin.join("xcrun.log");
    script(&bin.join("xcrun"), &format!("echo \"$@\" >> '{}'", log.display()));
    ctx.path_var = bin.to_string_lossy().into_owned();
    let a = obj! {
        "path" => &ctx.home.join("gone"), "kind" => "simulator", "new_device" => true, "cwd" => &ctx.home,
        "steps" => vec![Json::Arr(["xcrun", "simctl", "create", "-x", t, r].iter().map(|s| Json::from(*s)).collect())],
    };
    assert!(matches!(crate::rebuild::restore(&ctx, &a), crate::rebuild::Outcome::Failed(_)));
    assert!(!log.exists(), "nothing ran");
}
