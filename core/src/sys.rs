//! The few macOS calls std doesn't offer: bulk directory listing with private sizes
//! (getattrlistbulk), fd-relative open/unlink, clonefile, statvfs, flock, sync.

use std::ffi::{c_char, c_int, c_void, CString, OsStr};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

pub const AT_FDCWD: c_int = -2;
const O_RDONLY: c_int = 0;
const O_NOFOLLOW: c_int = 0x100;
const O_DIRECTORY: c_int = 0x10_0000;
const O_CLOEXEC: c_int = 0x100_0000;
pub const AT_REMOVEDIR: c_int = 0x80;
pub const ELOOP: i32 = 62;
pub const ENOTDIR: i32 = 20;
pub const ENOENT: i32 = 2;

pub const VREG: u32 = 1;
pub const VDIR: u32 = 2;
pub const VLNK: u32 = 5;

const ATTR_BIT_MAP_COUNT: u16 = 5;
const ATTR_CMN_NAME: u32 = 0x1;
const ATTR_CMN_DEVID: u32 = 0x2;
const ATTR_CMN_OBJTYPE: u32 = 0x8;
const ATTR_CMN_MODTIME: u32 = 0x400;
const ATTR_CMN_FILEID: u32 = 0x200_0000;
const ATTR_CMN_ERROR: u32 = 0x2000_0000;
const ATTR_CMN_RETURNED_ATTRS: u32 = 0x8000_0000;
const ATTR_FILE_LINKCOUNT: u32 = 0x1;
const ATTR_FILE_ALLOCSIZE: u32 = 0x4;
const ATTR_FILE_DATALENGTH: u32 = 0x200;
const ATTR_CMNEXT_PRIVATESIZE: u32 = 0x8;
const FSOPT_ATTR_CMN_EXTENDED: u64 = 0x20;
const CLONE_NOFOLLOW: u32 = 0x1;
const LOCK_EX: c_int = 2;

#[repr(C)]
struct AttrList {
    bitmapcount: u16,
    reserved: u16,
    commonattr: u32,
    volattr: u32,
    dirattr: u32,
    fileattr: u32,
    forkattr: u32,
}

/// macOS `struct statvfs` (block counts are 32-bit there).
#[repr(C)]
#[derive(Default)]
struct StatVfs {
    f_bsize: u64,
    f_frsize: u64,
    f_blocks: u32,
    f_bfree: u32,
    f_bavail: u32,
    f_files: u32,
    f_ffree: u32,
    f_favail: u32,
    f_fsid: u64,
    f_flag: u64,
    f_namemax: u64,
}

extern "C" {
    fn openat(fd: c_int, path: *const c_char, oflag: c_int, ...) -> c_int;
    fn unlinkat(fd: c_int, path: *const c_char, flag: c_int) -> c_int;
    fn getattrlistbulk(fd: c_int, list: *mut AttrList, buf: *mut c_void, size: usize, options: u64) -> c_int;
    fn clonefile(src: *const c_char, dst: *const c_char, flags: u32) -> c_int;
    fn statvfs(path: *const c_char, buf: *mut StatVfs) -> c_int;
    fn flock(fd: c_int, op: c_int) -> c_int;
    fn sync();
}

fn cstr(s: &OsStr) -> io::Result<CString> {
    CString::new(s.as_bytes()).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"))
}

fn check(r: c_int) -> io::Result<c_int> {
    if r < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(r)
    }
}

