// http_client/src-tauri/src/domain/services/validation.rs
//
// One name rule for every named entity — collections, folders, requests and
// environments all reach it here rather than each growing its own copy
// (CLAUDE.md section 7, DRY).
use crate::domain::error::AppError;

pub fn validated_name(name: &str) -> Result<&str, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidRequest("Name cannot be empty".into()));
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(
            validated_name("  Staging  ").expect("should accept"),
            "Staging"
        );
    }

    #[test]
    fn rejects_a_name_that_is_only_whitespace() {
        assert!(validated_name("   ").is_err());
    }
}
