// spikes/ws-libcurl/src/ws.rs
//
// A thin safe layer over one libcurl WebSocket connection: connect, send a
// whole message, receive a whole message. This is roughly the shape
// `http/curl_ws_ffi.rs` would take in 13b, kept small so the spike measures
// libcurl rather than a design.
//
// Every `unsafe` block is an FFI call whose preconditions are local: a handle
// this struct owns and has not cleaned up, and buffers that outlive the call.
use std::ffi::CString;
use std::os::raw::{c_char, c_long, c_void};
use std::ptr;
use std::thread;
use std::time::{Duration, Instant};

use crate::ffi::{self, curl_socket_t, curl_ws_frame, CURL};
use crate::wait::wait_socket;

/// How the connection waits when libcurl says CURLE_AGAIN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wait {
    /// Sleep a fixed slice and try again.
    Sleep,
    /// Block in poll()/WSAPoll() on libcurl's socket.
    Socket,
}

pub const SLEEP_SLICE: Duration = Duration::from_millis(10);
const RECV_BUFFER: usize = 64 * 1024;

pub struct ConnectOptions<'a> {
    pub url: &'a str,
    pub headers: &'a [&'a str],
    pub timeout_ms: Option<c_long>,
    pub connect_timeout_ms: c_long,
    pub native_ca: bool,
    pub verify_tls: bool,
    pub proxy: Option<&'a str>,
    /// CURLWS_NOAUTOPONG: PINGs reach the app, which answers them itself.
    pub no_auto_pong: bool,
}

impl<'a> ConnectOptions<'a> {
    pub fn new(url: &'a str) -> Self {
        Self {
            url,
            headers: &[],
            timeout_ms: None,
            connect_timeout_ms: 10_000,
            native_ca: true,
            verify_tls: true,
            proxy: None,
            no_auto_pong: false,
        }
    }
}

#[derive(Debug)]
pub struct ConnectError {
    pub code: ffi::CURLcode,
    pub message: String,
    /// libcurl's error buffer, which is usually more specific than strerror.
    pub detail: String,
    /// CURLINFO_RESPONSE_CODE after the failed perform; 0 if none arrived.
    pub status: i64,
}

#[derive(Debug, Clone)]
pub struct CurlFailure {
    pub code: ffi::CURLcode,
    pub message: String,
}

