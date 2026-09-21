// http_client/src-tauri/src/domain/mime.rs
//
// One extension ⇄ media-type table, used in both directions: naming a
// downloaded file from its Content-Type, and labelling an uploaded file part
// from its extension. Two tables would be two chances to disagree about what
// a .csv is (CLAUDE.md section 7, DRY).
//
// Deliberately short. This is a lookup for convenience — a wrong guess costs
// the user a retyped Content-Type, so a sprawling table earns nothing.

/// Extension (lowercase, no dot) paired with its media type.
const TABLE: [(&str, &str); 16] = [
    ("json", "application/json"),
    ("xml", "application/xml"),
    ("html", "text/html"),
    ("htm", "text/html"),
    ("csv", "text/csv"),
    ("txt", "text/plain"),
    ("md", "text/markdown"),
    ("pdf", "application/pdf"),
    ("zip", "application/zip"),
    ("gz", "application/gzip"),
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("svg", "image/svg+xml"),
    ("webp", "image/webp"),
];

/// What to label an uploaded file part. None means "let the server work it
/// out", which is better than asserting application/octet-stream over
/// something the table simply has not heard of.
pub fn content_type_for_extension(extension: &str) -> Option<&'static str> {
    let wanted = extension.to_ascii_lowercase();
    TABLE
        .iter()
        .find(|(ext, _)| *ext == wanted)
        .map(|(_, media_type)| *media_type)
}

/// What to call a downloaded file. The first extension listed for a media
/// type wins, so image/jpeg suggests .jpg rather than .jpeg.
pub fn extension_for_content_type(content_type: &str) -> Option<&'static str> {
    let wanted = content_type.to_ascii_lowercase();
    TABLE
        .iter()
        .find(|(_, media_type)| *media_type == wanted)
        .map(|(ext, _)| *ext)
        .or(match wanted.as_str() {
            // Not in the table because it has no meaningful extension going
            // the other way: nothing should be uploaded *as* octet-stream
            // just because it is called .bin.
            "application/octet-stream" => Some("bin"),
            "text/xml" => Some("xml"),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_an_extension_to_its_media_type_case_insensitively() {
        assert_eq!(content_type_for_extension("PNG"), Some("image/png"));
        assert_eq!(content_type_for_extension("json"), Some("application/json"));
    }

    #[test]
    fn an_unknown_extension_gets_no_guess_rather_than_octet_stream() {
        assert_eq!(content_type_for_extension("xyz"), None);
    }

    #[test]
    fn the_first_extension_listed_wins_going_back() {
        assert_eq!(extension_for_content_type("image/jpeg"), Some("jpg"));
    }

    #[test]
    fn handles_the_two_types_that_only_make_sense_one_way() {
        assert_eq!(
            extension_for_content_type("application/octet-stream"),
            Some("bin")
        );
        assert_eq!(extension_for_content_type("text/xml"), Some("xml"));
        assert_eq!(content_type_for_extension("bin"), None);
    }
}
