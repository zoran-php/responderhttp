// http_client/src-tauri/src/domain/services/collections.rs
use std::sync::Arc;

use crate::domain::error::AppError;
use crate::domain::ids::new_id;
use crate::domain::models::{
    Collection, Example, ExampleSummary, Folder, HttpRequest, KeyValue, NewExample, SavedRequest,
    SavedWebSocket, WebSocketRequest, WsDraft,
};
use crate::domain::ports::{
    CollectionRepository, ExampleRepository, FolderRepository, SavedRequestRepository,
    WebSocketRepository,
};
use crate::domain::secrets::{request_without_literal_secrets, SecretState};
use crate::domain::services::validation::validated_name;

/// The largest response body an example may keep. Anything bigger is refused
/// rather than truncated: a saved example that silently differs from what the
/// server sent is worse than no example at all. One megabyte covers every API
/// response worth keeping as documentation, and keeps the app-data database
/// from growing without a bound.
pub const MAX_EXAMPLE_BODY_BYTES: usize = 1024 * 1024;

/// Everything inside one collection, as the sidebar needs it. Examples arrive
/// as summaries — the tree shows all of them and their bodies would dwarf
/// everything else here.
pub struct CollectionContents {
    pub folders: Vec<Folder>,
    pub requests: Vec<SavedRequest>,
    pub examples: Vec<ExampleSummary>,
    /// Beside `requests` rather than mixed into it: they are separate types
    /// (see `SavedWebSocket`), and the tree interleaves them by name.
    pub web_sockets: Vec<SavedWebSocket>,
}

/// Use-cases over the storage aggregates of a collection. Cheap to clone
/// (Arc), so the command layer can move a clone into a blocking task.
#[derive(Clone)]
pub struct Collections {
    collections: Arc<dyn CollectionRepository>,
    folders: Arc<dyn FolderRepository>,
    requests: Arc<dyn SavedRequestRepository>,
    examples: Arc<dyn ExampleRepository>,
    web_sockets: Arc<dyn WebSocketRepository>,
}

impl Collections {
    pub fn new(
        collections: Arc<dyn CollectionRepository>,
        folders: Arc<dyn FolderRepository>,
        requests: Arc<dyn SavedRequestRepository>,
        examples: Arc<dyn ExampleRepository>,
        web_sockets: Arc<dyn WebSocketRepository>,
    ) -> Self {
        Self {
            collections,
            folders,
            requests,
            examples,
            web_sockets,
        }
    }

    pub fn list(&self) -> Result<Vec<Collection>, AppError> {
        self.collections.list()
    }

    pub fn contents(&self, collection_id: &str) -> Result<CollectionContents, AppError> {
        Ok(CollectionContents {
            folders: self.folders.list_by_collection(collection_id)?,
            requests: self.requests.list_by_collection(collection_id)?,
            examples: self.examples.list_summaries_by_collection(collection_id)?,
            web_sockets: self.web_sockets.list_by_collection(collection_id)?,
        })
    }

    pub fn create_collection(&self, name: &str) -> Result<Collection, AppError> {
        self.collections.create(validated_name(name)?)
    }

    pub fn rename_collection(&self, id: &str, name: &str) -> Result<(), AppError> {
        self.collections.rename(id, validated_name(name)?)
    }

    pub fn delete_collection(&self, id: &str) -> Result<(), AppError> {
        self.collections.delete(id)
    }

    pub fn create_folder(
        &self,
        collection_id: &str,
        parent_folder_id: Option<&str>,
        name: &str,
    ) -> Result<Folder, AppError> {
        self.folders
            .create(collection_id, parent_folder_id, validated_name(name)?)
    }

    pub fn rename_folder(&self, id: &str, name: &str) -> Result<(), AppError> {
        self.folders.rename(id, validated_name(name)?)
    }

    pub fn delete_folder(&self, id: &str) -> Result<(), AppError> {
        self.folders.delete(id)
    }

