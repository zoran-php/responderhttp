// http_client/src-tauri/src/domain/services/downloads.rs
//
// Writing a response body to a file the user chose.
//
// There is deliberately no `FileWriter` port here. A trait with one
// implementation and no test seam is speculative (CLAUDE.md section 7,
// YAGNI): these functions are already testable against a temp directory,
// and the filesystem is not something a second adapter is coming for.
//
// The file chooser itself is *not* here — a native dialog is a platform
// concern and lives in the command layer, which is the adapter to the
// platform. This service only decides what to write and what to call it.
use std::path::Path;

use crate::domain::error::AppError;
use crate::domain::mime::extension_for_content_type;
use crate::domain::models::{KeyValue, ResponseBody};

const FALLBACK_FILE_NAME: &str = "response";

#[derive(Clone, Default)]
pub struct Downloads;

impl Downloads {
    pub fn new() -> Self {
        Self
    }

    /// Returns how many bytes landed on disk, which is what the UI reports.
    pub fn save(&self, path: &Path, body: &ResponseBody) -> Result<u64, AppError> {
        write(path, body.as_bytes())
    }

    /// Text the app generated rather than received — the OpenAPI export. Same
    /// error mapping, so a read-only directory reads the same whichever button
    /// the user pressed.
    pub fn save_text(&self, path: &Path, text: &str) -> Result<u64, AppError> {
        write(path, text.as_bytes())
    }
}

fn write(path: &Path, bytes: &[u8]) -> Result<u64, AppError> {
    std::fs::write(path, bytes).map_err(|error| {
        AppError::Storage(format!("could not write {}: {error}", path.display()))
    })?;
    Ok(bytes.len() as u64)
}

/// What the Save As dialog opens with. `Content-Disposition` first because a
/// server that bothers to send one has named the file itself; then the URL's
/// last path segment, which is right for the `/files/report.pdf` shape; then
/// a fallback with an extension guessed from `Content-Type`.
pub fn suggested_file_name(url: &str, headers: &[KeyValue]) -> String {
    if let Some(name) = disposition_file_name(headers) {
        return name;
    }
    if let Some(name) = url_file_name(url) {
        return name;
    }
    match content_type(headers).and_then(extension_for_content_type) {
        Some(extension) => format!("{FALLBACK_FILE_NAME}.{extension}"),
        None => FALLBACK_FILE_NAME.to_string(),
    }
}

fn disposition_file_name(headers: &[KeyValue]) -> Option<String> {
    let value = find_header(headers, "content-disposition")?;
    // filename*= (RFC 5987) is deliberately not handled: it carries a
    // charset and percent-encoding, and getting it half-right would produce
    // a worse name than falling through to the URL.
    let start = value.to_ascii_lowercase().find("filename=")? + "filename=".len();
    let raw = value[start..].split(';').next()?.trim();
    let unquoted = raw.trim_matches('"');
    sanitise(unquoted)
}

fn url_file_name(url: &str) -> Option<String> {
    let without_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let path = without_scheme.split(['?', '#']).next()?;
    let last = path.rsplit('/').next()?;
    sanitise(last)
}

/// A name from a header or a URL is untrusted input heading for a filesystem
/// path. Separators and traversal are stripped so a server cannot steer the
/// save dialog out of the directory the user picked.
fn sanitise(candidate: &str) -> Option<String> {
    let cleaned: String = candidate
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .filter(|c| !c.is_control())
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

fn content_type(headers: &[KeyValue]) -> Option<&str> {
    let value = find_header(headers, "content-type")?;
    Some(value.split(';').next().unwrap_or(value).trim())
}

fn find_header<'a>(headers: &'a [KeyValue], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(name: &str, value: &str) -> KeyValue {
        KeyValue::new(name, value)
    }

    #[test]
    fn content_disposition_wins_because_the_server_named_the_file_itself() {
        let headers = vec![
            header("Content-Type", "application/pdf"),
            header(
                "Content-Disposition",
                "attachment; filename=\"q3-report.pdf\"",
            ),
        ];

        assert_eq!(
            suggested_file_name("https://api.example.com/files/12345", &headers),
            "q3-report.pdf"
        );
    }

    #[test]
    fn the_header_name_is_matched_case_insensitively_and_unquoted() {
        let headers = vec![header(
            "content-disposition",
            "attachment; FileName=data.csv",
        )];

        assert_eq!(suggested_file_name("https://x.test/", &headers), "data.csv");
    }

    #[test]
    fn falls_back_to_the_last_url_segment() {
        assert_eq!(
            suggested_file_name("https://cdn.example.com/assets/logo.png?v=3", &[]),
            "logo.png"
        );
    }

    #[test]
    fn a_url_with_no_segment_falls_back_to_the_content_type() {
        let headers = vec![header("Content-Type", "application/json; charset=utf-8")];

        assert_eq!(
            suggested_file_name("https://api.example.com/", &headers),
            "response.json"
        );
    }

    #[test]
    fn an_unknown_content_type_gets_no_extension_rather_than_a_wrong_one() {
        let headers = vec![header("Content-Type", "application/vnd.example.thing")];

        assert_eq!(
            suggested_file_name("https://api.example.com/", &headers),
            "response"
        );
    }

    /// A server should not be able to steer the save dialog out of the
    /// directory the user picked.
    #[test]
    fn a_traversing_name_is_stripped_to_something_harmless() {
        let headers = vec![header(
            "Content-Disposition",
            "attachment; filename=\"../../etc/passwd\"",
        )];

        assert_eq!(
            suggested_file_name("https://x.test/", &headers),
            "etcpasswd"
        );
    }

    #[test]
    fn a_disposition_with_nothing_usable_falls_through_to_the_url() {
        let headers = vec![header("Content-Disposition", "attachment; filename=\"  \"")];

        assert_eq!(
            suggested_file_name("https://x.test/report.txt", &headers),
            "report.txt"
        );
    }

    #[test]
    fn writes_the_bytes_of_a_binary_body_verbatim() {
        let dir = std::env::temp_dir().join(format!("responderhttp-dl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir should be creatable");
        let path = dir.join("out.bin");
        let body = ResponseBody::Binary {
            bytes: vec![0x89, 0x50, 0x4e, 0x47],
        };

        let written = Downloads::new().save(&path, &body).expect("should write");

        assert_eq!(written, 4);
        assert_eq!(
            std::fs::read(&path).expect("should read back"),
            body.as_bytes()
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn writes_generated_text_through_the_same_path() {
        let dir =
            std::env::temp_dir().join(format!("responderhttp-dl-text-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir should be creatable");
        let path = dir.join("out.json");
        let text = "{\n  \"openapi\": \"3.2.0\"\n}";

        let written = Downloads::new()
            .save_text(&path, text)
            .expect("should write");

        assert_eq!(written, text.len() as u64);
        assert_eq!(
            std::fs::read_to_string(&path).expect("should read back"),
            text
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_path_that_cannot_be_written_surfaces_rather_than_reporting_success() {
        let body = ResponseBody::Text("hello".into());

        let error = Downloads::new()
            .save(
                Path::new("/nonexistent-directory-responderhttp/out.txt"),
                &body,
            )
            .expect_err("should fail");

        assert!(matches!(error, AppError::Storage(_)));
    }
}