/// Opens a directory relative to `dir` (or AT_FDCWD) without following a final symlink.
pub fn open_dir(dir: RawFd, name: &OsStr) -> io::Result<OwnedFd> {
    let c = cstr(name)?;
    // SAFETY: c is a valid C string; openat returns a new fd or -1.
    let fd = check(unsafe { openat(dir, c.as_ptr(), O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC) })?;
    // SAFETY: fd was just returned by openat and is owned by nobody else.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

pub fn unlink_at(dir: RawFd, name: &OsStr, flag: c_int) -> io::Result<()> {
    let c = cstr(name)?;
    // SAFETY: c is a valid C string.
    check(unsafe { unlinkat(dir, c.as_ptr(), flag) }).map(|_| ())
}

/// One directory entry as getattrlistbulk reports it. Sizes are 0 for directories.
#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub name: Vec<u8>,
    pub kind: u32,
    pub dev: u64,
    pub ino: u64,
    pub mtime: i64,
    pub nlink: u32,
    /// Allocated bytes not shared with a clone (APFS); allocated bytes where unsupported.
    pub private: u64,
    pub len: u64,
    pub error: u32,
}

impl Entry {
    pub fn name_os(&self) -> &OsStr {
        OsStr::from_bytes(&self.name)
    }
    pub fn name_str(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.name)
    }
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_ne_bytes(b[i..i + 4].try_into().unwrap())
}
fn i64_at(b: &[u8], i: usize) -> i64 {
    i64::from_ne_bytes(b[i..i + 8].try_into().unwrap())
}

