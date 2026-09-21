// http_client/src-tauri/src/domain/services/openapi_import.rs
//
// "Import an OpenAPI file as a new collection" (PLAN.md Phase 8d).
//
// Two steps, because the dialog shows what will happen before anything is
// written: `load` reads, checks and holds the document; `preview` and
// `import` map it with whatever options the dialog has set. The document
// stays on this side between the two — the webview gets a token, never the
// path or the file's content.
//
// Only one import is pending at a time: the dialog is modal, and loading a
// second file replaces the first.
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::domain::error::AppError;
use crate::domain::ids::new_id;
use crate::domain::import_plan::ImportedIds;
use crate::domain::ports::ImportRepository;
use crate::openapi::document::OpenApiVersion;
use crate::openapi::import::detect::{detect, MAX_FILE_BYTES};
use crate::openapi::import::grouping::Grouping;
use crate::openapi::import::refs::{check_references, Refs};
use crate::openapi::import::to_collection::{
    arrangement_for, default_grouping, to_plan, ImportNote, ImportOptions,
};
use crate::openapi::import::validate::{validate, Coverage};
use crate::openapi::import::{Detected, Refusal, SourceFormat};

struct Pending {
    token: String,
    file_name: String,
    detected: Detected,
    coverage: Coverage,
}

/// What `load` found.
pub enum LoadOutcome {
    Ready(ImportPreview),
    Refused { file_name: String, refusal: Refusal },
}

/// The folders one grouping would create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupingPreview {
    pub grouping: Grouping,
    pub folder_count: usize,
    pub top_level: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewVariable {
    pub name: String,
    pub secret: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentPreview {
    pub name: String,
    pub variables: Vec<PreviewVariable>,
}

/// Everything the dialog shows before the user confirms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPreview {
    pub token: String,
    pub file_name: String,
    pub title: String,
    pub version: OpenApiVersion,
    pub format: SourceFormat,
    pub request_count: usize,
    pub example_count: usize,
    pub default_grouping: Grouping,
    pub groupings: Vec<GroupingPreview>,
    pub environment: Option<EnvironmentPreview>,
    pub notes: Vec<ImportNote>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportResult {
    pub ids: ImportedIds,
    pub collection_name: String,
    pub request_count: usize,
    pub notes: Vec<ImportNote>,
}

/// Cheap to clone (Arc), like every other service here.
#[derive(Clone)]
pub struct OpenApiImport {
    repository: Arc<dyn ImportRepository>,
    pending: Arc<Mutex<Option<Pending>>>,
}

impl OpenApiImport {
    pub fn new(repository: Arc<dyn ImportRepository>) -> Self {
        Self {
            repository,
            pending: Arc::new(Mutex::new(None)),
        }
    }

    /// The options the dialog opens with.
    pub fn default_options(detected: &Detected) -> ImportOptions {
        ImportOptions {
            grouping: default_grouping(Refs::new(&detected.document)),
            include_examples: true,
            create_environment: true,
        }
    }

    /// Reads and checks a file. A file that cannot be imported is an
    /// outcome, not an error: the dialog shows why.
    pub fn load(&self, path: &Path) -> Result<LoadOutcome, AppError> {
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        match read_and_check(path) {
            Ok(Ok((detected, coverage))) => {
                let token = new_id("imp");
                let options = Self::default_options(&detected);
                let pending = Pending {
                    token,
                    file_name,
                    detected,
                    coverage,
                };
                let preview = preview_of(&pending, &options);
                log::info!(
                    "openapi import: loaded {} {} with {} operations",
                    preview.format.label(),
                    preview.version.label(),
                    preview.request_count
                );
                *self.lock() = Some(pending);
                Ok(LoadOutcome::Ready(preview))
            }
            Ok(Err(refusal)) => {
                log::info!("openapi import: refused ({})", refusal_kind(&refusal));
                *self.lock() = None;
                Ok(LoadOutcome::Refused { file_name, refusal })
            }
            Err(error) => Err(error),
        }
    }

    pub fn preview(&self, token: &str, options: &ImportOptions) -> Result<ImportPreview, AppError> {
        let guard = self.lock();
        let pending = pending_for(&guard, token)?;
        Ok(preview_of(pending, options))
    }

    /// Writes the pending document. The token is spent only on success, so a
    /// failed write (the credential store was locked, say) can be retried.
    pub fn import(&self, token: &str, options: &ImportOptions) -> Result<ImportResult, AppError> {
        let mut guard = self.lock();
        let pending = pending_for(&guard, token)?;
        let mapped = to_plan(&pending.detected.document, options);
        let ids = self.repository.import(&mapped.plan)?;
        let result = ImportResult {
            ids,
            collection_name: mapped.plan.collection_name.clone(),
            request_count: mapped.plan.requests.len(),
            notes: with_coverage(mapped.notes, pending.coverage),
        };
        log::info!(
            "openapi import: wrote {} requests, {} examples",
            result.request_count,
            mapped.plan.example_count()
        );
        *guard = None;
        Ok(result)
    }

    /// The dialog closed without importing.
    pub fn discard(&self, token: &str) {
        let mut guard = self.lock();
        if guard.as_ref().is_some_and(|pending| pending.token == token) {
            *guard = None;
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Pending>> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn pending_for<'a>(
    guard: &'a MutexGuard<'_, Option<Pending>>,
    token: &str,
) -> Result<&'a Pending, AppError> {
    guard
        .as_ref()
        .filter(|pending| pending.token == token)
        .ok_or_else(|| {
            AppError::NotFound("that import is no longer open; choose the file again".into())
        })
}

/// `Err` only for a failure that is not about the file's content.
fn read_and_check(path: &Path) -> Result<Result<(Detected, Coverage), Refusal>, AppError> {
    let size = match std::fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            return Ok(Err(Refusal::Unreadable {
                message: error.to_string(),
            }))
        }
    };
    if size > MAX_FILE_BYTES {
        return Ok(Err(Refusal::TooLarge {
            bytes: size,
            limit: MAX_FILE_BYTES,
        }));
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Ok(Err(Refusal::Unreadable {
                message: error.to_string(),
            }))
        }
    };
    check(&bytes)
}

