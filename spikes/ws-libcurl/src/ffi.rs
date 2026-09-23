// spikes/ws-libcurl/src/ffi.rs
//
// The hand-written declarations this spike exists to prove. `curl-sys` has no
// binding for libcurl's WebSocket API (checked against 0.4.90+curl-8.21.0), so
// the three functions and the frame struct are declared here from
// include/curl/websockets.h of the libcurl that curl-sys builds.
//
// The general easy-handle functions are declared here too rather than taken
// from curl-sys, so the same source also builds with plain rustc against a
// self-built libcurl (the cloud run, `--cfg raw_libcurl`). In the cargo build
// the symbols resolve to curl-sys's statically linked libcurl.
#![allow(non_camel_case_types, dead_code)]

use std::os::raw::{c_char, c_int, c_long, c_uint, c_void};

pub type CURL = c_void;
pub type CURLcode = c_int;
/// libcurl's `curl_off_t` is 64-bit on every platform it supports.
pub type curl_off_t = i64;

#[cfg(windows)]
pub type curl_socket_t = usize;
#[cfg(not(windows))]
pub type curl_socket_t = c_int;

/// Mirrors `struct curl_ws_frame` in websockets.h, field for field.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct curl_ws_frame {
    pub age: c_int,
    pub flags: c_int,
    pub offset: curl_off_t,
    pub bytesleft: curl_off_t,
    pub len: usize,
}

// Frame flags (websockets.h).
pub const CURLWS_TEXT: c_uint = 1 << 0;
pub const CURLWS_BINARY: c_uint = 1 << 1;
pub const CURLWS_CONT: c_uint = 1 << 2;
pub const CURLWS_CLOSE: c_uint = 1 << 3;
pub const CURLWS_PING: c_uint = 1 << 4;
pub const CURLWS_OFFSET: c_uint = 1 << 5;
pub const CURLWS_PONG: c_uint = 1 << 6;
pub const CURLWS_NOAUTOPONG: c_long = 1 << 1;

// Options and infos (curl.h). The value is the option's type base plus its
// number: LONG = 0, OBJECTPOINT = 10000, FUNCTIONPOINT = 20000.
pub const CURLOPT_URL: c_int = 10_002;
pub const CURLOPT_PROXY: c_int = 10_004;
pub const CURLOPT_ERRORBUFFER: c_int = 10_010;
pub const CURLOPT_HTTPHEADER: c_int = 10_023;
pub const CURLOPT_HEADERDATA: c_int = 10_029;
pub const CURLOPT_SSL_VERIFYPEER: c_int = 64;
pub const CURLOPT_HEADERFUNCTION: c_int = 20_079;
pub const CURLOPT_SSL_VERIFYHOST: c_int = 81;
pub const CURLOPT_HTTP_VERSION: c_int = 84;
pub const CURLOPT_NOSIGNAL: c_int = 99;
pub const CURLOPT_CONNECT_ONLY: c_int = 141;
pub const CURLOPT_TIMEOUT_MS: c_int = 155;
pub const CURLOPT_CONNECTTIMEOUT_MS: c_int = 156;
pub const CURLOPT_SSL_OPTIONS: c_int = 216;
pub const CURLOPT_WS_OPTIONS: c_int = 320;

pub const CURL_HTTP_VERSION_1_1: c_long = 2;
pub const CURLSSLOPT_NATIVE_CA: c_long = 1 << 4;
/// `CONNECT_ONLY = 2` is what puts libcurl in WebSocket mode; the curl crate's
/// `connect_only(bool)` can only express 0 and 1.
pub const CONNECT_ONLY_WEBSOCKET: c_long = 2;

pub const CURLINFO_RESPONSE_CODE: c_int = 0x0020_0000 + 2;
pub const CURLINFO_ACTIVESOCKET: c_int = 0x0050_0000 + 44;

pub const CURL_GLOBAL_ALL: c_long = 3;

pub const CURLE_OK: CURLcode = 0;
pub const CURLE_GOT_NOTHING: CURLcode = 52;
pub const CURLE_AGAIN: CURLcode = 81;

pub const CURL_ERROR_SIZE: usize = 256;

pub type HeaderCallback = extern "C" fn(*mut c_char, usize, usize, *mut c_void) -> usize;

extern "C" {
    pub fn curl_global_init(flags: c_long) -> CURLcode;
    pub fn curl_version() -> *const c_char;
    pub fn curl_easy_init() -> *mut CURL;
    pub fn curl_easy_cleanup(handle: *mut CURL);
    pub fn curl_easy_setopt(handle: *mut CURL, option: c_int, ...) -> CURLcode;
    pub fn curl_easy_getinfo(handle: *mut CURL, info: c_int, ...) -> CURLcode;
    pub fn curl_easy_perform(handle: *mut CURL) -> CURLcode;
    pub fn curl_easy_strerror(code: CURLcode) -> *const c_char;
    pub fn curl_slist_append(list: *mut c_void, value: *const c_char) -> *mut c_void;
    pub fn curl_slist_free_all(list: *mut c_void);

    pub fn curl_ws_recv(
        handle: *mut CURL,
        buffer: *mut c_void,
        buflen: usize,
        recv: *mut usize,
        meta: *mut *const curl_ws_frame,
    ) -> CURLcode;
    pub fn curl_ws_send(
        handle: *mut CURL,
        buffer: *const c_void,
        buflen: usize,
        sent: *mut usize,
        fragsize: curl_off_t,
        flags: c_uint,
    ) -> CURLcode;
    pub fn curl_ws_meta(handle: *mut CURL) -> *const curl_ws_frame;
}

pub fn strerror(code: CURLcode) -> String {
    // SAFETY: curl_easy_strerror returns a pointer to a static, NUL-terminated
    // string for every code, including unknown ones.
    unsafe { std::ffi::CStr::from_ptr(curl_easy_strerror(code)) }
        .to_string_lossy()
        .into_owned()
}

pub fn version() -> String {
    // SAFETY: curl_version returns a static NUL-terminated string.
    unsafe { std::ffi::CStr::from_ptr(curl_version()) }
        .to_string_lossy()
        .into_owned()
}
