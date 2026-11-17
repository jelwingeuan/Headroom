//! What the scan must find, and above all what it must never mark reclaimable.

use super::*;
use crate::testutil::{ctx, git, script, touch, world, write};
use crate::{rebuild, reclaim, state};
use std::os::unix::fs::PermissionsExt;

const OLD: u64 = 1_600_000_000; // 2020: cold

fn node_project(dir: &Path, when: u64) {
    write(&dir.join("package.json"), r#"{"name":"p","version":"1.0.0","scripts":{"build":"echo built"}}"#);
    write(&dir.join("package-lock.json"), "{}");
    write(&dir.join(".gitignore"), "node_modules/\n");
    write(&dir.join("index.js"), "console.log(1)\n");
    git(dir, &["init", "-q"], Some(when));
    git(dir, &["add", "-A"], Some(when));
    git(dir, &["commit", "-qm", "init"], Some(when));
    write(&dir.join("node_modules/left-pad/index.js"), "module.exports = 1\n");
    write(&dir.join("node_modules/.package-lock.json"), "{}");
    touch(&dir.join("node_modules/left-pad/index.js"), when);
    touch(&dir.join("node_modules/.package-lock.json"), when + 10);
}

fn find<'a>(s: &'a Scan, p: &Path) -> &'a Cand {
    s.cands.iter().find(|c| c.path == p).unwrap_or_else(|| {
        panic!("{} not found in {:?}", p.display(), s.cands.iter().map(|c| &c.path).collect::<Vec<_>>())
    })
}

fn blocked(s: &Scan, p: &Path) -> String {
    let c = find(s, p);
    assert!(!c.reclaimable(), "{} must be kept", p.display());
    c.reason().to_string()
}

#[test]
fn a_cold_node_modules_beside_package_json_may_go() {
    let ctx = ctx("scan-cold");
    let p = world(&ctx).join("app");
    node_project(&p, OLD);
    let s = scan(&ctx, &world(&ctx)).unwrap();
    let c = find(&s, &p.join("node_modules"));
    assert!(c.reclaimable(), "{}", c.reason());
    assert!(c.reason().contains("rebuild: npm ci"), "{}", c.reason());
    assert!(c.bytes > 0);
}

