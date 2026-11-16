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
    let mk = |name: &str, when: u64| {
        let proj = ctx.home.join("code").join(name);
        write(&proj.join("Package.swift"), "// swift-tools-version:5.9\n");
        git(&proj, &["init", "-q"], Some(when));
        git(&proj, &["add", "-A"], Some(when));
        git(&proj, &["commit", "-qm", "c"], Some(when));
        let d = dd.join(format!("{name}-abcdefghijkl"));
        write(
            &d.join("info.plist"),
            &format!("<plist><dict><key>WorkspacePath</key>\n<string>{}</string></dict></plist>", proj.display()),
        );
        std::fs::create_dir_all(d.join("Build/Products/Debug")).unwrap();
        (proj, d)
    };
    let (cold_proj, cold) = mk("SwiftAlpha", OLD);
    let (_, warm) = mk("SwiftBravo", ctx.now - 2 * DAY);
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
fn hugging_face_needs_a_cli_to_come_back() {
    let mut ctx = ctx("m-hf");
    ctx.machine = true;
    let repo = ctx.home.join(".cache/huggingface/hub/models--org--tiny");
    write(&repo.join("refs/main"), "abc123");
    write(&repo.join("blobs/b1"), "weights");
    std::fs::create_dir_all(repo.join("snapshots/abc123")).unwrap();
    std::os::unix::fs::symlink("../../blobs/b1", repo.join("snapshots/abc123/model.bin")).unwrap();
    for p in [repo.join("refs/main"), repo.join("blobs/b1")] {
        touch(&p, OLD);
    }
    let mut c = discover(&ctx, Some("hf_model"));
    judge(&ctx, &mut c, Some(&[]));
    assert!(c[0].blocked.as_deref().unwrap().contains("huggingface-cli"));
    let bin = ctx.home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    script(&bin.join("hf"), "exit 0");
    ctx.path_var = format!("{}:/usr/bin", bin.display());
    let mut c = discover(&ctx, Some("hf_model"));
    // The snapshot symlinks were made just now; blob access times keep it warm until they age.
    c[0].active = Some(OLD);
    c[0].recent_rule = false;
    judge(&ctx, &mut c, Some(&[]));
    assert!(c[0].reclaimable(), "{}", c[0].reason());
    let step = c[0].plan.as_ref().unwrap().steps[0].join(" ");
    assert!(
        step.starts_with("hf download org/tiny model.bin --revision abc123 --repo-type model --cache-dir "),
        "{step}"
    );
}

#[test]
fn ai_history_is_never_reclaimable_and_never_deletable() {
    let mut ctx = ctx("m-history");
    ctx.machine = true;
    write(&ctx.home.join(".claude/projects/x/a.jsonl"), "{}\n");
    write(&ctx.home.join(".codex/sessions/a.jsonl"), "{}\n");
    write(&ctx.home.join(".local/share/claude/versions/2.1.9"), "old binary");
    write(&ctx.home.join(".local/share/claude/versions/2.1.10"), "newest binary");
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
    assert_eq!(old, ["2.1.9"], "the newest version stays");
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
