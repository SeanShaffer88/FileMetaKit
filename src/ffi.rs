//! C ABI surface — everything declared in include/filemetakit.h.
//! Conventions (house standard, from WinRecovery):
//! - `*mut c_char` results are caller-owned; free with `fm_free_string`.
//! - Failures return null / an ERR_* code + a thread-local last-error message.
//! - Nothing unwinds across the FFI boundary (catch_unwind everywhere;
//!   release profile also sets panic = "abort").

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::panic::catch_unwind;

use crate::{ERR_INTERNAL, ERR_INVALID_ARG};

thread_local! {
    static LAST_ERROR: RefCell<(i32, CString)> =
        RefCell::new((0, CString::new("").unwrap()));
}

pub fn set_error(code: i32, msg: &str) {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = (
            code,
            CString::new(msg).unwrap_or_else(|_| CString::new("error").unwrap()),
        );
    });
}

#[no_mangle]
pub extern "C" fn fm_version() -> *const c_char {
    static VERSION_C: &[u8] = b"0.1.0\0";
    VERSION_C.as_ptr() as *const c_char
}

#[no_mangle]
pub extern "C" fn fm_last_error() -> *const c_char {
    LAST_ERROR.with(|e| e.borrow().1.as_ptr())
}

#[no_mangle]
pub extern "C" fn fm_last_error_code() -> c_int {
    LAST_ERROR.with(|e| e.borrow().0) as c_int
}

/// # Safety (caller contract): `p` is null or a string this library
/// returned and has not been freed yet. Documented in the header.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn fm_free_string(p: *mut c_char) {
    if !p.is_null() {
        unsafe {
            drop(CString::from_raw(p));
        }
    }
}

fn clear_error() {
    set_error(0, "");
}

fn to_c_string(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(c) => c.into_raw(),
        Err(_) => {
            set_error(ERR_INTERNAL, "result contained a NUL byte");
            std::ptr::null_mut()
        }
    }
}

/// # Safety: `p` is null or a valid NUL-terminated UTF-8 string.
unsafe fn arg_str<'a>(p: *const c_char, name: &str) -> Result<&'a str, (i32, String)> {
    if p.is_null() {
        return Err((ERR_INVALID_ARG, format!("{name} is null")));
    }
    // SAFETY: caller contract in the header.
    let cs = unsafe { CStr::from_ptr(p) };
    cs.to_str()
        .map_err(|_| (ERR_INVALID_ARG, format!("{name} is not valid UTF-8")))
}

fn run(f: impl FnOnce() -> Result<String, (i32, String)> + std::panic::UnwindSafe) -> *mut c_char {
    match catch_unwind(f) {
        Ok(Ok(s)) => {
            clear_error();
            to_c_string(s)
        }
        Ok(Err((code, msg))) => {
            set_error(code, &msg);
            std::ptr::null_mut()
        }
        Err(_) => {
            set_error(ERR_INTERNAL, "internal panic caught at FFI boundary");
            std::ptr::null_mut()
        }
    }
}

/// Read metadata. See header for the JSON shape.
/// # Safety: `path` is null or a valid NUL-terminated UTF-8 string.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn fm_read(path: *const c_char) -> *mut c_char {
    run(|| {
        let p = unsafe { arg_str(path, "path") }?.to_string();
        crate::meta::read_file_json(&p)
    })
}

/// Apply supported edits. See header.
/// # Safety: both arguments are null or valid NUL-terminated UTF-8 strings.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn fm_write(path: *const c_char, edits_json: *const c_char) -> *mut c_char {
    run(|| {
        let p = unsafe { arg_str(path, "path") }?.to_string();
        let e = unsafe { arg_str(edits_json, "edits_json") }?.to_string();
        crate::meta::write_file_json(&p, &e)
    })
}

/// Compare two files' metadata. See header.
/// # Safety: both arguments are null or valid NUL-terminated UTF-8 strings.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn fm_compare(path_a: *const c_char, path_b: *const c_char) -> *mut c_char {
    run(|| {
        let a = unsafe { arg_str(path_a, "path_a") }?.to_string();
        let b = unsafe { arg_str(path_b, "path_b") }?.to_string();
        crate::meta::compare_file_json(&a, &b)
    })
}