#[test]
fn a_recent_commit_or_an_open_editor_keeps_the_project() {
    let mut ctx = ctx("scan-recent");
    let w = world(&ctx);
    let fresh = w.join("fresh");
    node_project(&fresh, ctx.now - 2 * DAY);
    let open = w.join("open");
    node_project(&open, OLD);
    let storage = ctx.home.join("Library/Application Support/Code/User/globalStorage/storage.json");
    let uri = format!("file://{}", open.display()).replace(' ', "%20");
    write(&storage, &format!(r#"{{"windowsState":{{"lastActiveWindow":{{"folder":"{uri}"}}}}}}"#));
    let s = scan(&ctx, &w).unwrap();
    assert!(blocked(&s, &fresh.join("node_modules")).contains("used 2 days ago"));
    assert!(blocked(&s, &open.join("node_modules")).contains("days ago"));
    // The editor's record ages like everything else.
    touch(&storage, ctx.now - 30 * DAY);
    ctx.recent_days = 14;
    let s = scan(&ctx, &w).unwrap();
    assert!(find(&s, &open.join("node_modules")).reclaimable());
}

#[test]
fn tracked_files_inside_block_it() {
    let ctx = ctx("scan-tracked");
    let p = world(&ctx).join("vendored");
    node_project(&p, OLD);
    git(&p, &["add", "-f", "node_modules/left-pad/index.js"], Some(OLD));
    git(&p, &["commit", "-qm", "vendor"], Some(OLD));
    let s = scan(&ctx, &world(&ctx)).unwrap();
    assert!(blocked(&s, &p.join("node_modules")).contains("tracks"));
}

#[test]
fn ambiguous_build_and_dist_only_go_when_git_ignores_them() {
    let ctx = ctx("scan-maybe");
    let w = world(&ctx);
    // Hand-made, untracked, not ignored: real work.
    let site = w.join("site");
    node_project(&site, OLD);
    write(&site.join("build/notes.md"), "hand written");
    write(&site.join("dist/report.html"), "exported by hand");
    // Tracked build output: real work too.
    let tracked = w.join("tracked");
    node_project(&tracked, OLD);
    write(&tracked.join("build/output.js"), "module.exports = 42");
    git(&tracked, &["add", "build"], Some(OLD));
    git(&tracked, &["commit", "-qm", "b"], Some(OLD));
    // Ignored and produced by the build script: may go.
    let ignored = w.join("ignored");
    node_project(&ignored, OLD);
    write(&ignored.join(".gitignore"), "node_modules/\ndist/\n");
    git(&ignored, &["commit", "-qam", "ignore dist"], Some(OLD));
    write(&ignored.join("dist/app.js"), "built");
    // Not a git repo at all: an ambiguous name can't be proved.
    let loose = w.join("loose");
    write(&loose.join("package.json"), "{}");
    write(&loose.join("build/x"), "x");
    let s = scan(&ctx, &w).unwrap();
    assert!(blocked(&s, &site.join("build")).contains("git ignores"));
    assert!(blocked(&s, &site.join("dist")).contains("git ignores"));
    assert!(blocked(&s, &tracked.join("build")).contains("tracks"));
    assert!(blocked(&s, &loose.join("build")).contains("git ignores"));
    let d = find(&s, &ignored.join("dist"));
    assert!(d.reclaimable(), "{}", d.reason());
}

#[test]
fn a_venv_needs_a_dependency_list() {
    let ctx = ctx("scan-venv");
    let w = world(&ctx);
    let bare = w.join("bare");
    write(&bare.join("main.py"), "import six");
    write(&bare.join(".gitignore"), "env/\n");
    git(&bare, &["init", "-q"], Some(OLD));
    git(&bare, &["add", "-A"], Some(OLD));
    git(&bare, &["commit", "-qm", "c"], Some(OLD));
    write(&bare.join("env/pyvenv.cfg"), "home = /usr/bin\n");
    write(&bare.join("env/lib/python3.12/site-packages/six-1.16.0.dist-info/METADATA"), "Name: six");
    let ok = w.join("ok");
    write(&ok.join("requirements.txt"), "six\n");
    write(&ok.join(".gitignore"), ".venv/\n");
    git(&ok, &["init", "-q"], Some(OLD));
    git(&ok, &["add", "-A"], Some(OLD));
    git(&ok, &["commit", "-qm", "c"], Some(OLD));
    write(&ok.join(".venv/pyvenv.cfg"), "home = /usr/bin\n");
    write(&ok.join(".venv/lib/python3.12/site-packages/six-1.16.0.dist-info/METADATA"), "Name: six");
    let s = scan(&ctx, &w).unwrap();
    assert!(blocked(&s, &bare.join("env")).contains("no dependency list"));
    let v = find(&s, &ok.join(".venv"));
    assert!(v.reclaimable(), "{}", v.reason());
    assert_eq!(v.plan.as_ref().unwrap().pins.as_deref(), Some("six==1.16.0\n"));
}

#[test]
fn a_hand_edited_node_modules_is_kept() {
    let ctx = ctx("scan-edited");
    let p = world(&ctx).join("patched");
    node_project(&p, OLD);
    touch(&p.join("node_modules/left-pad/index.js"), OLD + DAY); // a day after the install
    let s = scan(&ctx, &world(&ctx)).unwrap();
    assert!(blocked(&s, &p.join("node_modules")).contains("changed after the install"));
    // Tool caches inside node_modules (.cache, .vite) are not hand edits.
    let q = world(&ctx).join("cached");
    node_project(&q, OLD);
    write(&q.join("node_modules/.cache/babel/x"), "cache");
    let s = scan(&ctx, &world(&ctx)).unwrap();
    assert!(find(&s, &q.join("node_modules")).reclaimable());
}

#[test]
fn node_modules_without_an_install_record_is_kept() {
    let ctx = ctx("scan-norecord");
    let p = world(&ctx).join("app");
    node_project(&p, OLD);
    std::fs::remove_file(p.join("node_modules/.package-lock.json")).unwrap();
    let s = scan(&ctx, &world(&ctx)).unwrap();
    assert!(blocked(&s, &p.join("node_modules")).contains("no install record"));
}

#[test]
fn never_looks_inside_git_dependency_trees_or_bundles() {
    let ctx = ctx("scan-never-enter");
    let p = world(&ctx).join("app");
    node_project(&p, OLD);
    // A package inside node_modules looks like a project of its own: never searched.
    write(&p.join("node_modules/left-pad/package.json"), "{}");
    write(&p.join("node_modules/left-pad/node_modules/x/i.js"), "x");
    write(&p.join("Tool.app/Contents/Resources/package.json"), "{}");
    write(&p.join("Tool.app/Contents/Resources/node_modules/y/i.js"), "y");
    write(&p.join(".git/stash/package.json"), "{}");
    std::fs::create_dir_all(p.join(".git/stash/node_modules")).unwrap();
    let s = scan(&ctx, &world(&ctx)).unwrap();
    let paths: Vec<&PathBuf> = s.cands.iter().map(|c| &c.path).collect();
    assert_eq!(paths, [&p.join("node_modules")]);
}

#[test]
fn a_symlinked_build_folder_is_not_a_candidate() {
    let ctx = ctx("scan-symlink");
    let p = world(&ctx).join("app");
    node_project(&p, OLD);
    std::fs::remove_dir_all(p.join("node_modules")).unwrap();
    let elsewhere = world(&ctx).join("precious");
    write(&elsewhere.join("keep"), "x");
    std::os::unix::fs::symlink(&elsewhere, p.join("node_modules")).unwrap();
    let s = scan(&ctx, &world(&ctx)).unwrap();
    assert!(s.cands.is_empty(), "{:?}", s.cands.iter().map(|c| &c.path).collect::<Vec<_>>());
}

#[test]
fn system_folders_and_the_folders_above_home_are_refused() {
    let ctx = ctx("scan-refuse");
    for p in ["/", "/System", "/usr", "/Library", "/Applications"] {
        assert!(scan(&ctx, Path::new(p)).is_err(), "{p}");
    }
    assert!(scan(&ctx, ctx.home.parent().unwrap()).is_err());
    std::fs::create_dir_all(ctx.home.join("Library/Developer")).unwrap();
    assert!(scan(&ctx, &ctx.home.join("Library/Developer")).is_err());
    assert!(scan(&ctx, &ctx.home).is_ok());
}

#[test]
fn a_project_with_a_running_process_is_kept() {
    let ctx = ctx("scan-busy");
    let p = world(&ctx).join("server");
    node_project(&p, OLD);
    std::fs::create_dir_all(p.join("src")).unwrap();
    let mut child = std::process::Command::new("/bin/sleep").arg("30").current_dir(p.join("src")).spawn().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let s = scan(&ctx, &world(&ctx)).unwrap();
    let _ = child.kill();
    let _ = child.wait();
    assert!(blocked(&s, &p.join("node_modules")).contains("in use: sleep"));
}

fn agent_repo(ctx: &Ctx) -> (PathBuf, PathBuf) {
    let w = world(ctx);
    let repo = w.join("agent");
    let bare = w.join("remote.git");
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "-q", "--bare"], Some(OLD));
    write(&repo.join("README.md"), "# a\n");
    write(&repo.join(".gitignore"), ".env\n.claude/worktrees/\nnode_modules/\n");
    git(&repo, &["init", "-q", "-b", "main"], Some(OLD));
    git(&repo, &["add", "-A"], Some(OLD));
    git(&repo, &["commit", "-qm", "init"], Some(OLD));
    git(&repo, &["remote", "add", "origin", &bare.to_string_lossy()], Some(OLD));
    git(&repo, &["push", "-q", "origin", "main"], Some(OLD));
    let wt = |n: &str| repo.join(".claude/worktrees").join(n);
    git(&repo, &["worktree", "add", "-q", "-b", "agent/merged", &wt("merged").to_string_lossy()], Some(OLD));
    write(&wt("merged").join("f.js"), "feature");
    git(&wt("merged"), &["add", "-A"], Some(OLD));
    git(&wt("merged"), &["commit", "-qm", "feature"], Some(OLD));
    git(&repo, &["merge", "-q", "--ff-only", "agent/merged"], Some(OLD));
    git(&repo, &["push", "-q", "origin", "main"], Some(OLD));
    git(&repo, &["worktree", "add", "-q", "-b", "agent/unpushed", &wt("unpushed").to_string_lossy()], Some(OLD));
    write(&wt("unpushed").join("wip.js"), "wip");
    git(&wt("unpushed"), &["add", "-A"], Some(OLD));
    git(&wt("unpushed"), &["commit", "-qm", "wip"], Some(OLD));
    git(&repo, &["worktree", "add", "-q", "-b", "agent/env", &wt("env").to_string_lossy()], Some(OLD));
    write(&wt("env").join(".env"), "API_KEY=only-here\n");
    git(&repo, &["worktree", "add", "-q", "-b", "agent/dirty", &wt("dirty").to_string_lossy()], Some(OLD));
    write(&wt("dirty").join("untracked.txt"), "work");
    (repo, bare)
}