/// Detection, schema validation and reference checks, in that order.
pub fn check(bytes: &[u8]) -> Result<Result<(Detected, Coverage), Refusal>, AppError> {
    let detected = match detect(bytes) {
        Ok(detected) => detected,
        Err(refusal) => return Ok(Err(refusal)),
    };
    let coverage = match validate(&detected.document, detected.version)? {
        Ok(coverage) => coverage,
        Err(refusal) => return Ok(Err(refusal)),
    };
    if let Err(refusal) = check_references(&detected.document) {
        return Ok(Err(refusal));
    }
    Ok(Ok((detected, coverage)))
}

fn preview_of(pending: &Pending, options: &ImportOptions) -> ImportPreview {
    let document = &pending.detected.document;
    let refs = Refs::new(document);
    let mapped = to_plan(document, options);
    let groupings = Grouping::ALL
        .iter()
        .map(|&grouping| {
            let arrangement = arrange_or_reuse(refs, grouping, options, &mapped.plan.folders);
            GroupingPreview {
                grouping,
                folder_count: arrangement.0,
                top_level: arrangement.1,
            }
        })
        .collect();
    ImportPreview {
        token: pending.token.clone(),
        file_name: pending.file_name.clone(),
        title: mapped.plan.collection_name.clone(),
        version: pending.detected.version,
        format: pending.detected.format,
        request_count: mapped.plan.requests.len(),
        example_count: mapped.plan.example_count(),
        default_grouping: default_grouping(refs),
        groupings,
        environment: mapped
            .plan
            .environment
            .as_ref()
            .map(|environment| EnvironmentPreview {
                name: environment.name.clone(),
                variables: environment
                    .variables
                    .iter()
                    .map(|variable| PreviewVariable {
                        name: variable.name.clone(),
                        secret: variable.secret,
                    })
                    .collect(),
            }),
        notes: with_coverage(mapped.notes, pending.coverage),
    }
}

