// http_client/src-tauri/build.rs
//
// The only change from tauri-build's default is the application manifest
// (windows/app.manifest): it adds the DPI-awareness declaration the Windows
// App Certification Kit looks for. On other platforms the attribute is
// ignored.
//
// `try_build` and `?` rather than `build`, which panics: CLAUDE.md section 7
// keeps `unwrap`/`expect` out of non-test code, and a returned error fails
// the build script with its message just the same.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let windows =
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("windows/app.manifest"));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))?;
    Ok(())
}