#[test]
fn agent_worktrees_go_only_when_merged_clean_and_fully_rebuildable() {
    let ctx = ctx("scan-worktrees");
    let (repo, _) = agent_repo(&ctx);
    let wt = |n: &str| repo.join(".claude/worktrees").join(n);
    let s = scan(&ctx, &world(&ctx)).unwrap();
    let m = find(&s, &wt("merged"));
    assert!(m.reclaimable(), "{}", m.reason());
    assert!(blocked(&s, &wt("unpushed")).contains("not merged or pushed"));
    assert!(blocked(&s, &wt("env")).contains(".env"));
    assert!(blocked(&s, &wt("dirty")).contains("uncommitted or untracked"));
    // Removed through git (never --force), and it comes back with `git worktree add`.
    let a = reclaim::reclaim(&ctx, m, "clean").unwrap();
    assert!(!wt("merged").exists() && !repo.join(".git/worktrees/merged").exists());
    assert!(wt("env").join(".env").exists() && wt("unpushed").join("wip.js").exists());
    assert_eq!(a.get("steps").unwrap().arr()[0].arr()[0], Json::from("git"));
    let (j, ok) = rebuild::undo_all(&ctx, None);
    assert!(ok, "{j}");
    assert!(wt("merged").join("f.js").exists());
}

