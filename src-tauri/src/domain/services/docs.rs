// http_client/src-tauri/src/domain/services/docs.rs
//
// Reading and writing an item's Markdown documentation (PLAN.md Phase 12).
//
// The rule this service owns is the size cap, and the dispatch from "which
// kind of item" to "which repository". Everything else — what Markdown means,
// how it is rendered, when the editor saves — belongs above it. Nothing here
// knows what a tab is.
use std::sync::Arc;

use crate::domain::error::AppError;
use crate::domain::ports::{CollectionRepository, FolderRepository, SavedRequestRepository};

/// The largest documentation a single item may hold, in bytes.
///
/// Refused rather than truncated, for the same reason an over-long example
/// body is (services/collections.rs): documentation that silently differs
/// from what the user typed is worse than a refusal they can see. A quarter
/// of a megabyte is around forty thousand words — far past any endpoint note
/// anyone writes, and small enough that a runaway paste cannot bloat the
/// app-data database.
pub const MAX_DOCS_BYTES: usize = 256 * 1024;

/// Which item some documentation belongs to.
///
/// A closed enum rather than a string: the three documentable kinds are fixed
/// by the schema, and anything else is a bug the compiler should catch rather
/// than a not-found at runtime. Examples are deliberately absent — they are
/// records of one exchange, not something to annotate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocsTarget {
    Collection,
    Folder,
    Request,
}

/// Use-cases over the three documentable aggregates. Cheap to clone (Arc),
/// like the other services, so the command layer can move a clone into a
/// blocking task.
#[derive(Clone)]
pub struct Docs {
    collections: Arc<dyn CollectionRepository>,
    folders: Arc<dyn FolderRepository>,
    requests: Arc<dyn SavedRequestRepository>,
}

impl Docs {
    pub fn new(
        collections: Arc<dyn CollectionRepository>,
        folders: Arc<dyn FolderRepository>,
        requests: Arc<dyn SavedRequestRepository>,
    ) -> Self {
        Self {
            collections,
            folders,
            requests,
        }
    }

    pub fn get(&self, target: DocsTarget, id: &str) -> Result<String, AppError> {
        match target {
            DocsTarget::Collection => self.collections.docs(id),
            DocsTarget::Folder => self.folders.docs(id),
            DocsTarget::Request => self.requests.docs(id),
        }
    }