    /// An id of None saves a new request; an existing id overwrites it, which
    /// is what "Save" on an already-saved request means.
    pub fn save_request(
        &self,
        id: Option<String>,
        collection_id: &str,
        folder_id: Option<&str>,
        name: &str,
        request: HttpRequest,
    ) -> Result<SavedRequest, AppError> {
        let saved = SavedRequest {
            id: id.unwrap_or_else(|| new_id("req")),
            collection_id: collection_id.to_string(),
            folder_id: folder_id.map(str::to_string),
            name: validated_name(name)?.to_string(),
            request,
            secret_state: SecretState::Ok,
        };
        self.requests.save(&saved)?;
        Ok(saved)
    }

    pub fn load_request(&self, id: &str) -> Result<SavedRequest, AppError> {
        self.requests.get(id)
    }

    pub fn rename_request(&self, id: &str, name: &str) -> Result<(), AppError> {
        self.requests.rename(id, validated_name(name)?)
    }

    pub fn move_request(&self, id: &str, folder_id: Option<&str>) -> Result<(), AppError> {
        self.requests.move_to(id, folder_id)
    }

    pub fn delete_request(&self, id: &str) -> Result<(), AppError> {
        self.requests.delete(id)
    }

    /// The WebSocket counterpart of `save_request`: an id of None saves a new
    /// one, an existing id overwrites it. Ids share the `req` prefix because
    /// the rows share a table, and rename, move, delete and docs reach a
    /// WebSocket through the request commands by that id.
    ///
    /// No URL check here, as there is none on the HTTP side: a half-typed
    /// draft is worth saving. The scheme is checked when connecting.
    pub fn save_web_socket(
        &self,
        id: Option<String>,
        collection_id: &str,
        folder_id: Option<&str>,
        name: &str,
        request: WebSocketRequest,
        draft: WsDraft,
    ) -> Result<SavedWebSocket, AppError> {
        let saved = SavedWebSocket {
            id: id.unwrap_or_else(|| new_id("req")),
            collection_id: collection_id.to_string(),
            folder_id: folder_id.map(str::to_string),
            name: validated_name(name)?.to_string(),
            request,
            draft,
        };
        self.web_sockets.save(&saved)?;
        Ok(saved)
    }

    pub fn load_web_socket(&self, id: &str) -> Result<SavedWebSocket, AppError> {
        self.web_sockets.get(id)
    }

    /// An example always hangs off a stored request, so the caller must save
    /// the request first — there is no place to put one otherwise.
    pub fn save_example(
        &self,
        request_id: &str,
        name: &str,
        request: HttpRequest,
        status: u16,
        response_headers: Vec<KeyValue>,
        response_body: String,
    ) -> Result<Example, AppError> {
        let name = validated_name(name)?.to_string();
        if response_body.len() > MAX_EXAMPLE_BODY_BYTES {
            return Err(AppError::InvalidRequest(format!(
                "this response is {} bytes and an example may keep at most {}",
                response_body.len(),
                MAX_EXAMPLE_BODY_BYTES
            )));
        }
        self.examples.create(&NewExample {
            request_id: request_id.to_string(),
            name,
            // Examples keep no secrets (PLAN.md Phase 9). The frontend already
            // leaves secret variables as placeholders in the snapshot; a
            // literal auth secret is blanked here.
            request: request_without_literal_secrets(&request),
            status,
            response_headers,
            response_body,
        })
    }

    pub fn load_example(&self, id: &str) -> Result<Example, AppError> {
        self.examples.get(id)
    }

    pub fn rename_example(&self, id: &str, name: &str) -> Result<(), AppError> {
        self.examples.rename(id, validated_name(name)?)
    }

    pub fn delete_example(&self, id: &str) -> Result<(), AppError> {
        self.examples.delete(id)
    }
}