#[test]
fn clean_records_the_rebuild_and_undo_never_overwrites() {
    let mut ctx = ctx("scan-undo");
    let p = world(&ctx).join("app");
    node_project(&p, OLD);
    // A fake npm on PATH: `npm ci` recreates node_modules and counts its runs.
    let bin = ctx.home.parent().unwrap().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let runs = bin.join("runs");
    script(
        &bin.join("npm"),
        &format!(
            "echo \"$@\" >> '{}'; mkdir -p node_modules/left-pad && echo '{{}}' > node_modules/.package-lock.json",
            runs.display()
        ),
    );
    ctx.path_var = format!("{}:/usr/bin:/bin", bin.display());
    let j = reclaim::clean(&ctx, &world(&ctx), true, "clean").unwrap();
    assert_eq!(j.get("actions").unwrap().arr().len(), 1, "{j}");
    assert!(!p.join("node_modules").exists());
    let h = state::history(&ctx);
    assert_eq!(h[0].s("lock_sha256"), crate::sha256::hex(b"{}"));
    assert_eq!(h[0].get("steps").unwrap().to_string(), r#"[["npm","ci"]]"#);
    let (_, ok) = rebuild::undo_all(&ctx, None);
    assert!(ok);
    assert!(p.join("node_modules/.package-lock.json").exists());
    assert_eq!(std::fs::read_to_string(&runs).unwrap(), "ci\n");
    // Deleted again, then something else appears at the path: undo leaves it alone.
    touch(&p.join("node_modules/.package-lock.json"), OLD + 10);
    touch(&p.join("node_modules/left-pad"), OLD);
    reclaim::clean(&ctx, &world(&ctx), true, "clean").unwrap();
    write(&p.join("node_modules/mine.txt"), "made by hand after the clean");
    let (j, ok) = rebuild::undo_all(&ctx, None);
    assert!(ok, "{j}");
    assert_eq!(std::fs::read_to_string(p.join("node_modules/mine.txt")).unwrap(), "made by hand after the clean");
    assert_eq!(std::fs::read_to_string(&runs).unwrap(), "ci\n", "npm must not run over an existing folder");
}

#[test]
fn the_recheck_before_delete_catches_a_project_that_became_recent() {
    let ctx = ctx("scan-recheck");
    let p = world(&ctx).join("app");
    node_project(&p, OLD);
    let s = scan(&ctx, &world(&ctx)).unwrap();
    let c = find(&s, &p.join("node_modules")).clone();
    assert!(c.reclaimable());
    write(&p.join("new.js"), "new work");
    git(&p, &["add", "new.js"], None);
    git(&p, &["commit", "-qm", "today"], None);
    let e = reclaim::reclaim(&ctx, &c, "clean").unwrap_err();
    assert!(e.contains("days ago"), "{e}");
    assert!(p.join("node_modules/left-pad/index.js").exists());
}

#[test]
fn warm_scan_of_many_files_is_fast() {
    // A scaled-down judge speed tree: 20 projects x 50 dirs x 100 empty files = 100k files.
    let ctx = ctx("scan-speed");
    let w = world(&ctx);
    for p in 0..20 {
        let proj = w.join(format!("p{p:03}"));
        write(&proj.join("package.json"), r#"{"dependencies":{"left-pad":"^1"}}"#);
        for d in 0..50 {
            let dir = proj.join(format!("node_modules/m{d:03}"));
            std::fs::create_dir_all(&dir).unwrap();
            for f in 0..100 {
                std::fs::File::create(dir.join(format!("f{f:03}.js"))).unwrap();
            }
        }
    }
    scan(&ctx, &w).unwrap();
    let t = std::time::Instant::now();
    let s = scan(&ctx, &w).unwrap();
    let secs = t.elapsed().as_secs_f64();
    assert_eq!(s.cands.len(), 20);
    eprintln!("warm scan of 100k files: {secs:.3}s");
    assert!(secs < 2.0, "warm scan took {secs:.2}s");
}

#[test]
fn sizes_are_private_bytes_with_hard_links_counted_only_when_all_links_are_inside() {
    let ctx = ctx("scan-sizes");
    let p = world(&ctx).join("app");
    node_project(&p, OLD);
    let nm = p.join("node_modules");
    std::fs::write(nm.join("left-pad/inside.js"), vec![1u8; 100_000]).unwrap();
    std::fs::hard_link(nm.join("left-pad/inside.js"), nm.join("left-pad/inside-link.js")).unwrap();
    std::fs::write(nm.join("left-pad/shared.js"), vec![2u8; 100_000]).unwrap();
    std::fs::hard_link(nm.join("left-pad/shared.js"), p.join("outside-link.js")).unwrap();
    for f in ["left-pad/inside.js", "left-pad/shared.js"] {
        touch(&nm.join(f), OLD);
    }
    let mut c = vec![Cand::new(nm.clone(), "node_modules", How::Tree)];
    size_all(&mut c);
    let block = |f: &str| std::fs::metadata(nm.join(f)).unwrap().blocks() * 512;
    let small = block("left-pad/index.js") + block(".package-lock.json");
    // inside.js once (both links are inside), shared.js not at all (deleting frees nothing).
    assert_eq!(c[0].bytes, small + block("left-pad/inside.js"));
}

#[test]
fn a_delete_that_fails_midway_is_recorded_so_rebuild_brings_it_back() {
    let mut ctx = ctx("scan-partial");
    let p = world(&ctx).join("app");
    node_project(&p, OLD);
    // node_modules/sub is read-only: x's files can be removed, x itself can't.
    let x = p.join("node_modules/sub/x");
    for f in ["a.js", "b.js"] {
        write(&x.join(f), "x");
        touch(&x.join(f), OLD);
    }
    let sub = p.join("node_modules/sub");
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o555)).unwrap();
    let bin = ctx.home.parent().unwrap().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let runs = bin.join("runs");
    script(
        &bin.join("npm"),
        &format!(
            "echo \"$@\" >> '{}'; mkdir -p node_modules/left-pad && echo '{{}}' > node_modules/.package-lock.json",
            runs.display()
        ),
    );
    ctx.path_var = format!("{}:/usr/bin:/bin", bin.display());
    let j = reclaim::clean(&ctx, &world(&ctx), true, "clean").unwrap();
    let err = j.get("errors").unwrap().arr()[0].s("error").to_string();
    assert!(err.contains("partly deleted"), "{j}");
    let h = state::history(&ctx);
    assert_eq!(h.len(), 1, "a half-deleted folder must be in the history");
    assert_eq!(h[0].get("partial"), Some(&Json::Bool(true)));
    assert!(p.join("node_modules").exists(), "the remnant is still there");
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (out, ok) = rebuild::undo_all(&ctx, None);
    assert!(ok, "{out}");
    assert_eq!(std::fs::read_to_string(&runs).unwrap(), "ci\n", "the rebuild ran over our own remnant");
    assert!(p.join("node_modules/.package-lock.json").exists());
}

