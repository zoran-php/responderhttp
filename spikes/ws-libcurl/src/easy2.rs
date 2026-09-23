// spikes/ws-libcurl/src/easy2.rs
//
// Cargo build only. Sets up the handshake through the curl crate's Easy2,
// exactly as CurlClient builds a request today, then hands the raw handle to
// the WebSocket FFI. This is the integration 13b depends on: the crate for
// everything it can express, raw calls only for CONNECT_ONLY=2 and the frames.
use std::os::raw::c_long;

use curl::easy::{Easy2, Handler, HttpVersion, SslOpt, WriteError};

use crate::ffi;
use crate::ws::{ConnectError, Connection};

pub struct Collector {
    pub headers: Vec<String>,
}

impl Handler for Collector {
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        Ok(data.len())
    }

    fn header(&mut self, data: &[u8]) -> bool {
        self.headers
            .push(String::from_utf8_lossy(data).trim_end().to_string());
        true
    }
}

fn failure(error: curl::Error, status: i64) -> ConnectError {
    ConnectError {
        code: error.code() as ffi::CURLcode,
        message: error.description().to_string(),
        detail: error.extra_description().unwrap_or_default().to_string(),
        status,
    }
}

pub fn connect(url: &str) -> Result<Connection, ConnectError> {
    let mut easy = Easy2::new(Collector {
        headers: Vec::new(),
    });
    easy.url(url).map_err(|e| failure(e, 0))?;
    easy.http_version(HttpVersion::V11)
        .map_err(|e| failure(e, 0))?;
    easy.ssl_options(SslOpt::new().native_ca(true))
        .map_err(|e| failure(e, 0))?;
    easy.signal(false).map_err(|e| failure(e, 0))?;
    // SAFETY: a live handle owned by `easy`; CONNECT_ONLY takes a long.
    let code = unsafe {
        ffi::curl_easy_setopt(
            easy.raw() as *mut _ as *mut ffi::CURL,
            ffi::CURLOPT_CONNECT_ONLY,
            ffi::CONNECT_ONLY_WEBSOCKET as c_long,
        )
    };
    if code != ffi::CURLE_OK {
        return Err(ConnectError {
            code,
            message: ffi::strerror(code),
            detail: "setting CONNECT_ONLY=2".into(),
            status: 0,
        });
    }
    let performed = easy.perform();
    let status = easy.response_code().map(i64::from).unwrap_or(0);
    performed.map_err(|e| failure(e, status))?;
    Ok(Connection::from_easy2(easy, status))
}