/// One rule for every name in the tree, so an empty or whitespace-only name
/// cannot reach storage from any path.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{
        Auth, HttpMethod, RequestBody, RequestSettings, WebSocketSettings, WsMessageFormat,
    };
    use std::sync::Mutex;

    #[derive(Default)]
    struct SpyCollections {
        created: Mutex<Vec<String>>,
    }

    impl CollectionRepository for SpyCollections {
        fn list(&self) -> Result<Vec<Collection>, AppError> {
            Ok(Vec::new())
        }
        fn create(&self, name: &str) -> Result<Collection, AppError> {
            self.created
                .lock()
                .expect("spy lock poisoned")
                .push(name.to_string());
            Ok(Collection {
                id: "col_1".into(),
                name: name.to_string(),
            })
        }
        fn rename(&self, _id: &str, _name: &str) -> Result<(), AppError> {
            Ok(())
        }
        fn delete(&self, _id: &str) -> Result<(), AppError> {
            Ok(())
        }
        fn docs(&self, _id: &str) -> Result<String, AppError> {
            Ok(String::new())
        }
        fn set_docs(&self, _id: &str, _markdown: &str) -> Result<(), AppError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct StubFolders;

    impl FolderRepository for StubFolders {
        fn list_by_collection(&self, _collection_id: &str) -> Result<Vec<Folder>, AppError> {
            Ok(Vec::new())
        }
        fn create(
            &self,
            _collection_id: &str,
            _parent_folder_id: Option<&str>,
            name: &str,
        ) -> Result<Folder, AppError> {
            Ok(Folder {
                id: "fld_1".into(),
                collection_id: "col_1".into(),
                parent_folder_id: None,
                name: name.to_string(),
            })
        }
        fn rename(&self, _id: &str, _name: &str) -> Result<(), AppError> {
            Ok(())
        }
        fn delete(&self, _id: &str) -> Result<(), AppError> {
            Ok(())
        }
        fn docs(&self, _id: &str) -> Result<String, AppError> {
            Ok(String::new())
        }
        fn set_docs(&self, _id: &str, _markdown: &str) -> Result<(), AppError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct SpyRequests {
        saved: Mutex<Vec<SavedRequest>>,
    }

    impl SavedRequestRepository for SpyRequests {
        fn list_by_collection(&self, _collection_id: &str) -> Result<Vec<SavedRequest>, AppError> {
            Ok(Vec::new())
        }
        fn get(&self, id: &str) -> Result<SavedRequest, AppError> {
            Err(AppError::NotFound(format!("request {id} does not exist")))
        }
        fn save(&self, saved: &SavedRequest) -> Result<(), AppError> {
            self.saved
                .lock()
                .expect("spy lock poisoned")
                .push(saved.clone());
            Ok(())
        }
        fn rename(&self, _id: &str, _name: &str) -> Result<(), AppError> {
            Ok(())
        }
        fn move_to(&self, _id: &str, _folder_id: Option<&str>) -> Result<(), AppError> {
            Ok(())
        }
        fn delete(&self, _id: &str) -> Result<(), AppError> {
            Ok(())
        }
        fn docs(&self, _id: &str) -> Result<String, AppError> {
            Ok(String::new())
        }
        fn set_docs(&self, _id: &str, _markdown: &str) -> Result<(), AppError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct SpyExamples {
        created: Mutex<Vec<NewExample>>,
    }

    impl ExampleRepository for SpyExamples {
        fn list_summaries_by_collection(
            &self,
            _collection_id: &str,
        ) -> Result<Vec<ExampleSummary>, AppError> {
            Ok(Vec::new())
        }
        fn get(&self, id: &str) -> Result<Example, AppError> {
            Err(AppError::NotFound(format!("example {id} does not exist")))
        }
        fn create(&self, example: &NewExample) -> Result<Example, AppError> {
            self.created
                .lock()
                .expect("spy lock poisoned")
                .push(example.clone());
            Ok(Example {
                id: "exa_1".into(),
                request_id: example.request_id.clone(),
                name: example.name.clone(),
                created_at: "2026-09-14T00:00:00Z".into(),
                request: example.request.clone(),
                status: example.status,
                response_headers: example.response_headers.clone(),
                response_body: example.response_body.clone(),
            })
        }
        fn rename(&self, _id: &str, _name: &str) -> Result<(), AppError> {
            Ok(())
        }
        fn delete(&self, _id: &str) -> Result<(), AppError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct SpyWebSockets {
        saved: Mutex<Vec<SavedWebSocket>>,
    }

    impl WebSocketRepository for SpyWebSockets {
        fn list_by_collection(
            &self,
            _collection_id: &str,
        ) -> Result<Vec<SavedWebSocket>, AppError> {
            Ok(self.saved.lock().expect("spy lock poisoned").clone())
        }
        fn get(&self, id: &str) -> Result<SavedWebSocket, AppError> {
            Err(AppError::NotFound(format!(
                "WebSocket request {id} does not exist"
            )))
        }
        fn save(&self, saved: &SavedWebSocket) -> Result<(), AppError> {
            self.saved
                .lock()
                .expect("spy lock poisoned")
                .push(saved.clone());
            Ok(())
        }
    }

    fn service(collections: Arc<SpyCollections>, requests: Arc<SpyRequests>) -> Collections {
        Collections::new(
            collections,
            Arc::new(StubFolders),
            requests,
            Arc::new(SpyExamples::default()),
            Arc::new(SpyWebSockets::default()),
        )
    }

    fn example_service(examples: Arc<SpyExamples>) -> Collections {
        Collections::new(
            Arc::new(SpyCollections::default()),
            Arc::new(StubFolders),
            Arc::new(SpyRequests::default()),
            examples,
            Arc::new(SpyWebSockets::default()),
        )
    }

    fn web_socket_service(web_sockets: Arc<SpyWebSockets>) -> Collections {
        Collections::new(
            Arc::new(SpyCollections::default()),
            Arc::new(StubFolders),
            Arc::new(SpyRequests::default()),
            Arc::new(SpyExamples::default()),
            web_sockets,
        )
    }

    fn web_socket_request() -> WebSocketRequest {
        WebSocketRequest {
            url: "wss://echo.websocket.org".into(),
            headers: vec![KeyValue::new("Sec-WebSocket-Protocol", "chat")],
            settings: WebSocketSettings::default(),
        }
    }

    fn draft() -> WsDraft {
        WsDraft {
            format: WsMessageFormat::Json,
            text: "{\"hello\":1}".into(),
            ..WsDraft::default()
        }
    }

    fn request() -> HttpRequest {
        HttpRequest {
            method: HttpMethod::Get,
            url: "https://example.com/".into(),
            headers: Vec::new(),
            query_params: Vec::new(),
            body: RequestBody::None,
            auth: Auth::None,
            settings: RequestSettings::default(),
        }
    }

    #[test]
    fn trims_a_name_before_storing_it() {
        let collections = Arc::new(SpyCollections::default());
        let service = service(collections.clone(), Arc::new(SpyRequests::default()));

        service
            .create_collection("  Work APIs  ")
            .expect("should create");

        assert_eq!(
            collections.created.lock().expect("spy lock poisoned")[0],
            "Work APIs"
        );
    }

    #[test]
    fn rejects_a_blank_name_before_it_reaches_storage() {
        let collections = Arc::new(SpyCollections::default());
        let service = service(collections.clone(), Arc::new(SpyRequests::default()));

        let error = service.create_collection("   ").expect_err("should reject");

        assert!(matches!(error, AppError::InvalidRequest(_)));
        assert!(collections
            .created
            .lock()
            .expect("spy lock poisoned")
            .is_empty());
    }

    #[test]
    fn mints_an_id_for_a_new_request_and_keeps_it_on_re_save() {
        let requests = Arc::new(SpyRequests::default());
        let service = service(Arc::new(SpyCollections::default()), requests.clone());

        let first = service
            .save_request(None, "col_1", None, "List users", request())
            .expect("should save");
        service
            .save_request(
                Some(first.id.clone()),
                "col_1",
                None,
                "List users",
                request(),
            )
            .expect("should re-save");

        let saved = requests.saved.lock().expect("spy lock poisoned");
        assert!(saved[0].id.starts_with("req_"));
        assert_eq!(saved[0].id, saved[1].id);
    }

    #[test]
    fn an_example_name_goes_through_the_same_validation_as_every_other_name() {
        let examples = Arc::new(SpyExamples::default());
        let service = example_service(examples.clone());

        let error = service
            .save_example("req_1", "  ", request(), 200, Vec::new(), "{}".into())
            .expect_err("should reject");

        assert!(matches!(error, AppError::InvalidRequest(_)));
        assert!(examples
            .created
            .lock()
            .expect("spy lock poisoned")
            .is_empty());
    }

    /// Refused, not truncated: an example that silently differs from what the
    /// server sent would be worse than not having one.
    #[test]
    fn a_body_past_the_cap_is_refused_before_it_reaches_storage() {
        let examples = Arc::new(SpyExamples::default());
        let service = example_service(examples.clone());

        let error = service
            .save_example(
                "req_1",
                "Too big",
                request(),
                200,
                Vec::new(),
                "x".repeat(MAX_EXAMPLE_BODY_BYTES + 1),
            )
            .expect_err("should reject");

        assert!(matches!(error, AppError::InvalidRequest(_)));
        assert!(examples
            .created
            .lock()
            .expect("spy lock poisoned")
            .is_empty());
    }

    #[test]
    fn a_web_socket_gets_a_request_id_keeps_it_on_re_save_and_has_its_name_trimmed() {
        let web_sockets = Arc::new(SpyWebSockets::default());
        let service = web_socket_service(web_sockets.clone());

        let first = service
            .save_web_socket(
                None,
                "col_1",
                Some("fld_1"),
                "  Echo  ",
                web_socket_request(),
                draft(),
            )
            .expect("should save");
        service
            .save_web_socket(
                Some(first.id.clone()),
                "col_1",
                Some("fld_1"),
                "Echo",
                web_socket_request(),
                draft(),
            )
            .expect("should re-save");

        let saved = web_sockets.saved.lock().expect("spy lock poisoned");
        assert!(saved[0].id.starts_with("req_"));
        assert_eq!(saved[0].id, saved[1].id);
        assert_eq!(saved[0].name, "Echo");
        assert_eq!(saved[0].folder_id.as_deref(), Some("fld_1"));
        assert_eq!(saved[0].draft, draft());
    }

    #[test]
    fn a_web_socket_name_goes_through_the_same_validation_as_every_other_name() {
        let web_sockets = Arc::new(SpyWebSockets::default());
        let service = web_socket_service(web_sockets.clone());

        let error = service
            .save_web_socket(None, "col_1", None, "   ", web_socket_request(), draft())
            .expect_err("should reject");

        assert!(matches!(error, AppError::InvalidRequest(_)));
        assert!(web_sockets
            .saved
            .lock()
            .expect("spy lock poisoned")
            .is_empty());
    }

    /// Mixed collections are the point of the feature: the contents call
    /// carries both kinds, side by side.
    #[test]
    fn contents_carry_web_sockets_beside_http_requests() {
        let web_sockets = Arc::new(SpyWebSockets::default());
        let service = web_socket_service(web_sockets);
        service
            .save_web_socket(None, "col_1", None, "Echo", web_socket_request(), draft())
            .expect("should save");

        let contents = service.contents("col_1").expect("should list");

        assert_eq!(contents.web_sockets.len(), 1);
        assert_eq!(contents.web_sockets[0].name, "Echo");
    }

    #[test]
    fn a_body_exactly_at_the_cap_is_accepted() {
        let examples = Arc::new(SpyExamples::default());
        let service = example_service(examples.clone());

        service
            .save_example(
                "req_1",
                "Just fits",
                request(),
                200,
                Vec::new(),
                "x".repeat(MAX_EXAMPLE_BODY_BYTES),
            )
            .expect("should save");

        assert_eq!(examples.created.lock().expect("spy lock poisoned").len(), 1);
    }
}