#[test]
fn when_lsof_cannot_run_nothing_in_a_project_is_touched() {
    let ctx = ctx("scan-nolsof");
    let p = world(&ctx).join("app");
    node_project(&p, OLD);
    let entries = sys::list_path(&p).unwrap();
    let mut c = vec![];
    visit(&ctx, &p, &entries, Some(&p), Some(&p), 1, &mut c);
    let cwds = cwds_with(Path::new("/nonexistent/lsof"));
    assert!(cwds.is_none());
    judge(&ctx, &mut c, cwds.as_deref());
    let nm = c.iter().find(|x| x.kind == "node_modules").unwrap();
    assert!(nm.reason().contains("lsof couldn't list"), "{}", nm.reason());
    assert!(cwds_with(Path::new("/usr/sbin/lsof")).is_some());
}

#[test]
fn only_a_shell_or_agent_parent_is_ignored() {
    assert!(known_agent("-zsh"));
    assert!(known_agent("/Users/me/.local/bin/claude"));
    assert!(known_agent("node /usr/local/lib/node_modules/@anthropic-ai/claude-code/cli.js"));
    assert!(known_agent("/opt/homebrew/bin/codex mcp"));
    assert!(!known_agent("node server.js"));
    assert!(!known_agent("/usr/bin/python3 judge/run.py"));
}

