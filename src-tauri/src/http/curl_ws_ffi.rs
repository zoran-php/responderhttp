// http_client/src-tauri/src/http/curl_ws_ffi.rs
//
// The only module in this crate allowed to use `unsafe`: lib.rs denies it
// everywhere, and http/mod.rs lifts that for this file alone (CLAUDE.md
// section 11).
//
// Why it exists: `curl-sys` has no binding for libcurl's WebSocket API, and
// the `curl` crate's `connect_only(bool)` cannot express the value 2 that
// WebSocket mode needs. The declarations below mirror
// include/curl/websockets.h of the libcurl that curl-sys builds and links
// statically (8.21.0-DEV). The PLAN.md Phase 13a spike proved these exact
// declarations against that exact libcurl on Windows before anything was
// built on them.
//
// The general setopt/getinfo/strerror functions are declared here too, with
// our own types, rather than taken from curl-sys: the WebSocket option and
// info numbers are not in curl-sys either, and one consistent set of
// declarations in one file is easier to audit than a mix.
//
// Safety model, which every `unsafe` block below relies on: each function
// takes `&mut Easy2<H>`. The handle behind it is alive for the borrow, and
// the exclusive borrow guarantees nothing else uses it at the same time,
// which is libcurl's own rule for an easy handle. Buffers passed to libcurl
// outlive the call, and nothing libcurl returns is kept past it.
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_long, c_uint, c_void};
use std::ptr;
use std::time::Duration;

use curl::easy::Easy2;

// Frame flags (websockets.h).
pub const CURLWS_TEXT: u32 = 1 << 0;
pub const CURLWS_BINARY: u32 = 1 << 1;
pub const CURLWS_CONT: u32 = 1 << 2;
pub const CURLWS_CLOSE: u32 = 1 << 3;
pub const CURLWS_PING: u32 = 1 << 4;
pub const CURLWS_PONG: u32 = 1 << 6;

// Option and info numbers (curl.h): the type base plus the option's number,
// LONG = 0 and SOCKET info = 0x500000.
const CURLOPT_CONNECT_ONLY: c_int = 141;
const CURLOPT_WS_OPTIONS: c_int = 320;
const CURLINFO_ACTIVESOCKET: c_int = 0x0050_0000 + 44;

/// `CONNECT_ONLY = 2` is what puts libcurl in WebSocket mode.
const CONNECT_ONLY_WEBSOCKET: c_long = 2;
/// Pings are handed to the app, which answers them at once. libcurl's own
/// pong is only flushed on the app's next send or data frame, so a
/// connection that is only listening never answers (PLAN.md Phase 13a,
/// finding 1).
const CURLWS_NOAUTOPONG: c_long = 1 << 1;

const CURLE_OK: c_int = 0;
const CURLE_GOT_NOTHING: c_int = 52;
const CURLE_AGAIN: c_int = 81;

/// libcurl's `curl_socket_t`.
#[cfg(windows)]
pub type Socket = usize;
#[cfg(not(windows))]
pub type Socket = c_int;

#[cfg(windows)]
const BAD_SOCKET: Socket = usize::MAX;
#[cfg(not(windows))]
const BAD_SOCKET: Socket = -1;

/// Mirrors `struct curl_ws_frame` in websockets.h, field for field.
/// `curl_off_t` is 64-bit on every platform libcurl supports.
#[repr(C)]
#[derive(Clone, Copy)]
struct CurlWsFrame {
    age: c_int,
    flags: c_int,
    offset: i64,
    bytesleft: i64,
    len: usize,
}

extern "C" {
    fn curl_easy_setopt(handle: *mut c_void, option: c_int, ...) -> c_int;
    fn curl_easy_getinfo(handle: *mut c_void, info: c_int, ...) -> c_int;
    fn curl_easy_strerror(code: c_int) -> *const c_char;
    fn curl_ws_recv(
        handle: *mut c_void,
        buffer: *mut c_void,
        buflen: usize,
        recv: *mut usize,
        meta: *mut *const CurlWsFrame,
    ) -> c_int;
    fn curl_ws_send(
        handle: *mut c_void,
        buffer: *const c_void,
        buflen: usize,
        sent: *mut usize,
        fragsize: i64,
        flags: c_uint,
    ) -> c_int;
}

/// A libcurl result code. Kept as a number so the text is only looked up
/// when someone reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurlCode(pub i32);

impl CurlCode {
    pub fn describe(self) -> String {
        // SAFETY: curl_easy_strerror returns a pointer to a static,
        // NUL-terminated string for every code, known or not.
        unsafe { CStr::from_ptr(curl_easy_strerror(self.0)) }
            .to_string_lossy()
            .into_owned()
    }
}

fn raw<H>(easy: &mut Easy2<H>) -> *mut c_void {
    easy.raw().cast()
}

fn check(code: c_int) -> Result<(), CurlCode> {
    if code == CURLE_OK {
        Ok(())
    } else {
        Err(CurlCode(code))
    }
}

/// Must be called before `perform`. After it, `perform` stops once the
/// upgrade is done and leaves the connection open for `receive` and `send`.
pub fn enable_websocket_mode<H>(easy: &mut Easy2<H>) -> Result<(), CurlCode> {
    let handle = raw(easy);
    // SAFETY: a live, exclusively borrowed handle; both options take a long.
    unsafe {
        check(curl_easy_setopt(
            handle,
            CURLOPT_CONNECT_ONLY,
            CONNECT_ONLY_WEBSOCKET,
        ))?;
        check(curl_easy_setopt(
            handle,
            CURLOPT_WS_OPTIONS,
            CURLWS_NOAUTOPONG,
        ))
    }
}