impl CurlFailure {
    fn new(code: ffi::CURLcode) -> Self {
        Self {
            code,
            message: ffi::strerror(code),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FrameInfo {
    pub flags: u32,
    pub offset: i64,
    pub bytesleft: i64,
    pub len: usize,
}

#[derive(Debug)]
pub struct Message {
    /// Flags of the first chunk: its type (TEXT/BINARY/CLOSE/PING/PONG).
    pub flags: u32,
    pub data: Vec<u8>,
    /// Every chunk curl_ws_recv handed over for this message.
    pub chunks: Vec<FrameInfo>,
    /// Times the loop waited on CURLE_AGAIN before the message completed.
    pub waits: u32,
}

#[derive(Debug)]
pub enum RecvError {
    /// Nothing complete arrived before the deadline.
    Timeout {
        waits: u32,
    },
    Curl(CurlFailure),
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SendStats {
    pub calls: u32,
    pub partials: u32,
    pub agains: u32,
}

pub struct Connection {
    handle: *mut CURL,
    slist: *mut c_void,
    header_lines: Box<Vec<String>>,
    _error_buffer: Box<[u8; ffi::CURL_ERROR_SIZE]>,
    pub status: i64,
    #[cfg(not(raw_libcurl))]
    easy: Option<curl::easy::Easy2<crate::easy2::Collector>>,
}

extern "C" fn collect_header(
    data: *mut c_char,
    size: usize,
    count: usize,
    user: *mut c_void,
) -> usize {
    let total = size * count;
    // SAFETY: libcurl passes back the HEADERDATA pointer set in connect(), a
    // Vec<String> boxed inside the Connection that outlives the handle, and
    // `data` points at `total` readable bytes for the duration of the call.
    unsafe {
        let lines = &mut *(user as *mut Vec<String>);
        let bytes = std::slice::from_raw_parts(data as *const u8, total);
        lines.push(String::from_utf8_lossy(bytes).trim_end().to_string());
    }
    total
}

pub fn global_init() {
    // SAFETY: called once, before any other thread uses libcurl.
    unsafe {
        ffi::curl_global_init(ffi::CURL_GLOBAL_ALL);
    }
}

fn read_error_buffer(buffer: &[u8; ffi::CURL_ERROR_SIZE]) -> String {
    let end = buffer.iter().position(|b| *b == 0).unwrap_or(buffer.len());
    String::from_utf8_lossy(&buffer[..end]).into_owned()
}

impl Connection {
    /// Connect with a handle made by curl_easy_init, the way 13b's FFI module
    /// would if it did not go through the curl crate at all.
    pub fn connect(options: &ConnectOptions<'_>) -> Result<Self, ConnectError> {
        // SAFETY: curl_easy_init has no preconditions after global init.
        let handle = unsafe { ffi::curl_easy_init() };
        let mut connection = Self {
            handle,
            slist: ptr::null_mut(),
            header_lines: Box::new(Vec::new()),
            _error_buffer: Box::new([0; ffi::CURL_ERROR_SIZE]),
            status: 0,
            #[cfg(not(raw_libcurl))]
            easy: None,
        };
        let url = CString::new(options.url).unwrap_or_default();
        let proxy = options.proxy.map(|p| CString::new(p).unwrap_or_default());
        for header in options.headers {
            let line = CString::new(*header).unwrap_or_default();
            // SAFETY: curl_slist_append copies the string.
            connection.slist = unsafe { ffi::curl_slist_append(connection.slist, line.as_ptr()) };
        }
        let callback: ffi::HeaderCallback = collect_header;
        // SAFETY: a live handle; strings are copied by libcurl (since 7.17);
        // the error buffer and header Vec are boxed and live as long as the
        // handle; the slist lives until Drop.
        unsafe {
            let h = connection.handle;
            ffi::curl_easy_setopt(
                h,
                ffi::CURLOPT_ERRORBUFFER,
                connection._error_buffer.as_mut_ptr(),
            );
            ffi::curl_easy_setopt(h, ffi::CURLOPT_HEADERFUNCTION, callback);
            ffi::curl_easy_setopt(
                h,
                ffi::CURLOPT_HEADERDATA,
                &mut *connection.header_lines as *mut Vec<String> as *mut c_void,
            );
            ffi::curl_easy_setopt(h, ffi::CURLOPT_URL, url.as_ptr());
            ffi::curl_easy_setopt(h, ffi::CURLOPT_NOSIGNAL, 1 as c_long);
            ffi::curl_easy_setopt(h, ffi::CURLOPT_HTTP_VERSION, ffi::CURL_HTTP_VERSION_1_1);
            ffi::curl_easy_setopt(h, ffi::CURLOPT_CONNECT_ONLY, ffi::CONNECT_ONLY_WEBSOCKET);
            ffi::curl_easy_setopt(
                h,
                ffi::CURLOPT_CONNECTTIMEOUT_MS,
                options.connect_timeout_ms,
            );
            if let Some(timeout) = options.timeout_ms {
                ffi::curl_easy_setopt(h, ffi::CURLOPT_TIMEOUT_MS, timeout);
            }
            if options.native_ca {
                ffi::curl_easy_setopt(h, ffi::CURLOPT_SSL_OPTIONS, ffi::CURLSSLOPT_NATIVE_CA);
            }
            if !options.verify_tls {
                ffi::curl_easy_setopt(h, ffi::CURLOPT_SSL_VERIFYPEER, 0 as c_long);
                ffi::curl_easy_setopt(h, ffi::CURLOPT_SSL_VERIFYHOST, 0 as c_long);
            }
            if let Some(proxy) = &proxy {
                ffi::curl_easy_setopt(h, ffi::CURLOPT_PROXY, proxy.as_ptr());
            }
            if options.no_auto_pong {
                ffi::curl_easy_setopt(h, ffi::CURLOPT_WS_OPTIONS, ffi::CURLWS_NOAUTOPONG);
            }
            if !connection.slist.is_null() {
                ffi::curl_easy_setopt(h, ffi::CURLOPT_HTTPHEADER, connection.slist);
            }
        }
        // SAFETY: live handle, all options set above.
        let code = unsafe { ffi::curl_easy_perform(connection.handle) };
        connection.status = connection.response_code();
        if code != ffi::CURLE_OK {
            return Err(ConnectError {
                code,
                message: ffi::strerror(code),
                detail: read_error_buffer(&connection._error_buffer),
                status: connection.status,
            });
        }
        Ok(connection)
    }

    /// Adopt a handle whose transfer the curl crate's Easy2 performed. Proves
    /// the pattern 13b will use: Easy2 for setup, raw pointer for the frames.
    #[cfg(not(raw_libcurl))]
    #[allow(unused_mut)] // Easy2::raw takes &self in some curl versions, &mut self in others
    pub fn from_easy2(mut easy: curl::easy::Easy2<crate::easy2::Collector>, status: i64) -> Self {
        let handle = easy.raw() as *mut _ as *mut CURL;
        let lines = easy.get_ref().headers.clone();
        Self {
            handle,
            slist: ptr::null_mut(),
            header_lines: Box::new(lines),
            _error_buffer: Box::new([0; ffi::CURL_ERROR_SIZE]),
            status,
            easy: Some(easy),
        }
    }

    pub fn response_headers(&self) -> &[String] {
        &self.header_lines
    }

    fn response_code(&self) -> i64 {
        let mut code: c_long = 0;
        // SAFETY: live handle; RESPONSE_CODE writes one long.
        unsafe {
            ffi::curl_easy_getinfo(
                self.handle,
                ffi::CURLINFO_RESPONSE_CODE,
                &mut code as *mut c_long,
            );
        }
        i64::from(code)
    }

    pub fn socket(&self) -> Option<curl_socket_t> {
        #[cfg(windows)]
        let mut socket: curl_socket_t = usize::MAX;
        #[cfg(not(windows))]
        let mut socket: curl_socket_t = -1;
        // SAFETY: live handle; ACTIVESOCKET writes one curl_socket_t.
        let code = unsafe {
            ffi::curl_easy_getinfo(
                self.handle,
                ffi::CURLINFO_ACTIVESOCKET,
                &mut socket as *mut curl_socket_t,
            )
        };
        #[cfg(windows)]
        let bad = socket == usize::MAX;
        #[cfg(not(windows))]
        let bad = socket < 0;
        (code == ffi::CURLE_OK && !bad).then_some(socket)
    }

    fn wait(&self, mode: Wait, for_write: bool, deadline: Instant) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match (mode, self.socket()) {
            (Wait::Socket, Some(socket)) => {
                let ms = remaining.as_millis().min(i32::MAX as u128) as i32;
                wait_socket(socket, for_write, ms);
            }
            _ => thread::sleep(SLEEP_SLICE.min(remaining)),
        }
    }

    /// Send one whole message, following the curl_ws_send contract: CURLE_OK
    /// with `sent` short of the length means call again with the rest;
    /// CURLE_AGAIN means wait and call again with the same buffer.
    pub fn send(
        &self,
        flags: u32,
        data: &[u8],
        mode: Wait,
        timeout: Duration,
    ) -> Result<SendStats, CurlFailure> {
        let deadline = Instant::now() + timeout;
        let mut stats = SendStats::default();
        let mut offset = 0;
        loop {
            let remaining = &data[offset..];
            let mut sent = 0usize;
            // SAFETY: live handle; `remaining` is valid for its length for
            // the duration of the call.
            let code = unsafe {
                ffi::curl_ws_send(
                    self.handle,
                    remaining.as_ptr() as *const c_void,
                    remaining.len(),
                    &mut sent,
                    0,
                    flags,
                )
            };
            stats.calls += 1;
            match code {
                ffi::CURLE_OK => {
                    if sent < remaining.len() {
                        stats.partials += 1;
                    }
                    offset += sent;
                    if offset >= data.len() {
                        return Ok(stats);
                    }
                }
                ffi::CURLE_AGAIN => {
                    stats.agains += 1;
                    if Instant::now() >= deadline {
                        return Err(CurlFailure {
                            code,
                            message: format!("send still blocked after {timeout:?}"),
                        });
                    }
                    self.wait(mode, true, deadline);
                }
                other => return Err(CurlFailure::new(other)),
            }
        }
    }

    pub fn send_text(&self, text: &str, mode: Wait) -> Result<SendStats, CurlFailure> {
        self.send(
            ffi::CURLWS_TEXT,
            text.as_bytes(),
            mode,
            Duration::from_secs(10),
        )
    }

    pub fn send_close(
        &self,
        code: u16,
        reason: &str,
        mode: Wait,
    ) -> Result<SendStats, CurlFailure> {
        let mut payload = code.to_be_bytes().to_vec();
        payload.extend_from_slice(reason.as_bytes());
        self.send(ffi::CURLWS_CLOSE, &payload, mode, Duration::from_secs(5))
    }

    /// Receive one whole message, reassembling libcurl's chunks. A message is
    /// complete when a chunk has no bytes left in its frame and no CONT flag.
    pub fn recv(&self, mode: Wait, timeout: Duration) -> Result<Message, RecvError> {
        let deadline = Instant::now() + timeout;
        let mut buffer = vec![0u8; RECV_BUFFER];
        let mut message = Message {
            flags: 0,
            data: Vec::new(),
            chunks: Vec::new(),
            waits: 0,
        };
        loop {
            let mut received = 0usize;
            let mut meta: *const curl_ws_frame = ptr::null();
            // SAFETY: live handle; buffer valid for its length; libcurl writes
            // `received` and a pointer to frame metadata it owns.
            let code = unsafe {
                ffi::curl_ws_recv(
                    self.handle,
                    buffer.as_mut_ptr() as *mut c_void,
                    buffer.len(),
                    &mut received,
                    &mut meta,
                )
            };
            match code {
                ffi::CURLE_OK => {
                    // SAFETY: on CURLE_OK libcurl sets `meta` to a frame that
                    // stays valid until the next call on this handle; it is
                    // copied out immediately.
                    let frame = unsafe { *meta };
                    let flags = frame.flags as u32;
                    if message.chunks.is_empty() {
                        message.flags = flags;
                    }
                    message.chunks.push(FrameInfo {
                        flags,
                        offset: frame.offset,
                        bytesleft: frame.bytesleft,
                        len: frame.len,
                    });
                    message.data.extend_from_slice(&buffer[..received]);
                    if frame.bytesleft == 0 && flags & ffi::CURLWS_CONT == 0 {
                        return Ok(message);
                    }
                }
                ffi::CURLE_AGAIN => {
                    if Instant::now() >= deadline {
                        return Err(RecvError::Timeout {
                            waits: message.waits,
                        });
                    }
                    message.waits += 1;
                    self.wait(mode, false, deadline);
                }
                other => return Err(RecvError::Curl(CurlFailure::new(other))),
            }
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        #[cfg(not(raw_libcurl))]
        if self.easy.take().is_some() {
            // Easy2's own Drop ran curl_easy_cleanup.
            return;
        }
        // SAFETY: the handle and slist were created in connect() and are not
        // used after this point.
        unsafe {
            ffi::curl_easy_cleanup(self.handle);
            if !self.slist.is_null() {
                ffi::curl_slist_free_all(self.slist);
            }
        }
    }
}

pub fn flag_names(flags: u32) -> String {
    let names = [
        (ffi::CURLWS_TEXT, "TEXT"),
        (ffi::CURLWS_BINARY, "BINARY"),
        (ffi::CURLWS_CONT, "CONT"),
        (ffi::CURLWS_CLOSE, "CLOSE"),
        (ffi::CURLWS_PING, "PING"),
        (ffi::CURLWS_OFFSET, "OFFSET"),
        (ffi::CURLWS_PONG, "PONG"),
    ];
    let set: Vec<&str> = names
        .iter()
        .filter(|(bit, _)| flags & bit != 0)
        .map(|(_, n)| *n)
        .collect();
    if set.is_empty() {
        "0".into()
    } else {
        set.join("|")
    }
}
