//! Headroom's core, linked into the macOS app as libheadroom_core.a. The C ABI is three
//! functions (app/Sources/CHeadroom/headroom.h): the CLI/MCP entry point and a JSON call for
//! the UI with its matching free.

#![allow(clippy::too_many_arguments)]

pub mod json;

pub mod agent;
pub mod api;
pub mod autopilot;
pub mod cli;
pub mod cold;
pub mod delete;
pub mod git;
pub mod machine;
pub mod mcp;
pub mod proc;
pub mod rebuild;
pub mod reclaim;
pub mod rules;
pub mod scan;
pub mod sha256;
pub mod shrink;
pub mod state;
pub mod sys;

#[cfg(test)]
mod testutil;

use std::ffi::{c_char, CStr, CString};
use std::panic::catch_unwind;
use std::sync::Mutex;

/// Runs the CLI (or MCP server, or agent) with `argv` (argv[0] included); returns the exit status.
///
/// # Safety
/// `argv` must point to `argc` valid NUL-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn headroom_main(argc: i32, argv: *const *const c_char) -> i32 {
    let args: Vec<String> = (0..argc.max(0) as usize)
        .map(|i| {
            // SAFETY: the caller passes argc valid C strings.
            let p = unsafe { *argv.add(i) };
            if p.is_null() {
                String::new()
            } else {
                // SAFETY: p is a valid NUL-terminated string.
                unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
            }
        })
        .collect();
    catch_unwind(|| cli::main(args)).unwrap_or(70)
}

static CALLS: Mutex<()> = Mutex::new(());

/// One UI request as JSON; returns a JSON reply the caller must pass to `headroom_free`.
///
/// # Safety
/// `request` must be NULL or a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn headroom_call(request: *const c_char) -> *mut c_char {
    let req = if request.is_null() {
        String::new()
    } else {
        // SAFETY: request is a valid NUL-terminated string.
        unsafe { CStr::from_ptr(request) }.to_string_lossy().into_owned()
    };
    let _one_at_a_time = CALLS.lock().unwrap_or_else(|e| e.into_inner());
    let reply = catch_unwind(|| api::call(&req).to_string())
        .unwrap_or_else(|_| r#"{"error":"Headroom's core hit an internal error"}"#.into());
    CString::new(reply).unwrap_or_default().into_raw()
}

/// Frees a reply from `headroom_call`.
///
/// # Safety
/// `reply` must be NULL or a pointer returned by `headroom_call`, freed once.
#[no_mangle]
pub unsafe extern "C" fn headroom_free(reply: *mut c_char) {
    if !reply.is_null() {
        // SAFETY: reply came from CString::into_raw in headroom_call.
        drop(unsafe { CString::from_raw(reply) });
    }
}
