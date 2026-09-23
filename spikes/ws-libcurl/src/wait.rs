// spikes/ws-libcurl/src/wait.rs
//
// Block until libcurl's socket is readable or writable, instead of sleeping
// in a loop. One of the two strategies 13a measures; the other is a plain
// sleep. poll() on Unix, WSAPoll() on Windows.
use crate::ffi::curl_socket_t;

/// True if the socket became ready before `timeout_ms`.
pub fn wait_socket(socket: curl_socket_t, for_write: bool, timeout_ms: i32) -> bool {
    imp::wait(socket, for_write, timeout_ms)
}

#[cfg(unix)]
mod imp {
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
        fn poll(fds: *mut PollFd, nfds: c_ulong, timeout: c_int) -> c_int;
    }

    pub fn wait(socket: c_int, for_write: bool, timeout_ms: i32) -> bool {
        let mut fd = PollFd {
            fd: socket,
            events: if for_write { POLLOUT } else { POLLIN },
            revents: 0,
        };
        // SAFETY: one valid PollFd, count 1.
        unsafe { poll(&mut fd, 1, timeout_ms) > 0 }
    }
}

#[cfg(windows)]
mod imp {
    #[repr(C)]
    struct WsaPollFd {
        fd: usize,
        events: i16,
        revents: i16,
    }

    const POLLRDNORM: i16 = 0x0100;
    const POLLWRNORM: i16 = 0x0010;

    #[link(name = "ws2_32")]
    extern "system" {
        fn WSAPoll(fds: *mut WsaPollFd, count: u32, timeout: i32) -> i32;
    }

    pub fn wait(socket: usize, for_write: bool, timeout_ms: i32) -> bool {
        let mut fd = WsaPollFd {
            fd: socket,
            events: if for_write { POLLWRNORM } else { POLLRDNORM },
            revents: 0,
        };
        // SAFETY: one valid WsaPollFd, count 1.
        unsafe { WSAPoll(&mut fd, 1, timeout_ms) > 0 }
    }
}