/// A shell that just sits in `dir` (bash waiting on a FIFO: no child processes).
fn idle_shell(dir: &Path) -> std::process::Child {
    let fifo = dir.join(".idle-fifo");
    assert!(std::process::Command::new("/usr/bin/mkfifo").arg(&fifo).status().unwrap().success());
    let c = std::process::Command::new("/bin/bash")
        .args(["-c", "read -t 30 <> \"$0\"", &fifo.to_string_lossy()])
        .current_dir(dir)
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    c
}

#[test]
fn a_merged_worktree_made_today_from_old_commits_is_cold_unless_someone_sits_in_it() {
    // Like the judge's world: `git worktree add` runs today (no date), the work is old.
    let ctx = ctx("scan-wt-cold");
    let w = world(&ctx);
    let repo = w.join("agent");
    write(&repo.join("README.md"), "# a\n");
    write(&repo.join(".gitignore"), ".claude/worktrees/\n");
    git(&repo, &["init", "-q", "-b", "main"], Some(OLD));
    git(&repo, &["add", "-A"], Some(OLD));
    git(&repo, &["commit", "-qm", "init"], Some(OLD));
    let wt = repo.join(".claude/worktrees/merged");
    git(&repo, &["worktree", "add", "-q", "-b", "agent/merged", &wt.to_string_lossy()], None);
    write(&wt.join("f.js"), "feature");
    git(&wt, &["add", "-A"], Some(OLD + 3600));
    git(&wt, &["commit", "-qm", "feature"], Some(OLD + 3600));
    git(&repo, &["merge", "-q", "--ff-only", "agent/merged"], None);
    let s = scan(&ctx, &w).unwrap();
    assert!(find(&s, &wt).reclaimable(), "{}", find(&s, &wt).reason());
    // An agent's shell sitting in the worktree: it is in use.
    let mut sh = idle_shell(&wt);
    let s = scan(&ctx, &w).unwrap();
    let _ = sh.kill();
    let _ = sh.wait();
    assert!(blocked(&s, &wt).contains("in use: bash"));
    let _ = std::fs::remove_file(wt.join(".idle-fifo"));
}

#[test]
fn a_shell_in_a_normal_project_does_not_hold_its_dependencies() {
    let ctx = ctx("scan-shell-quiet");
    let p = world(&ctx).join("app");
    node_project(&p, OLD);
    let mut sh = idle_shell(&p.join("node_modules"));
    let s = scan(&ctx, &world(&ctx)).unwrap();
    let _ = sh.kill();
    let _ = sh.wait();
    let c = find(&s, &p.join("node_modules"));
    assert!(c.reclaimable(), "{}", c.reason());
}

#[test]
fn a_branch_switch_in_a_worktree_today_is_use_even_at_the_same_commit() {
    let ctx = ctx("scan-wt-checkout");
    let w = world(&ctx);
    let repo = w.join("agent");
    write(&repo.join("README.md"), "# a\n");
    write(&repo.join(".gitignore"), ".claude/worktrees/\n");
    git(&repo, &["init", "-q", "-b", "main"], Some(OLD));
    git(&repo, &["add", "-A"], Some(OLD));
    git(&repo, &["commit", "-qm", "init"], Some(OLD));
    let wt = repo.join(".claude/worktrees/merged");
    git(&repo, &["worktree", "add", "-q", "-b", "agent/merged", &wt.to_string_lossy()], None);
    // Today, at the same commit: someone started working here.
    git(&wt, &["checkout", "-q", "-b", "fix"], None);
    let s = scan(&ctx, &w).unwrap();
    assert!(blocked(&s, &wt).contains("used 0 days ago"), "{}", find(&s, &wt).reason());
}
