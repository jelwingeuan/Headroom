//! Dedupe frees only true duplicates and keeps every byte; archives refuse anything unsafe.

use super::*;
use crate::testutil::{ctx, tmp, write};

#[test]
fn dedupe_clones_identical_files_and_leaves_the_rest() {
    let t = tmp("shrink-dedupe");
    let big = vec![42u8; 200_000];
    for p in ["a", "b"] {
        std::fs::create_dir_all(t.join(p).join("node_modules/x")).unwrap();
        std::fs::write(t.join(p).join("node_modules/x/lib.js"), &big).unwrap();
    }
    std::fs::write(t.join("a/node_modules/x/other.js"), vec![1u8; 200_000]).unwrap();
    std::fs::write(t.join("b/node_modules/x/other.js"), vec![2u8; 200_000]).unwrap();
    // A hard-linked file is shared already: left alone.
    std::fs::write(t.join("a/node_modules/x/linked.js"), &big).unwrap();
    std::fs::hard_link(t.join("a/node_modules/x/linked.js"), t.join("a/hard")).unwrap();
    std::fs::set_permissions(t.join("b/node_modules/x/lib.js"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let mtime = std::fs::metadata(t.join("b/node_modules/x/lib.js")).unwrap().modified().unwrap();
    let dirs = vec![t.join("a/node_modules"), t.join("b/node_modules")];
    let r = dedupe(&dirs);
    assert_eq!(r.files, 1);
    assert!(r.bytes >= 200_000);
    let after = |p: &str| {
        sys::list_path(&t.join(p).join("node_modules/x")).unwrap().into_iter().find(|e| e.name == b"lib.js").unwrap()
    };
    assert_eq!(after("a").private + after("b").private, 0, "both copies now share their blocks");
    assert_eq!(std::fs::read(t.join("b/node_modules/x/lib.js")).unwrap(), big);
    let m = std::fs::metadata(t.join("b/node_modules/x/lib.js")).unwrap();
    assert_eq!(m.permissions().mode() & 0o777, 0o755);
    assert_eq!(m.modified().unwrap(), mtime);
    assert_eq!(std::fs::read(t.join("b/node_modules/x/other.js")).unwrap(), vec![2u8; 200_000]);
    assert_eq!(std::fs::metadata(t.join("a/hard")).unwrap().nlink(), 2);
}

fn project(ctx: &Ctx) -> PathBuf {
    let p = ctx.home.join("code/old");
    write(&p.join("src/main.rs"), "fn main() {}\n");
    write(&p.join("notes/todo.md"), "keep me\n");
    std::fs::create_dir_all(p.join("empty")).unwrap();
    p
}

#[test]
fn archive_verifies_then_restore_puts_it_back_exactly() {
    let ctx = ctx("shrink-archive");
    let p = project(&ctx);
    let vol = tmp("shrink-volume");
    let j = archive(&ctx, &p, &vol, false).unwrap();
    let dest = PathBuf::from(j.s("archive"));
    assert!(dest.join("manifest.json").is_file());
    let left: Vec<String> =
        std::fs::read_dir(&p).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(left, [STUB]);
    restore(&dest).unwrap();
    assert_eq!(std::fs::read_to_string(p.join("notes/todo.md")).unwrap(), "keep me\n");
    assert!(p.join("empty").is_dir() && !p.join(STUB).exists());
    // Never into a path that has something in it again.
    assert!(restore(&dest).unwrap_err().contains("not empty"));
}

#[test]
fn archive_refuses_symlinks_and_the_same_drive() {
    let ctx = ctx("shrink-refuse");
    let p = project(&ctx);
    let vol = tmp("shrink-vol2");
    assert!(archive(&ctx, &p, &vol, true).unwrap_err().contains("another drive"));
    std::os::unix::fs::symlink("/etc/hosts", p.join("link")).unwrap();
    assert!(archive(&ctx, &p, &vol, false).unwrap_err().contains("symlink"));
    assert!(p.join("src/main.rs").exists(), "nothing deleted on refusal");
}

#[test]
fn restore_refuses_tampered_archives() {
    let ctx = ctx("shrink-tamper");
    let p = project(&ctx);
    let vol = tmp("shrink-vol3");
    let dest = PathBuf::from(archive(&ctx, &p, &vol, false).unwrap().s("archive"));
    let m = dest.join("manifest.json");
    let good = std::fs::read_to_string(&m).unwrap();
    // A changed file.
    std::fs::write(dest.join("files/notes/todo.md"), "tampered").unwrap();
    assert!(restore(&dest).unwrap_err().contains("SHA-256"));
    std::fs::write(dest.join("files/notes/todo.md"), "keep me\n").unwrap();
    // A path escaping the project.
    std::fs::write(&m, good.replace("\"notes/todo.md\"", "\"../escape.md\"")).unwrap();
    assert!(restore(&dest).unwrap_err().contains("unsafe path"));
    // A symlink planted in the archive.
    std::fs::write(&m, &good).unwrap();
    std::fs::remove_file(dest.join("files/notes/todo.md")).unwrap();
    std::os::unix::fs::symlink("/etc/hosts", dest.join("files/notes/todo.md")).unwrap();
    assert!(restore(&dest).unwrap_err().contains("symlink"));
    assert!(!ctx.home.parent().unwrap().join("escape.md").exists());
    let left: Vec<String> =
        std::fs::read_dir(&p).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(left, [STUB], "a failed restore leaves nothing behind");
}

#[test]
fn a_swap_happens_only_when_both_copies_still_hash_the_same() {
    let t = tmp("shrink-verify");
    std::fs::write(t.join("src"), vec![5u8; 100_000]).unwrap();
    std::fs::write(t.join("dup"), vec![5u8; 100_000]).unwrap();
    let e = sys::list_path(&t).unwrap().into_iter().find(|e| e.name == b"dup").unwrap();
    let dup = F { path: t.join("dup"), dev: e.dev, ino: e.ino, len: e.len, private: e.private, mtime: e.mtime };
    // A hash from before someone changed one of them: no swap, nothing changed.
    assert!(replace_with_clone(&t.join("src"), &dup, &sha256::hex(&[6u8; 100_000])).is_err());
    assert_eq!(std::fs::metadata(t.join("dup")).unwrap().ino(), e.ino);
    assert!(!t.join(".dup.headroom-clone").exists());
    replace_with_clone(&t.join("src"), &dup, &sha256::hex(&[5u8; 100_000])).unwrap();
    assert_ne!(std::fs::metadata(t.join("dup")).unwrap().ino(), e.ino);
    assert_eq!(std::fs::read(t.join("dup")).unwrap(), vec![5u8; 100_000]);
}

#[test]
fn clean_dedupes_kept_copies_of_one_lockfile_and_never_a_recent_project() {
    use crate::testutil::{git, touch};
    let ctx = ctx("shrink-clean");
    let old = 1_600_000_000;
    let w = crate::testutil::world(&ctx);
    let big = vec![9u8; 2 << 20];
    let mk = |name: &str, when: u64| {
        let p = w.join(name);
        write(&p.join("package.json"), r#"{"name":"same"}"#);
        write(&p.join("package-lock.json"), r#"{"lockfileVersion":3}"#);
        write(&p.join(".gitignore"), "node_modules/\n");
        git(&p, &["init", "-q"], Some(when));
        git(&p, &["add", "-A"], Some(when));
        git(&p, &["commit", "-qm", "c"], Some(when));
        std::fs::create_dir_all(p.join("node_modules/typescript/lib")).unwrap();
        std::fs::write(p.join("node_modules/typescript/lib/typescript.js"), &big).unwrap();
        write(&p.join("node_modules/.package-lock.json"), "{}");
        touch(&p.join("node_modules/typescript/lib/typescript.js"), when);
        touch(&p.join("node_modules/.package-lock.json"), when);
        // A hand edit keeps each copy from deletion; dedupe keeps every byte, so it may still run.
        write(&p.join(format!("node_modules/typescript/patch-{name}.js")), name);
        touch(&p.join(format!("node_modules/typescript/patch-{name}.js")), when + 86_400);
        p
    };
    let copies = [mk("a", old), mk("b", old), mk("c", old)];
    let recent = mk("d", ctx.now - 86_400);
    let ino = |p: &Path| std::fs::metadata(p).unwrap().ino();
    let recent_ino = ino(&recent.join("node_modules/typescript/lib/typescript.js"));
    let j = crate::reclaim::clean(&ctx, &w, true, "clean").unwrap();
    assert!(j.get("actions").unwrap().arr().is_empty(), "nothing deleted: {j}");
    let d = j.get("dedupe").unwrap();
    assert_eq!(d.get("files"), Some(&Json::Int(2)), "{j}");
    assert!(
        d.get("bytes").and_then(Json::u64).unwrap() <= 2 * (2 << 20) + 64 * 1024,
        "counted bytes never exceed what the copies held"
    );
    for p in &copies {
        assert_eq!(std::fs::read(p.join("node_modules/typescript/lib/typescript.js")).unwrap(), big);
        let name = p.file_name().unwrap().to_string_lossy();
        assert_eq!(std::fs::read_to_string(p.join(format!("node_modules/typescript/patch-{name}.js"))).unwrap(), name);
    }
    let private: u64 =
        copies.iter().map(|p| sys::list_path(&p.join("node_modules/typescript/lib")).unwrap()[0].private).sum();
    assert_eq!(private, 0, "the three copies share one set of blocks");
    assert_eq!(ino(&recent.join("node_modules/typescript/lib/typescript.js")), recent_ino);
}

#[test]
fn a_write_to_the_original_during_the_swap_puts_it_back() {
    use std::io::Write;
    let t = tmp("shrink-race");
    std::fs::write(t.join("src"), vec![5u8; 100_000]).unwrap();
    std::fs::write(t.join("dup"), vec![5u8; 100_000]).unwrap();
    let e = sys::list_path(&t).unwrap().into_iter().find(|e| e.name == b"dup").unwrap();
    let dup = F { path: t.join("dup"), dev: e.dev, ino: e.ino, len: e.len, private: e.private, mtime: e.mtime };
    // A writer that opened the file before the swap writes right after the rename.
    let mut writer = std::fs::OpenOptions::new().append(true).open(t.join("dup")).unwrap();
    let r = swap_checked(&t.join("src"), &dup, &sha256::hex(&[5u8; 100_000]), || {
        writer.write_all(b"late").unwrap();
        writer.sync_all().unwrap();
    });
    let err = r.unwrap_err().to_string();
    assert!(err.contains("written during the swap"), "{err}");
    let mut want = vec![5u8; 100_000];
    want.extend_from_slice(b"late");
    assert_eq!(std::fs::read(t.join("dup")).unwrap(), want, "the late write survives");
    assert_eq!(std::fs::metadata(t.join("dup")).unwrap().ino(), e.ino, "the original is back");
    let left: Vec<String> =
        std::fs::read_dir(&t).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(left.iter().all(|n| !n.contains("headroom")), "no temporary files left: {left:?}");
}
