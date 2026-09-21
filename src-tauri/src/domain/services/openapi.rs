// http_client/src-tauri/src/domain/services/openapi.rs
//
// "Export this collection as an OpenAPI document." Gathers the four aggregates
// the mapping needs, hands them to the pure function in openapi/from_collection,
// and serialises the result.
//
// **This service has no environment repository, on purpose.** Resolving
// `{{api_key}}` on the way out would write a live credential into a file the
// user is about to commit to git. The mapping emits `{api_key}` as a Server
// Variable with a placeholder default instead, and the only way to keep that
// guarantee is to not have the means to break it. Adding an
// `Arc<dyn EnvironmentRepository>` field here is the change to argue about in
// review, not the line that uses it.
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::domain::error::AppError;
use crate::domain::models::{Example, Folder, SavedRequest};
use crate::domain::ports::{
    CollectionRepository, ExampleRepository, FolderRepository, SavedRequestRepository,
};
use crate::openapi::document::OpenApiVersion;
use crate::openapi::format::{render, ExportFormat};
use crate::openapi::from_collection::{
    document_file_name, to_document, ExportDocs, ExportInput, ExportNote,
};

/// The document, ready to write, plus everything the mapping could not say.
pub struct ExportedDocument {
    pub file_name: String,
    /// JSON or YAML, as asked for.
    pub text: String,
    pub notes: Vec<ExportNote>,
}

/// Cheap to clone (Arc), so the command layer can move a clone into a
/// blocking task like every other service here does.
#[derive(Clone)]
pub struct OpenApiExport {
    collections: Arc<dyn CollectionRepository>,
    folders: Arc<dyn FolderRepository>,
    requests: Arc<dyn SavedRequestRepository>,
    examples: Arc<dyn ExampleRepository>,
}

impl OpenApiExport {
    pub fn new(
        collections: Arc<dyn CollectionRepository>,
        folders: Arc<dyn FolderRepository>,
        requests: Arc<dyn SavedRequestRepository>,
        examples: Arc<dyn ExampleRepository>,
    ) -> Self {
        Self {
            collections,
            folders,
            requests,
            examples,
        }
    }

    /// `include_examples` is opt-in and defaults off in the UI: a saved
    /// response body can contain a token the server handed back, and an export
    /// is a file that leaves the machine.
    pub fn export(
        &self,
        collection_id: &str,
        version: OpenApiVersion,
        format: ExportFormat,
        include_examples: bool,
    ) -> Result<ExportedDocument, AppError> {
        let collection = self
            .collections
            .list()?
            .into_iter()
            .find(|candidate| candidate.id == collection_id)
            .ok_or_else(|| AppError::NotFound(format!("no collection {collection_id}")))?;

        let folders = self.folders.list_by_collection(collection_id)?;
        let requests = self.requests.list_by_collection(collection_id)?;
        let examples = if include_examples {
            self.load_examples(collection_id)?
        } else {
            BTreeMap::new()
        };

        let docs = self.load_docs(&collection.id, &folders, &requests)?;

        let (document, notes) = to_document(
            ExportInput {
                collection: &collection,
                folders: &folders,
                requests: &requests,
                examples: &examples,
                docs: &docs,
            },
            version,
        );

        Ok(ExportedDocument {
            file_name: document_file_name(&collection.name, version, format),
            text: render(&document, format)?,
            notes,
        })
    }

    /// One read per item (PLAN.md Phase 12).
    ///
    /// The same shape as `load_examples` below, and for the same reason: an
    /// export is one collection, triggered by a person, and these are
    /// microsecond reads against a local file. A bulk
    /// `docs_by_collection` would be two more methods on two more traits to
    /// save a millisecond nobody can perceive — YAGNI (CLAUDE.md section 7).
    /// If a collection ever grows large enough for it to matter, that method
    /// is a small addition, not a redesign.
    fn load_docs(
        &self,
        collection_id: &str,
        folders: &[Folder],
        requests: &[SavedRequest],
    ) -> Result<ExportDocs, AppError> {
        let mut docs = ExportDocs {
            collection: self.collections.docs(collection_id)?,
            ..ExportDocs::default()
        };
        for folder in folders {
            docs.folders
                .insert(folder.id.clone(), self.folders.docs(&folder.id)?);
        }
        for request in requests {
            docs.requests
                .insert(request.id.clone(), self.requests.docs(&request.id)?);
        }
        Ok(docs)
    }

    /// One `get` per example, because the summary a collection listing returns
    /// deliberately leaves the body behind. A manual export of one collection
    /// is the only caller, so the row count is the user's own and small.
    fn load_examples(
        &self,
        collection_id: &str,
    ) -> Result<BTreeMap<String, Vec<Example>>, AppError> {
        let mut by_request: BTreeMap<String, Vec<Example>> = BTreeMap::new();
        for summary in self.examples.list_summaries_by_collection(collection_id)? {
            let example = self.examples.get(&summary.id)?;
            by_request
                .entry(example.request_id.clone())
                .or_default()
                .push(example);
        }
        Ok(by_request)
    }
}
