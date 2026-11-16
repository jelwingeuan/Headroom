//! Deleting a folder so the deletion can't be redirected (THREAT-MODEL invariant 1): the
//! folder is opened with O_NOFOLLOW, its (device, inode) must match the scan, and everything
//! below it is removed through directory file descriptors with unlinkat, so a symlink is only
//! ever removed itself and the walk never leaves the folder.

use crate::state::Ctx;
use crate::sys::{self, AT_FDCWD, AT_REMOVEDIR, ELOOP, ENOENT, ENOTDIR, VDIR};
use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

fn other(msg: &str) -> io::Error {
    io::Error::other(msg.to_string())
}

pub fn delete_tree(path: &Path, dev: u64, ino: u64) -> io::Result<()> {
    delete_tree_counted(path, dev, ino).map_err(|(e, _)| e)
}

/// `delete_tree`, and on failure how many entries it had already removed (a folder that
/// failed midway is half gone and still needs its rebuild).
pub fn delete_tree_counted(path: &Path, dev: u64, ino: u64) -> Result<(), (io::Error, u64)> {
    let mut removed = 0;
    let r = (|| {
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return Err(other("no parent folder"));
        };
        let pfd = sys::open_dir(AT_FDCWD, parent.as_os_str())?;
        let fd = sys::open_dir(pfd.as_raw_fd(), name)?;
        let f = std::fs::File::from(fd);
        let m = f.metadata()?;
        if (m.dev(), m.ino()) != (dev, ino) {
            return Err(other("the folder changed since the scan"));
        }
        empty(f.as_raw_fd(), 0, &mut removed)?;
        drop(f);
        sys::unlink_at(pfd.as_raw_fd(), name, AT_REMOVEDIR)
    })();
    r.map_err(|e| (e, removed))
}

fn gone_ok(r: io::Result<()>) -> io::Result<()> {
    match r {
        Err(e) if e.raw_os_error() == Some(ENOENT) => Ok(()),
        r => r,
    }
}

fn empty(fd: RawFd, depth: u32, removed: &mut u64) -> io::Result<()> {
    if depth > 1000 {
        return Err(other("folders nested too deep"));
    }
    for e in sys::list_fd(fd)? {
        let name = e.name_os();
        if e.kind == VDIR {
            match sys::open_dir(fd, name) {
                Ok(sub) => {
                    empty(sub.as_raw_fd(), depth + 1, removed)?;
                    drop(sub);
                    gone_ok(sys::unlink_at(fd, name, AT_REMOVEDIR))?;
                    *removed += 1;
                }
                // Swapped for a symlink or a file since listing: remove that, nothing behind it.
                Err(err) if matches!(err.raw_os_error(), Some(ELOOP | ENOTDIR)) => {
                    gone_ok(sys::unlink_at(fd, name, 0))?;
                    *removed += 1;
                }
                Err(err) if err.raw_os_error() == Some(ENOENT) => {}
                Err(err) => return Err(err),
            }
        } else {
            gone_ok(sys::unlink_at(fd, name, 0))?;
            *removed += 1;
        }
    }
    Ok(())
}

/// Deletes one regular file (never a symlink's target) by name inside its parent folder.
pub fn delete_file(path: &Path) -> io::Result<()> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else { return Err(other("no parent folder")) };
    let pfd = sys::open_dir(AT_FDCWD, parent.as_os_str())?;
    if !std::fs::symlink_metadata(path)?.is_file() {
        return Err(other("not a regular file"));
    }
    gone_ok(sys::unlink_at(pfd.as_raw_fd(), name, 0))
}

/// Places nothing may ever delete, whatever a rule says: home and above, Headroom's own
/// state, AI conversation history, Docker Desktop's VM disk.
pub fn forbidden(ctx: &Ctx, path: &Path) -> Option<&'static str> {
    let h = &ctx.home;
    if !path.is_absolute() {
        return None; // docker:// and other tool-managed items
    }
    if h.starts_with(path) {
        return Some("the home folder or above");
    }
    if path.starts_with(&ctx.state) || ctx.state.starts_with(path) {
        return Some("Headroom's own history");
    }
    let never = [
        ".claude/projects",
        ".codex/sessions",
        ".codex/archived_sessions",
        "Library/Containers/com.docker.docker",
        "Library/Group Containers/group.com.docker",
    ];
    never
        .iter()
        .any(|n| path.starts_with(h.join(n)) || h.join(n).starts_with(path))
        .then_some("conversation history or Docker's disk")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::tmp;
    use std::os::unix::fs::symlink;

    fn id(p: &Path) -> (u64, u64) {
        let m = std::fs::symlink_metadata(p).unwrap();
        (m.dev(), m.ino())
    }

    #[test]
    fn deletes_a_tree_but_never_follows_symlinks_out_of_it() {
        let t = tmp("del-links");
        let outside = t.join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("precious"), "keep").unwrap();
        let target = t.join("proj/target");
        std::fs::create_dir_all(target.join("a/b")).unwrap();
        std::fs::write(target.join("a/b/f"), "x").unwrap();
        symlink(&outside, target.join("a/link-dir")).unwrap();
        symlink(outside.join("precious"), target.join("link-file")).unwrap();
        let (d, i) = id(&target);
        delete_tree(&target, d, i).unwrap();
        assert!(!target.exists());
        assert_eq!(std::fs::read_to_string(outside.join("precious")).unwrap(), "keep");
    }

    #[test]
    fn refuses_a_folder_swapped_after_the_scan() {
        let t = tmp("del-swap");
        let target = t.join("node_modules");
        std::fs::create_dir(&target).unwrap();
        let (d, i) = id(&target);
        // Swapped for a symlink to somewhere precious.
        let precious = t.join("precious");
        std::fs::create_dir(&precious).unwrap();
        std::fs::write(precious.join("f"), "keep").unwrap();
        std::fs::remove_dir(&target).unwrap();
        symlink(&precious, &target).unwrap();
        assert!(delete_tree(&target, d, i).is_err());
        assert!(precious.join("f").exists());
        // Swapped for another real folder.
        std::fs::remove_file(&target).unwrap();
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("new"), "x").unwrap();
        assert!(delete_tree(&target, d, i).is_err());
        assert!(target.join("new").exists());
    }

    #[test]
    fn delete_file_refuses_symlinks() {
        let t = tmp("del-file");
        std::fs::write(t.join("real"), "keep").unwrap();
        symlink(t.join("real"), t.join("link")).unwrap();
        assert!(delete_file(&t.join("link")).is_err());
        assert!(t.join("real").exists());
    }

    #[test]
    fn history_and_home_are_forbidden() {
        let ctx = crate::testutil::ctx("del-forbid");
        assert!(forbidden(&ctx, &ctx.home).is_some());
        assert!(forbidden(&ctx, Path::new("/")).is_some());
        assert!(forbidden(&ctx, &ctx.home.join(".claude/projects/x")).is_some());
        assert!(forbidden(&ctx, &ctx.home.join(".codex")).is_some());
        assert!(forbidden(&ctx, &ctx.state.join("history.jsonl")).is_some());
        assert!(forbidden(&ctx, &ctx.home.join("code/app/node_modules")).is_none());
    }
}
