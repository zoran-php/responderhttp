// spikes/static-link-proof/src/main.rs
//
// Throwaway proof that libcurl ships inside the binary: one hardcoded HTTPS
// request, print the status code. Run it on a machine with no dev tools.
//
// A missing DLL stops the process before main() runs, so if this prints
// anything at all, libcurl, TLS, HTTP/2 and the C runtime are linked in.
// A curl error after that is a configuration problem (e.g. CA roots), not a
// linking problem.
use std::process::ExitCode;
use std::time::Duration;

use curl::easy::{Easy, HttpVersion, SslOpt};

const PROOF_URL: &str = "https://example.com/";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

fn main() -> ExitCode {
    print_build_info();
    match fetch_status(PROOF_URL) {
        Ok(status) => {
            println!("status:  {status}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("request failed: curl error {}: {err}", err.code());
            ExitCode::FAILURE
        }
    }
}

fn print_build_info() {
    let version = curl::Version::get();
    println!("libcurl: {}", version.version());
    println!("tls:     {}", version.ssl_version().unwrap_or("none"));
    println!("zlib:    {}", version.libz_version().unwrap_or("none"));
    println!("http2:   {}", version.feature_http2());
}

fn fetch_status(url: &str) -> Result<u32, curl::Error> {
    let mut easy = Easy::new();
    easy.url(url)?;
    easy.http_version(HttpVersion::V2TLS)?;
    // rustls carries no trust store of its own, so libcurl has no verifier
    // unless told where roots come from. Use the OS store (the Windows cert
    // store here): it ships with the OS, so no CA file travels with the binary.
    easy.ssl_options(SslOpt::new().native_ca(true))?;
    easy.timeout(REQUEST_TIMEOUT)?;
    // Without a write callback libcurl prints the body to stdout.
    easy.write_function(|body| Ok(body.len()))?;
    easy.perform()?;
    easy.response_code()
}