/// The plan already holds the chosen grouping's folders; the others are
/// arranged on their own, which is cheap.
fn arrange_or_reuse(
    refs: Refs<'_>,
    grouping: Grouping,
    options: &ImportOptions,
    planned: &[crate::domain::import_plan::PlannedFolder],
) -> (usize, Vec<String>) {
    if grouping == options.grouping {
        let top = planned
            .iter()
            .filter(|folder| folder.parent.is_none())
            .map(|folder| folder.name.clone())
            .collect();
        return (planned.len(), top);
    }
    let arrangement = arrangement_for(refs, grouping);
    (arrangement.folders.len(), arrangement.top_level_names())
}

fn with_coverage(mut notes: Vec<ImportNote>, coverage: Coverage) -> Vec<ImportNote> {
    if coverage == Coverage::StructureOnly {
        notes.insert(0, ImportNote::SchemaObjectsUnchecked);
    }
    notes
}

/// For the log: which refusal, never what the file said.
fn refusal_kind(refusal: &Refusal) -> &'static str {
    match refusal {
        Refusal::Unreadable { .. } => "unreadable",
        Refusal::TooLarge { .. } => "too large",
        Refusal::NotUtf8 => "not UTF-8",
        Refusal::Empty => "empty",
        Refusal::InvalidJson { .. } => "invalid JSON",
        Refusal::InvalidYaml { .. } => "invalid YAML",
        Refusal::NotJsonOrYaml => "not JSON or YAML",
        Refusal::NotOpenApi => "not OpenAPI",
        Refusal::Swagger2 => "Swagger 2.0",
        Refusal::VersionNotString => "version not a string",
        Refusal::UnsupportedVersion { .. } => "unsupported version",
        Refusal::SchemaViolations { .. } => "schema violations",
        Refusal::ExternalReferences { .. } => "external references",
        Refusal::BrokenReferences { .. } => "broken references",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use super::*;
    use crate::domain::import_plan::ImportPlan;
    use crate::openapi::import::grouping::Grouping;

    #[derive(Default)]
    struct RecordingRepository {
        written: StdMutex<Vec<ImportPlan>>,
        fail: bool,
    }

    impl ImportRepository for RecordingRepository {
        fn import(&self, plan: &ImportPlan) -> Result<ImportedIds, AppError> {
            if self.fail {
                return Err(AppError::SecretStore("locked".into()));
            }
            self.written.lock().expect("lock").push(plan.clone());
            Ok(ImportedIds {
                collection_id: "col_1".into(),
                environment_id: None,
            })
        }
    }

    const SPEC: &str = r#"
openapi: 3.1.0
info: {title: Pets, version: '1'}
servers: [{url: 'https://api.example.com'}]
tags: [{name: pets}]
paths:
  /pets:
    get:
      tags: [pets]
      summary: List pets
      responses:
        '200':
          description: ok
          content:
            application/json:
              example: []
  /health:
    get:
      responses: {'200': {description: ok}}
"#;

    fn write_temp(name: &str, content: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("responderhttp-import-{}", new_id("t")));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join(name);
        std::fs::write(&path, content).expect("write temp file");
        path
    }

    fn service(repository: Arc<RecordingRepository>) -> OpenApiImport {
        OpenApiImport::new(repository)
    }

    fn loaded(service: &OpenApiImport, content: &str) -> ImportPreview {
        match service
            .load(&write_temp("spec.yaml", content.as_bytes()))
            .expect("load")
        {
            LoadOutcome::Ready(preview) => preview,
            LoadOutcome::Refused { refusal, .. } => panic!("refused: {refusal:?}"),
        }
    }

    #[test]
    fn a_valid_file_is_previewed_with_default_options() {
        let service = service(Arc::default());

        let preview = loaded(&service, SPEC);

        assert_eq!(preview.file_name, "spec.yaml");
        assert_eq!(preview.title, "Pets");
        assert_eq!(preview.format, SourceFormat::Yaml);
        assert_eq!(preview.version, OpenApiVersion::V3_1);
        assert_eq!(preview.request_count, 2);
        assert_eq!(preview.example_count, 1);
        assert_eq!(preview.default_grouping, Grouping::Tags);
        let tags = &preview.groupings[0];
        assert_eq!(tags.grouping, Grouping::Tags);
        assert_eq!(tags.top_level, vec!["pets"]);
        let paths = &preview.groupings[1];
        assert_eq!(paths.top_level, vec!["health", "pets"]);
        assert_eq!(preview.groupings[2].folder_count, 0);
        let environment = preview.environment.expect("environment");
        assert_eq!(environment.variables[0].name, "baseUrl");
    }

    #[test]
    fn an_invalid_file_is_an_outcome_and_clears_what_was_pending() {
        let repository = Arc::new(RecordingRepository::default());
        let service = service(repository);
        let first = loaded(&service, SPEC);

        let outcome = service
            .load(&write_temp("bad.json", br#"{"openapi":"3.1.0"}"#))
            .expect("load");

        let LoadOutcome::Refused { file_name, refusal } = outcome else {
            panic!("expected a refusal");
        };
        assert_eq!(file_name, "bad.json");
        assert!(matches!(refusal, Refusal::SchemaViolations { .. }));
        assert!(service
            .preview(&first.token, &OpenApiImport::default_options_for_test())
            .is_err());
    }

    #[test]
    fn a_missing_file_is_unreadable() {
        let service = service(Arc::default());
        let path = std::env::temp_dir().join("responderhttp-import-does-not-exist.yaml");

        let LoadOutcome::Refused { refusal, .. } = service.load(&path).expect("load") else {
            panic!("expected a refusal");
        };
        assert!(matches!(refusal, Refusal::Unreadable { .. }));
    }

    #[test]
    fn external_references_are_refused_after_validation() {
        let spec = SPEC.replace("example: []", "schema: {$ref: './schemas.yaml#/Pets'}");

        let Ok(Err(refusal)) = check(spec.as_bytes()) else {
            panic!("expected a refusal");
        };
        assert!(matches!(refusal, Refusal::ExternalReferences { .. }));
    }

    #[test]
    fn preview_follows_the_options() {
        let service = service(Arc::default());
        let preview = loaded(&service, SPEC);

        let changed = service
            .preview(
                &preview.token,
                &ImportOptions {
                    grouping: Grouping::Flat,
                    include_examples: false,
                    create_environment: false,
                },
            )
            .expect("preview");

        assert_eq!(changed.example_count, 0);
        assert!(changed.environment.is_none());
        assert_eq!(changed.groupings[2].folder_count, 0);
        assert_eq!(changed.default_grouping, Grouping::Tags);
    }

    #[test]
    fn import_writes_the_plan_once_and_spends_the_token() {
        let repository = Arc::new(RecordingRepository::default());
        let service = service(repository.clone());
        let preview = loaded(&service, SPEC);
        let options = OpenApiImport::default_options_for_test();

        let result = service.import(&preview.token, &options).expect("import");

        assert_eq!(result.collection_name, "Pets");
        assert_eq!(result.request_count, 2);
        assert_eq!(repository.written.lock().unwrap().len(), 1);
        assert!(matches!(
            service.import(&preview.token, &options),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn a_failed_write_keeps_the_import_open_for_a_retry() {
        let repository = Arc::new(RecordingRepository {
            fail: true,
            ..RecordingRepository::default()
        });
        let service = service(repository);
        let preview = loaded(&service, SPEC);
        let options = OpenApiImport::default_options_for_test();

        assert!(service.import(&preview.token, &options).is_err());
        assert!(service.preview(&preview.token, &options).is_ok());
    }

    #[test]
    fn discard_only_drops_the_matching_token() {
        let service = service(Arc::default());
        let preview = loaded(&service, SPEC);
        let options = OpenApiImport::default_options_for_test();

        service.discard("imp_other");
        assert!(service.preview(&preview.token, &options).is_ok());

        service.discard(&preview.token);
        assert!(service.preview(&preview.token, &options).is_err());
    }

    #[test]
    fn a_foreign_dialect_is_reported_first() {
        let spec = SPEC.replace(
            "openapi: 3.1.0",
            "openapi: 3.1.0\njsonSchemaDialect: https://json-schema.org/draft/2020-12/schema",
        );
        let service = service(Arc::default());

        let preview = loaded(&service, &spec);

        assert_eq!(
            preview.notes.first(),
            Some(&ImportNote::SchemaObjectsUnchecked)
        );
    }

    impl OpenApiImport {
        fn default_options_for_test() -> ImportOptions {
            ImportOptions {
                grouping: Grouping::Tags,
                include_examples: true,
                create_environment: true,
            }
        }
    }
}
