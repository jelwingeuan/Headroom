//! `headroom agent --idle`: the background agent. It sleeps in a CoreFoundation run loop,
//! wakes once a minute (FSEvents batches arrive in the same loop), notes builds it sees
//! writing into projects, checks free space with one statvfs, and runs an autopilot pass only
//! when the budget is crossed (at most every 15 minutes). No polling of the disk otherwise.

use crate::state::{self, Ctx};
use crate::{autopilot, rules, sys};
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

type CFRef = *const c_void;

#[repr(C)]
struct FSEventStreamContext {
    version: isize,
    info: *mut c_void,
    retain: *const c_void,
    release: *const c_void,
    copy_description: *const c_void,
}

type Callback = extern "C" fn(CFRef, *mut c_void, usize, *mut c_void, *const u32, *const u64);

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopDefaultMode: CFRef;
    static kCFTypeArrayCallBacks: c_void;
    fn CFStringCreateWithCString(alloc: CFRef, s: *const c_char, encoding: u32) -> CFRef;
    fn CFArrayCreate(alloc: CFRef, values: *const CFRef, n: isize, callbacks: *const c_void) -> CFRef;
    fn CFRunLoopGetCurrent() -> CFRef;
    fn CFRunLoopRunInMode(mode: CFRef, seconds: f64, return_after_source: u8) -> i32;
}

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    fn FSEventStreamCreate(
        alloc: CFRef,
        cb: Callback,
        ctx: *mut FSEventStreamContext,
        paths: CFRef,
        since: u64,
        latency: f64,
        flags: u32,
    ) -> CFRef;
    fn FSEventStreamScheduleWithRunLoop(stream: CFRef, run_loop: CFRef, mode: CFRef);
    fn FSEventStreamStart(stream: CFRef) -> u8;
}

const UTF8: u32 = 0x0800_0100;
const SINCE_NOW: u64 = u64::MAX;
const RUN_FINISHED: i32 = 1;

static EVENTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

extern "C" fn on_events(
    _s: CFRef,
    _info: *mut c_void,
    n: usize,
    paths: *mut c_void,
    _flags: *const u32,
    _ids: *const u64,
) {
    let paths = paths as *const *const c_char;
    let mut q = EVENTS.lock().unwrap_or_else(|e| e.into_inner());
    for i in 0..n.min(10_000) {
        if q.len() >= 10_000 {
            break;
        }
        // SAFETY: FSEvents hands us `n` valid C strings (no kFSEventStreamCreateFlagUseCFTypes).
        let p = unsafe { CStr::from_ptr(*paths.add(i)) };
        q.push(p.to_string_lossy().into_owned());
    }
}

/// Watches `dir` on the current thread's run loop. False when FSEvents isn't available.
fn watch(dir: &Path) -> bool {
    let Ok(c) = CString::new(dir.to_string_lossy().as_bytes()) else { return false };
    // SAFETY: plain CoreFoundation/FSEvents calls with valid arguments; the stream lives for
    // the life of the process, so nothing is released.
    unsafe {
        let s = CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), UTF8);
        if s.is_null() {
            return false;
        }
        let arr = CFArrayCreate(std::ptr::null(), &s, 1, &kCFTypeArrayCallBacks as *const c_void);
        let mut ctx = FSEventStreamContext {
            version: 0,
            info: std::ptr::null_mut(),
            retain: std::ptr::null(),
            release: std::ptr::null(),
            copy_description: std::ptr::null(),
        };
        let stream = FSEventStreamCreate(std::ptr::null(), on_events, &mut ctx, arr, SINCE_NOW, 30.0, 0);
        if stream.is_null() {
            return false;
        }
        FSEventStreamScheduleWithRunLoop(stream, CFRunLoopGetCurrent(), kCFRunLoopDefaultMode);
        FSEventStreamStart(stream) != 0
    }
}

/// A write inside a build folder (node_modules, target, .next…) means that project is in use.
pub fn project_of_event(path: &str) -> Option<PathBuf> {
    let p = Path::new(path);
    let dirs: Vec<&str> = rules::RULES
        .iter()
        .flat_map(|r| r.dirs.iter().copied())
        .filter(|d| !d.contains('/') && !rules::is_maybe(d))
        .collect();
    let mut acc = PathBuf::new();
    for c in p.components() {
        let name = c.as_os_str().to_string_lossy();
        if dirs.contains(&&*name) && acc.components().count() > 2 {
            return Some(acc);
        }
        acc.push(c);
    }
    None
}

pub fn run(ctx: &Ctx) -> i32 {
    let watching = watch(&ctx.home);
    eprintln!("headroom agent: idle (watching {}: {watching})", ctx.home.display());
    let mut last_tick: Option<Instant> = None;
    loop {
        // SAFETY: runs this thread's run loop for up to 60 s; returns on timeout.
        let r = unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 60.0, 0) };
        if !watching || r == RUN_FINISHED {
            std::thread::sleep(Duration::from_secs(60)); // no sources: never spin
        }
        let events = std::mem::take(&mut *EVENTS.lock().unwrap_or_else(|e| e.into_inner()));
        let mut now_ctx = ctx.clone();
        now_ctx.now = state::wall_clock();
        // Writes that still exist (a deletion, Headroom's own included, is not use), outside ~/Library.
        let library = ctx.home.join("Library");
        let mut seen: Vec<(PathBuf, u64)> = events
            .iter()
            .filter(|e| !Path::new(e).starts_with(&library) && Path::new(e).exists())
            .filter_map(|e| project_of_event(e))
            .map(|p| (p, now_ctx.now))
            .collect();
        seen.sort();
        seen.dedup();
        state::observe(&now_ctx, &seen);
        let s = state::settings(&now_ctx);
        let due = last_tick.map_or(true, |t| t.elapsed() >= Duration::from_secs(15 * 60));
        if s.budget_bytes > 0 && due && sys::free_bytes(&ctx.home) < s.budget_bytes {
            last_tick = Some(Instant::now());
            now_ctx.recent_days = s.recent_days;
            autopilot::tick(&now_ctx, s.budget_bytes, &mut |e| eprintln!("{e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_writes_mark_their_project() {
        assert_eq!(
            project_of_event("/Users/a/code/app/node_modules/.vite/deps"),
            Some(PathBuf::from("/Users/a/code/app"))
        );
        assert_eq!(project_of_event("/Users/a/code/app/src/main.rs"), None);
        assert_eq!(project_of_event("/Users/a/code/rs/target/debug/x"), Some(PathBuf::from("/Users/a/code/rs")));
    }
}