    /// Replaces an item's documentation.
    ///
    /// The text is stored exactly as typed. Markdown is whitespace-sensitive
    /// — a trailing double space is a line break, and leading spaces make a
    /// code block — so unlike a name, documentation is never trimmed.
    pub fn set(&self, target: DocsTarget, id: &str, markdown: &str) -> Result<(), AppError> {
        if markdown.len() > MAX_DOCS_BYTES {
            return Err(AppError::InvalidRequest(format!(
                "this documentation is {} bytes and an item may keep at most {}",
                markdown.len(),
                MAX_DOCS_BYTES
            )));
        }

        match target {
            DocsTarget::Collection => self.collections.set_docs(id, markdown),
            DocsTarget::Folder => self.folders.set_docs(id, markdown),
            DocsTarget::Request => self.requests.set_docs(id, markdown),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::domain::models::{Collection, Folder, SavedRequest};

    /// One fake standing in for all three repositories, so a test can assert
    /// that the right one was reached by asking which label came back.
    #[derive(Default)]
    struct FakeDocs {
        label: &'static str,
        written: Mutex<Vec<(String, String)>>,
    }

    impl FakeDocs {
        fn new(label: &'static str) -> Arc<Self> {
            Arc::new(Self {
                label,
                written: Mutex::new(Vec::new()),
            })
        }

        fn writes(&self) -> Vec<(String, String)> {
            self.written.lock().expect("lock").clone()
        }
    }

    fn unused<T>() -> Result<T, AppError> {
        Err(AppError::Internal(
            "this service should not have been called".into(),
        ))
    }

    impl CollectionRepository for FakeDocs {
        fn list(&self) -> Result<Vec<Collection>, AppError> {
            unused()
        }
        fn create(&self, _name: &str) -> Result<Collection, AppError> {
            unused()
        }
        fn rename(&self, _id: &str, _name: &str) -> Result<(), AppError> {
            unused()
        }
        fn delete(&self, _id: &str) -> Result<(), AppError> {
            unused()
        }
        fn docs(&self, _id: &str) -> Result<String, AppError> {
            Ok(self.label.to_string())
        }
        fn set_docs(&self, id: &str, markdown: &str) -> Result<(), AppError> {
            self.written
                .lock()
                .expect("lock")
                .push((id.to_string(), markdown.to_string()));
            Ok(())
        }
    }

    impl FolderRepository for FakeDocs {
        fn list_by_collection(&self, _collection_id: &str) -> Result<Vec<Folder>, AppError> {
            unused()
        }
        fn create(
            &self,
            _collection_id: &str,
            _parent_folder_id: Option<&str>,
            _name: &str,
        ) -> Result<Folder, AppError> {
            unused()
        }
        fn rename(&self, _id: &str, _name: &str) -> Result<(), AppError> {
            unused()
        }
        fn delete(&self, _id: &str) -> Result<(), AppError> {
            unused()
        }
        fn docs(&self, _id: &str) -> Result<String, AppError> {
            Ok(self.label.to_string())
        }
        fn set_docs(&self, id: &str, markdown: &str) -> Result<(), AppError> {
            self.written
                .lock()
                .expect("lock")
                .push((id.to_string(), markdown.to_string()));
            Ok(())
        }
    }

    impl SavedRequestRepository for FakeDocs {
        fn list_by_collection(&self, _collection_id: &str) -> Result<Vec<SavedRequest>, AppError> {
            unused()
        }
        fn get(&self, _id: &str) -> Result<SavedRequest, AppError> {
            unused()
        }
        fn save(&self, _saved: &SavedRequest) -> Result<(), AppError> {
            unused()
        }
        fn rename(&self, _id: &str, _name: &str) -> Result<(), AppError> {
            unused()
        }
        fn move_to(&self, _id: &str, _folder_id: Option<&str>) -> Result<(), AppError> {
            unused()
        }
        fn delete(&self, _id: &str) -> Result<(), AppError> {
            unused()
        }
        fn docs(&self, _id: &str) -> Result<String, AppError> {
            Ok(self.label.to_string())
        }
        fn set_docs(&self, id: &str, markdown: &str) -> Result<(), AppError> {
            self.written
                .lock()
                .expect("lock")
                .push((id.to_string(), markdown.to_string()));
            Ok(())
        }
    }

    struct Fixture {
        docs: Docs,
        collections: Arc<FakeDocs>,
        folders: Arc<FakeDocs>,
        requests: Arc<FakeDocs>,
    }

    fn fixture() -> Fixture {
        let collections = FakeDocs::new("collection");
        let folders = FakeDocs::new("folder");
        let requests = FakeDocs::new("request");
        Fixture {
            docs: Docs::new(collections.clone(), folders.clone(), requests.clone()),
            collections,
            folders,
            requests,
        }
    }

    /// The whole job of the dispatch is getting these three the right way
    /// round; swapping two would be invisible without this.
    #[test]
    fn each_target_reaches_its_own_repository() {
        let fixture = fixture();

        assert_eq!(
            fixture.docs.get(DocsTarget::Collection, "id").expect("ok"),
            "collection"
        );
        assert_eq!(
            fixture.docs.get(DocsTarget::Folder, "id").expect("ok"),
            "folder"
        );
        assert_eq!(
            fixture.docs.get(DocsTarget::Request, "id").expect("ok"),
            "request"
        );
    }

    #[test]
    fn a_write_reaches_only_the_matching_repository() {
        let fixture = fixture();

        fixture
            .docs
            .set(DocsTarget::Folder, "fld_1", "text")
            .expect("should write");

        assert_eq!(
            fixture.folders.writes(),
            vec![("fld_1".to_string(), "text".to_string())]
        );
        assert!(fixture.collections.writes().is_empty());
        assert!(fixture.requests.writes().is_empty());
    }

    /// Markdown is whitespace-sensitive: two trailing spaces are a line
    /// break and four leading ones are a code block. Trimming documentation
    /// the way a name is trimmed would quietly change what the user wrote.
    #[test]
    fn documentation_is_stored_exactly_as_typed() {
        let fixture = fixture();
        let text = "    indented code\n\nline with a break  \n";

        fixture
            .docs
            .set(DocsTarget::Collection, "col_1", text)
            .expect("should write");

        assert_eq!(fixture.collections.writes()[0].1, text);
    }

    #[test]
    fn documentation_past_the_cap_is_refused_before_it_reaches_storage() {
        let fixture = fixture();

        let result = fixture.docs.set(
            DocsTarget::Request,
            "req_1",
            &"x".repeat(MAX_DOCS_BYTES + 1),
        );

        assert!(matches!(result, Err(AppError::InvalidRequest(_))));
        assert!(fixture.requests.writes().is_empty());
    }

    #[test]
    fn documentation_exactly_at_the_cap_is_accepted() {
        let fixture = fixture();

        fixture
            .docs
            .set(DocsTarget::Request, "req_1", &"x".repeat(MAX_DOCS_BYTES))
            .expect("should accept the cap itself");

        assert_eq!(fixture.requests.writes().len(), 1);
    }

    /// The cap counts bytes, because bytes are what the database stores.
    /// Measuring characters would let a document of multi-byte text run to
    /// four times the intended size.
    #[test]
    fn the_cap_counts_bytes_rather_than_characters() {
        let fixture = fixture();
        // Four bytes each, so a quarter as many characters reaches the cap.
        let over = "😀".repeat(MAX_DOCS_BYTES / 4 + 1);
        assert!(over.chars().count() < MAX_DOCS_BYTES);

        let result = fixture.docs.set(DocsTarget::Collection, "col_1", &over);

        assert!(matches!(result, Err(AppError::InvalidRequest(_))));
    }

    /// Empty is a legitimate value: it is how a user clears documentation.
    #[test]
    fn clearing_documentation_is_an_ordinary_write() {
        let fixture = fixture();

        fixture
            .docs
            .set(DocsTarget::Collection, "col_1", "")
            .expect("should write");

        assert_eq!(fixture.collections.writes()[0].1, "");
    }
}