/// Every entry of an open directory, in one or a few system calls per directory.
pub fn list_fd(fd: RawFd) -> io::Result<Vec<Entry>> {
    let mut al = AttrList {
        bitmapcount: ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: ATTR_CMN_RETURNED_ATTRS
            | ATTR_CMN_NAME
            | ATTR_CMN_DEVID
            | ATTR_CMN_OBJTYPE
            | ATTR_CMN_MODTIME
            | ATTR_CMN_FILEID
            | ATTR_CMN_ERROR,
        volattr: 0,
        dirattr: 0,
        fileattr: ATTR_FILE_LINKCOUNT | ATTR_FILE_ALLOCSIZE | ATTR_FILE_DATALENGTH,
        forkattr: ATTR_CMNEXT_PRIVATESIZE,
    };
    let mut buf = vec![0u8; 64 * 1024];
    let mut out = Vec::new();
    loop {
        // SAFETY: al and buf are valid for the call; buf.len() is its real size.
        let n = unsafe { getattrlistbulk(fd, &mut al, buf.as_mut_ptr().cast(), buf.len(), FSOPT_ATTR_CMN_EXTENDED) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        if n == 0 {
            return Ok(out);
        }
        let mut p = 0usize;
        for _ in 0..n {
            let len = u32_at(&buf, p) as usize;
            out.push(parse_entry(&buf[p..p + len]));
            p += len;
        }
    }
}

/// One packed entry. Without FSOPT_PACK_INVAL_ATTRS an attribute is present only when its
/// bit is in the returned set, in bit order: common, then file, then extended common.
fn parse_entry(b: &[u8]) -> Entry {
    let (cmn, file, ext) = (u32_at(b, 4), u32_at(b, 16), u32_at(b, 20));
    let mut q = 24;
    let mut take = |n: usize| {
        let at = q;
        q += n;
        at
    };
    let mut e = Entry::default();
    if cmn & ATTR_CMN_NAME != 0 {
        let at = take(8);
        let off = i32::from_ne_bytes(b[at..at + 4].try_into().unwrap()) as isize;
        let n = u32_at(b, at + 4) as usize;
        let start = (at as isize + off) as usize;
        e.name = b[start..start + n.saturating_sub(1)].to_vec();
    }
    if cmn & ATTR_CMN_DEVID != 0 {
        e.dev = i32::from_ne_bytes(b[take(4)..][..4].try_into().unwrap()) as u64;
    }
    if cmn & ATTR_CMN_OBJTYPE != 0 {
        e.kind = u32_at(b, take(4));
    }
    if cmn & ATTR_CMN_MODTIME != 0 {
        e.mtime = i64_at(b, take(16));
    }
    if cmn & ATTR_CMN_FILEID != 0 {
        e.ino = i64_at(b, take(8)) as u64;
    }
    if cmn & ATTR_CMN_ERROR != 0 {
        e.error = u32_at(b, take(4));
    }
    if file & ATTR_FILE_LINKCOUNT != 0 {
        e.nlink = u32_at(b, take(4));
    }
    let mut alloc = 0;
    if file & ATTR_FILE_ALLOCSIZE != 0 {
        alloc = i64_at(b, take(8)).max(0) as u64;
    }
    if file & ATTR_FILE_DATALENGTH != 0 {
        e.len = i64_at(b, take(8)).max(0) as u64;
    }
    e.private = if ext & ATTR_CMNEXT_PRIVATESIZE != 0 { i64_at(b, take(8)).max(0) as u64 } else { alloc };
    e
}

pub fn list_path(p: &Path) -> io::Result<Vec<Entry>> {
    let fd = open_dir(AT_FDCWD, p.as_os_str())?;
    list_fd(fd.as_raw_fd())
}

/// Free bytes for unprivileged users on the volume holding `p`: f_bavail * f_frsize.
pub fn free_bytes(p: &Path) -> u64 {
    let Ok(c) = cstr(p.as_os_str()) else { return 0 };
    let mut s = StatVfs::default();
    // SAFETY: c is valid; s is a correctly sized out-parameter.
    if unsafe { statvfs(c.as_ptr(), &mut s) } != 0 {
        return 0;
    }
    s.f_bavail as u64 * s.f_frsize
}

/// APFS clone of `src` at `dst` (which must not exist). Shares all data blocks.
pub fn clone_file(src: &Path, dst: &Path) -> io::Result<()> {
    let (s, d) = (cstr(src.as_os_str())?, cstr(dst.as_os_str())?);
    // SAFETY: both are valid C strings.
    check(unsafe { clonefile(s.as_ptr(), d.as_ptr(), CLONE_NOFOLLOW) }).map(|_| ())
}

/// Blocks until this process holds an exclusive lock on `f` (released on close).
pub fn lock(f: &std::fs::File) -> io::Result<()> {
    // SAFETY: f is an open file.
    check(unsafe { flock(f.as_raw_fd(), LOCK_EX) }).map(|_| ())
}

pub fn sync_all() {
    // SAFETY: sync takes no arguments and cannot fail.
    unsafe { sync() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    #[test]
    fn lists_names_kinds_and_private_sizes() {
        let t = crate::testutil::tmp("sys-list");
        std::fs::write(t.join("big"), vec![7u8; 1 << 20]).unwrap();
        std::fs::create_dir(t.join("sub")).unwrap();
        std::os::unix::fs::symlink("big", t.join("link")).unwrap();
        clone_file(&t.join("big"), &t.join("clone")).unwrap();
        std::fs::write(t.join("solo"), vec![1u8; 300_000]).unwrap();
        let mut e = list_path(&t).unwrap();
        e.sort_by(|a, b| a.name.cmp(&b.name));
        let names: Vec<_> = e.iter().map(|e| e.name_str().into_owned()).collect();
        assert_eq!(names, ["big", "clone", "link", "solo", "sub"]);
        let by = |n: &str| e.iter().find(|x| x.name_str() == n).unwrap().clone();
        assert_eq!(by("sub").kind, VDIR);
        assert_eq!(by("link").kind, VLNK);
        assert_eq!(by("big").len, 1 << 20);
        // Clones share every block: neither copy owns any of them privately.
        assert_eq!(by("big").private, 0);
        assert_eq!(by("clone").private, 0);
        let solo = by("solo");
        assert_eq!(solo.private, std::fs::metadata(t.join("solo")).unwrap().blocks() * 512);
        assert_eq!(solo.ino, std::fs::metadata(t.join("solo")).unwrap().ino());
        assert_eq!(solo.dev, std::fs::metadata(t.join("solo")).unwrap().dev());
    }

    #[test]
    fn free_bytes_is_sane() {
        let f = free_bytes(Path::new("/"));
        assert!(f > 0);
        assert!(f < 1 << 50);
    }
}
