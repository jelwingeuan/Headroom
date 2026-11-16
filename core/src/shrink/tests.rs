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