/// What one `curl_ws_recv` call produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Received {
    /// `len` bytes of a frame are in the buffer; `bytes_left` more of the
    /// same frame are still to come.
    Chunk {
        flags: u32,
        bytes_left: u64,
        len: usize,
    },
    /// Nothing to read right now.
    Again,
    /// The connection is gone (`CURLE_GOT_NOTHING`).
    Closed,
    Failed(CurlCode),
}

pub fn receive<H>(easy: &mut Easy2<H>, buffer: &mut [u8]) -> Received {
    let mut received = 0usize;
    let mut meta: *const CurlWsFrame = ptr::null();
    // SAFETY: a live, exclusively borrowed handle; `buffer` is valid for its
    // length for the whole call; libcurl writes `received` and `meta`.
    let code = unsafe {
        curl_ws_recv(
            raw(easy),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut received,
            &mut meta,
        )
    };
    match code {
        CURLE_OK if !meta.is_null() => {
            // SAFETY: on success libcurl points `meta` at frame data it owns,
            // valid until the next call on this handle. It is copied out here,
            // before anything else can touch the handle.
            let frame = unsafe { *meta };
            Received::Chunk {
                flags: frame.flags as u32,
                bytes_left: u64::try_from(frame.bytesleft).unwrap_or(0),
                len: received.min(buffer.len()),
            }
        }
        // Success without frame data would be a libcurl bug; reported rather
        // than trusted.
        CURLE_OK => Received::Failed(CurlCode(CURLE_OK)),
        CURLE_AGAIN => Received::Again,
        CURLE_GOT_NOTHING => Received::Closed,
        other => Received::Failed(CurlCode(other)),
    }
}

/// What one `curl_ws_send` call managed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    /// This many payload bytes were taken. Fewer than offered means call
    /// again with the rest (PLAN.md Phase 13a, finding 7).
    Bytes(usize),
    /// Blocked: wait until writable and call again with the same buffer.
    Again,
    Failed(CurlCode),
}

pub fn send<H>(easy: &mut Easy2<H>, data: &[u8], flags: u32) -> Written {
    let mut sent = 0usize;
    // SAFETY: a live, exclusively borrowed handle; `data` is valid for its
    // length for the whole call; libcurl writes `sent`. A fragsize of 0 sends
    // `data` as one whole frame.
    let code = unsafe {
        curl_ws_send(
            raw(easy),
            data.as_ptr().cast(),
            data.len(),
            &mut sent,
            0,
            flags as c_uint,
        )
    };
    match code {
        CURLE_OK => Written::Bytes(sent.min(data.len())),
        CURLE_AGAIN => Written::Again,
        other => Written::Failed(CurlCode(other)),
    }
}

/// The socket libcurl is using, for waiting on instead of sleeping.
pub fn active_socket<H>(easy: &mut Easy2<H>) -> Option<Socket> {
    let mut socket: Socket = BAD_SOCKET;
    // SAFETY: a live, exclusively borrowed handle; ACTIVESOCKET writes one
    // curl_socket_t through the pointer.
    let code =
        unsafe { curl_easy_getinfo(raw(easy), CURLINFO_ACTIVESOCKET, &mut socket as *mut Socket) };
    (code == CURLE_OK && socket != BAD_SOCKET).then_some(socket)
}

/// Blocks until the socket is readable (or writable), or the timeout
/// passes. True when it became ready. Waking up early is harmless: callers
/// ask libcurl again either way.
pub fn wait_for_socket(socket: Socket, for_write: bool, timeout: Duration) -> bool {
    let millis = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
    poll::wait(socket, for_write, millis)
}

#[cfg(windows)]
mod poll {
    #[repr(C)]
    struct WsaPollFd {
        fd: usize,
        events: i16,
        revents: i16,
    }

    const POLLRDNORM: i16 = 0x0100;
    const POLLWRNORM: i16 = 0x0010;

    // ws2_32 is already linked by libcurl.
    #[link(name = "ws2_32")]
    extern "system" {
        fn WSAPoll(fds: *mut WsaPollFd, count: u32, timeout: i32) -> i32;
    }

    pub fn wait(socket: usize, for_write: bool, timeout: i32) -> bool {
        let mut fd = WsaPollFd {
            fd: socket,
            events: if for_write { POLLWRNORM } else { POLLRDNORM },
            revents: 0,
        };
        // SAFETY: one valid, initialised WsaPollFd and a count of one.
        unsafe { WSAPoll(&mut fd, 1, timeout) > 0 }
    }
}

#[cfg(not(windows))]
mod poll {
    use std::os::raw::{c_int, c_short, c_ulong};

    #[repr(C)]
    struct PollFd {
        fd: c_int,
        events: c_short,
        revents: c_short,
    }

    const POLLIN: c_short = 0x001;
    const POLLOUT: c_short = 0x004;

    extern "C" {
        fn poll(fds: *mut PollFd, count: c_ulong, timeout: c_int) -> c_int;
    }

    pub fn wait(socket: c_int, for_write: bool, timeout: i32) -> bool {
        let mut fd = PollFd {
            fd: socket,
            events: if for_write { POLLOUT } else { POLLIN },
            revents: 0,
        };
        // SAFETY: one valid, initialised PollFd and a count of one.
        unsafe { poll(&mut fd, 1, timeout) > 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_code_describes_itself() {
        assert_eq!(CurlCode(CURLE_OK).describe(), "No error");
        assert!(!CurlCode(CURLE_GOT_NOTHING).describe().is_empty());
    }
}
