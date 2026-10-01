// http_client/src-tauri/src/domain/services/proto_schemas.rs
//
// The schemas gRPC requests are made against (PLAN-GRPC.md 16f): gathered
// from `.proto` files on disk or asked of a server by reflection, compiled
// once, and kept in memory by id so each Invoke and Send does not compile
// again.
//
// The cache is filled two ways: by importing or reflecting (the schema then
// lives for the session), and from the library on demand (16g, D3), so a
// saved request's schema id keeps working after a restart. Saving a schema
// puts it in the library.
//
// Reading the files is done here, with std::fs, as the OpenAPI import and
// downloads do, and bounded the same way: only files the compiler asks for,
// only inside the chosen folders, with size and count limits.
use std::collections::{HashMap, VecDeque};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use prost_reflect::{DescriptorPool, MethodDescriptor};

use crate::domain::error::AppError;
use crate::domain::ids::new_id;
use crate::domain::models::{GrpcCallRequest, ProtoSchemaSummary, StoredProtoSchema};
use crate::domain::ports::ProtoSchemaRepository;
use crate::domain::services::grpc_reflection::GrpcReflection;
use crate::proto::catalog::{catalog, find_method, ServiceInfo};
use crate::proto::compile::{compile, import_line, imports_of, load, SourceMap};
use crate::proto::example::example_json;

/// One file larger than this is not a schema anyone wrote by hand.
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
/// All the files of one import together.
const MAX_TOTAL_BYTES: u64 = 32 * 1024 * 1024;
const MAX_FILES: usize = 500;

/// Resolved by protox from its own copies, never looked for on disk.
const WELL_KNOWN_PREFIX: &str = "google/protobuf/";

pub use crate::domain::models::SchemaOrigin;

/// A reflected schema's session id starts with this. Saving one gives it a
/// library id of its own, since the server may say something else tomorrow.
const REFLECTION_ID_PREFIX: &str = "reflection:";

/// A compiled schema, ready to call against.
#[derive(Debug, Clone)]
pub struct LoadedSchema {
    pub id: String,
    /// Its name in the library. None until it is saved.
    pub name: Option<String>,
    pub origin: SchemaOrigin,
    pub pool: DescriptorPool,
    /// The serialized `FileDescriptorSet`: what the library will store.
    pub encoded: Vec<u8>,
    /// The source text of an import, keyed by import name. Empty for a
    /// reflected schema, which comes as descriptors.
    pub sources: SourceMap,
    pub services: Vec<ServiceInfo>,
}

type Cache = Arc<Mutex<HashMap<String, Arc<LoadedSchema>>>>;

#[derive(Clone)]
pub struct ProtoSchemas {
    reflection: GrpcReflection,
    library: Arc<dyn ProtoSchemaRepository>,
    loaded: Cache,
}

impl ProtoSchemas {
    pub fn new(reflection: GrpcReflection, library: Arc<dyn ProtoSchemaRepository>) -> Self {
        Self {
            reflection,
            library,
            loaded: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Asks the server for its schema. The id is the target's own, so
    /// reflecting the same server again replaces what it said before rather
    /// than piling up copies.
    pub fn reflect(&self, target: &GrpcCallRequest) -> Result<Arc<LoadedSchema>, AppError> {
        let reflected = self.reflection.reflect(target)?;
        let pool = load(&reflected.encoded)?;
        let tls = if target.target.tls { "tls" } else { "plain" };
        let id = format!("reflection:{tls}:{}", target.target.authority.trim());
        Ok(self.keep(LoadedSchema {
            services: catalog(&pool),
            id,
            name: None,
            origin: SchemaOrigin::Reflection,
            pool,
            encoded: reflected.encoded,
            sources: SourceMap::default(),
        }))
    }

    /// Gathers `roots` and everything they import from disk, and compiles
    /// them.
    ///
    /// Imports are looked up in `import_paths`, then in each root's own
    /// folder, in that order, as `protoc -I` would. An import that climbs
    /// out of those folders (`../x.proto`) is refused: the user adds the
    /// folder it lives in as an import path instead, which is also what
    /// makes the name stable when the schema is stored. protox already
    /// refuses such an import while parsing; `locate` refuses it again, so
    /// the rule does not depend on protox keeping that behaviour.
    pub fn import_files(
        &self,
        roots: &[PathBuf],
        import_paths: &[PathBuf],
    ) -> Result<Arc<LoadedSchema>, AppError> {
        if roots.is_empty() {
            return Err(AppError::InvalidRequest(
                "choose at least one .proto file".to_string(),
            ));
        }
        let mut folders: Vec<PathBuf> = import_paths.to_vec();
        for root in roots {
            if let Some(parent) = root.parent() {
                if !folders.iter().any(|folder| folder == parent) {
                    folders.push(parent.to_path_buf());
                }
            }
        }
        let root_names = roots
            .iter()
            .map(|root| name_within(root, &folders))
            .collect::<Result<Vec<_>, _>>()?;

        let mut sources = SourceMap::default();
        let mut total_bytes = 0u64;
        // Each name travels with the file that imports it (None for a
        // root), so a missing import is reported where it is written.
        let mut queue: VecDeque<(String, Option<String>)> =
            root_names.iter().map(|name| (name.clone(), None)).collect();
        while let Some((name, importer)) = queue.pop_front() {
            if name.starts_with(WELL_KNOWN_PREFIX) || sources.get(&name).is_some() {
                continue;
            }
            if sources.len() >= MAX_FILES {
                return Err(AppError::InvalidRequest(format!(
                    "the schema imports more than {MAX_FILES} files; stopped"
                )));
            }
            let path = locate(&name, &folders)
                .map_err(|error| at_import(error, importer.as_deref(), &name, &sources))?;
            let text = read_bounded(&path, &mut total_bytes)?;
            let imported = imports_of(&name, &text)?;
            queue.extend(imported.into_iter().map(|dep| (dep, Some(name.clone()))));
            sources.insert(name, text);
        }

        let compiled = compile(&sources, &root_names)?;
        Ok(self.keep(LoadedSchema {
            id: new_id("proto"),
            name: None,
            origin: SchemaOrigin::Import,
            services: catalog(&compiled.pool),
            pool: compiled.pool,
            encoded: compiled.encoded,
            sources,
        }))
    }

    /// From the cache, or else from the library, compiled once and cached.
    pub fn get(&self, id: &str) -> Result<Arc<LoadedSchema>, AppError> {
        if let Some(schema) = lock(&self.loaded).get(id).cloned() {
            return Ok(schema);
        }
        let stored = match self.library.get(id) {
            Ok(stored) => stored,
            Err(AppError::NotFound(_)) => {
                return Err(AppError::NotFound(format!(
                    "schema {id} is neither loaded nor saved; use server reflection or import its .proto files again"
                )))
            }
            Err(error) => return Err(error),
        };
        let pool = load(&stored.encoded)?;
        Ok(self.keep(LoadedSchema {
            services: catalog(&pool),
            id: stored.id,
            name: Some(stored.name),
            origin: stored.origin,
            pool,
            encoded: stored.encoded,
            sources: SourceMap::from(stored.sources),
        }))
    }

    /// The library, without loading any schema.
    pub fn list(&self) -> Result<Vec<ProtoSchemaSummary>, AppError> {
        self.library.list()
    }

    /// Puts a loaded schema in the library under `name`, or renames and
    /// updates it if it is there already. A reflected schema gets a library
    /// id of its own; the returned schema carries the id to use from now on.
    pub fn save(&self, id: &str, name: &str) -> Result<Arc<LoadedSchema>, AppError> {
        let name = checked_name(name)?;
        let loaded = self.get(id)?;
        let library_id = if loaded.id.starts_with(REFLECTION_ID_PREFIX) {
            new_id("proto")
        } else {
            loaded.id.clone()
        };
        self.library.save(&StoredProtoSchema {
            id: library_id.clone(),
            name: name.clone(),
            origin: loaded.origin,
            encoded: loaded.encoded.clone(),
            sources: loaded.sources.to_map(),
            updated_at: String::new(),
        })?;
        Ok(self.keep(LoadedSchema {
            id: library_id,
            name: Some(name),
            ..(*loaded).clone()
        }))
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<(), AppError> {
        let name = checked_name(name)?;
        self.library.rename(id, &name)?;
        let cached = lock(&self.loaded).get(id).cloned();
        if let Some(cached) = cached {
            self.keep(LoadedSchema {
                name: Some(name),
                ..(*cached).clone()
            });
        }
        Ok(())
    }

    /// Refused while a saved request uses the schema, naming the requests,
    /// so none is left pointing at nothing.
    pub fn delete(&self, id: &str) -> Result<(), AppError> {
        let users = self.library.users(id)?;
        if !users.is_empty() {
            return Err(AppError::InvalidRequest(format!(
                "the schema is used by {}; change or delete those requests first",
                users.join(", ")
            )));
        }
        self.library.delete(id)?;
        lock(&self.loaded).remove(id);
        Ok(())
    }

    pub fn method(&self, id: &str, path: &str) -> Result<MethodDescriptor, AppError> {
        Ok(find_method(&self.get(id)?.pool, path)?)
    }

    /// "Use Example Message": an example of the method's input type.
    pub fn example(&self, id: &str, path: &str) -> Result<String, AppError> {
        Ok(example_json(&self.method(id, path)?.input())?)
    }

    fn keep(&self, schema: LoadedSchema) -> Arc<LoadedSchema> {
        let schema = Arc::new(schema);
        lock(&self.loaded).insert(schema.id.clone(), schema.clone());
        schema
    }
}

fn checked_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidRequest(
            "a schema needs a name".to_string(),
        ));
    }
    Ok(name.to_string())
}

fn lock(cache: &Cache) -> MutexGuard<'_, HashMap<String, Arc<LoadedSchema>>> {
    cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A root's import name: its path relative to the first folder that holds
/// it, with forward slashes, as an `import` statement would spell it.
fn name_within(path: &Path, folders: &[PathBuf]) -> Result<String, AppError> {
    folders
        .iter()
        .find_map(|folder| path.strip_prefix(folder).ok())
        .map(import_name)
        .ok_or_else(|| {
            AppError::InvalidRequest(format!(
                "{} is not inside any import folder",
                path.display()
            ))
        })
}

fn import_name(relative: &Path) -> String {
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Leads a refusal to find or accept an import with where the import is
/// written, `file:line:` (or the file alone if the line is not found), as
/// a syntax error is led (found in the manual click-through, 16j). A root's
/// own failure has no importer and is left as it is.
fn at_import(error: AppError, importer: Option<&str>, name: &str, sources: &SourceMap) -> AppError {
    match (error, importer) {
        (AppError::InvalidRequest(message), Some(importer)) => {
            let place = sources
                .get(importer)
                .and_then(|source| import_line(source, name))
                .map_or_else(|| importer.to_string(), |line| format!("{importer}:{line}"));
            AppError::InvalidRequest(format!("{place}: {message}"))
        }
        (error, _) => error,
    }
}

/// Where an imported name lives: the first folder that has it.
fn locate(name: &str, folders: &[PathBuf]) -> Result<PathBuf, AppError> {
    let relative = Path::new(name);
    let escapes = relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)));
    if escapes {
        return Err(AppError::InvalidRequest(format!(
            "the import {name:?} points outside the import folders; add the folder it is in as an import path instead"
        )));
    }
    folders
        .iter()
        .map(|folder| folder.join(relative))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            let searched = folders
                .iter()
                .map(|folder| folder.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            AppError::InvalidRequest(format!(
                "cannot find {name}, which the schema imports, in any import folder ({searched}); add the folder it is in as an import path"
            ))
        })
}

fn read_bounded(path: &Path, total_bytes: &mut u64) -> Result<String, AppError> {
    let unreadable = |error: std::io::Error| {
        AppError::InvalidRequest(format!("could not read {}: {error}", path.display()))
    };
    let size = std::fs::metadata(path).map_err(unreadable)?.len();
    if size > MAX_FILE_BYTES {
        return Err(AppError::InvalidRequest(format!(
            "{} is larger than {} MiB, too large for a .proto file",
            path.display(),
            MAX_FILE_BYTES / (1024 * 1024)
        )));
    }
    *total_bytes += size;
    if *total_bytes > MAX_TOTAL_BYTES {
        return Err(AppError::InvalidRequest(format!(
            "the schema's files add up to more than {} MiB",
            MAX_TOTAL_BYTES / (1024 * 1024)
        )));
    }
    std::fs::read_to_string(path).map_err(unreadable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ports::{GrpcCall, GrpcTransport, ProtoSchemaRepository};
    use crate::proto::catalog::MethodKind;
    use crate::proto::fixture;

    /// Import tests never reach reflection; any open is a mistake.
    struct NoNetwork;

    impl GrpcTransport for NoNetwork {
        fn open(&self, _request: &GrpcCallRequest) -> Result<Box<dyn GrpcCall>, AppError> {
            Err(AppError::Transport("no network in these tests".to_string()))
        }
    }

    /// The library in memory. `users` maps a schema id to the requests
    /// that use it.
    #[derive(Default)]
    struct MemoryLibrary {
        stored: Mutex<HashMap<String, StoredProtoSchema>>,
        users: Mutex<HashMap<String, Vec<String>>>,
    }

    impl ProtoSchemaRepository for MemoryLibrary {
        fn list(&self) -> Result<Vec<ProtoSchemaSummary>, AppError> {
            let mut list: Vec<ProtoSchemaSummary> = self
                .stored
                .lock()
                .expect("stored")
                .values()
                .map(|s| ProtoSchemaSummary {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    origin: s.origin,
                    updated_at: s.updated_at.clone(),
                })
                .collect();
            list.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(list)
        }

        fn get(&self, id: &str) -> Result<StoredProtoSchema, AppError> {
            self.stored
                .lock()
                .expect("stored")
                .get(id)
                .cloned()
                .ok_or_else(|| AppError::NotFound(id.to_string()))
        }

        fn save(&self, schema: &StoredProtoSchema) -> Result<(), AppError> {
            let mut stored = schema.clone();
            stored.updated_at = "2026-09-28T00:00:00Z".to_string();
            self.stored
                .lock()
                .expect("stored")
                .insert(schema.id.clone(), stored);
            Ok(())
        }

        fn rename(&self, id: &str, name: &str) -> Result<(), AppError> {
            let mut stored = self.stored.lock().expect("stored");
            let schema = stored
                .get_mut(id)
                .ok_or_else(|| AppError::NotFound(id.to_string()))?;
            schema.name = name.to_string();
            Ok(())
        }

        fn delete(&self, id: &str) -> Result<(), AppError> {
            self.stored
                .lock()
                .expect("stored")
                .remove(id)
                .map(|_| ())
                .ok_or_else(|| AppError::NotFound(id.to_string()))
        }

        fn users(&self, id: &str) -> Result<Vec<String>, AppError> {
            Ok(self
                .users
                .lock()
                .expect("users")
                .get(id)
                .cloned()
                .unwrap_or_default())
        }
    }

    fn schemas() -> ProtoSchemas {
        schemas_on(Arc::new(MemoryLibrary::default()))
    }

    fn schemas_on(library: Arc<MemoryLibrary>) -> ProtoSchemas {
        ProtoSchemas::new(GrpcReflection::new(Arc::new(NoNetwork)), library)
    }

    fn imported(schemas: &ProtoSchemas, dir: &TempDir) -> Arc<LoadedSchema> {
        let root = dir.write("shop.proto", &shop_importing("money.proto"));
        dir.write("money.proto", fixture::MONEY);
        schemas.import_files(&[root], &[]).expect("imports")
    }

    #[test]
    fn a_saved_schema_loads_again_in_a_new_session_from_the_library_alone() {
        let library = Arc::new(MemoryLibrary::default());
        let dir = TempDir::new();
        let first = schemas_on(library.clone());
        let schema = imported(&first, &dir);

        let saved = first.save(&schema.id, "  Shop API ").expect("saved");
        drop(dir);
        let second = schemas_on(library.clone());
        let loaded = second.get(&saved.id).expect("from the library");

        assert_eq!(saved.id, schema.id, "an import keeps its id");
        assert_eq!(loaded.name.as_deref(), Some("Shop API"));
        assert_eq!(loaded.services[0].methods.len(), 4);
        assert!(loaded.sources.get("money.proto").is_some());
        assert_eq!(second.list().expect("list")[0].name, "Shop API");
    }

    #[test]
    fn an_unsaved_unknown_id_is_not_found() {
        assert!(matches!(
            schemas().get("proto_nope"),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn a_schema_needs_a_name() {
        let dir = TempDir::new();
        let schemas = schemas();
        let schema = imported(&schemas, &dir);

        assert!(matches!(
            schemas.save(&schema.id, "  "),
            Err(AppError::InvalidRequest(_))
        ));
    }

    #[test]
    fn renaming_updates_the_library_and_the_loaded_copy() {
        let dir = TempDir::new();
        let schemas = schemas();
        let schema = imported(&schemas, &dir);
        schemas.save(&schema.id, "Old").expect("saved");

        schemas.rename(&schema.id, "New").expect("renamed");

        assert_eq!(
            schemas.get(&schema.id).expect("loaded").name.as_deref(),
            Some("New")
        );
        assert_eq!(schemas.list().expect("list")[0].name, "New");
    }

    #[test]
    fn a_schema_in_use_is_not_deleted_and_the_users_are_named() {
        let library = Arc::new(MemoryLibrary::default());
        let dir = TempDir::new();
        let schemas = schemas_on(library.clone());
        let schema = imported(&schemas, &dir);
        schemas.save(&schema.id, "Shop").expect("saved");
        library.users.lock().expect("users").insert(
            schema.id.clone(),
            vec!["Get item".to_string(), "Watch".to_string()],
        );

        let error = schemas.delete(&schema.id).expect_err("in use");

        assert!(error.to_string().contains("Get item, Watch"), "{error}");
        assert!(schemas.get(&schema.id).is_ok());
    }

    #[test]
    fn deleting_an_unused_schema_removes_it_everywhere() {
        let dir = TempDir::new();
        let schemas = schemas();
        let schema = imported(&schemas, &dir);
        schemas.save(&schema.id, "Shop").expect("saved");

        schemas.delete(&schema.id).expect("deleted");

        assert!(schemas.list().expect("list").is_empty());
        assert!(matches!(
            schemas.get(&schema.id),
            Err(AppError::NotFound(_))
        ));
    }

    /// A fresh folder under the system temp directory, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(new_id("responderhttp-proto-test"));
            std::fs::create_dir_all(&dir).expect("temp dir");
            Self(dir)
        }

        fn write(&self, relative: &str, text: &str) -> PathBuf {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("parent dir");
            }
            std::fs::write(&path, text).expect("write");
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The fixture's shop.proto, importing money.proto from beside it.
    fn shop_importing(money_import: &str) -> String {
        fixture::SHOP.replace("common/money.proto", money_import)
    }

    #[test]
    fn a_file_importing_a_neighbour_is_imported_whole() {
        let dir = TempDir::new();
        let root = dir.write("shop.proto", &shop_importing("money.proto"));
        dir.write("money.proto", fixture::MONEY);
        let schemas = schemas();

        let schema = schemas.import_files(&[root], &[]).expect("imports");

        assert_eq!(schema.origin, SchemaOrigin::Import);
        assert_eq!(schema.services[0].name, "demo.v1.Shop");
        assert_eq!(schema.sources.len(), 2);
        assert!(schema.sources.get("money.proto").is_some());
        // Kept for Invoke, found by id.
        let method = schemas
            .method(&schema.id, "/demo.v1.Shop/Chat")
            .expect("found");
        assert_eq!(MethodKind::of(&method), MethodKind::Bidirectional);
    }

    #[test]
    fn an_import_path_finds_what_the_roots_folder_does_not_have() {
        let roots = TempDir::new();
        let shared = TempDir::new();
        let root = roots.write("shop/shop.proto", fixture::SHOP);
        shared.write("common/money.proto", fixture::MONEY);

        let without = schemas().import_files(std::slice::from_ref(&root), &[]);
        let with = schemas().import_files(&[root], std::slice::from_ref(&shared.0));

        let error = without.expect_err("common/money.proto is elsewhere");
        assert!(error.to_string().contains("common/money.proto"), "{error}");
        // Led by the importing file and the line of its import statement.
        let line =
            import_line(fixture::SHOP, "common/money.proto").expect("the fixture imports it");
        assert!(
            error
                .to_string()
                .starts_with(&format!("shop.proto:{line}: cannot find")),
            "{error}"
        );
        assert!(with
            .expect("imports")
            .sources
            .get("common/money.proto")
            .is_some());
    }

    #[test]
    fn an_import_climbing_out_of_the_folders_is_refused() {
        let dir = TempDir::new();
        let root = dir.write("api/shop.proto", &shop_importing("../money.proto"));
        dir.write("money.proto", fixture::MONEY);

        let error = schemas()
            .import_files(&[root], &[])
            .expect_err("../ is refused");

        // protox refuses a `..` import itself, while parsing, before
        // `locate` sees it. Its wording ("invalid group name") is its own;
        // what matters is the refusal and that it points at the import.
        assert!(matches!(error, AppError::InvalidRequest(_)));
        assert!(error.to_string().starts_with("shop.proto:6:"), "{error}");
    }

    #[test]
    fn well_known_imports_are_never_looked_for_on_disk() {
        let dir = TempDir::new();
        let root = dir.write("shop.proto", &shop_importing("money.proto"));
        dir.write("money.proto", fixture::MONEY);

        let schema = schemas().import_files(&[root], &[]).expect("imports");

        assert!(schema
            .sources
            .names()
            .all(|name| !name.starts_with(WELL_KNOWN_PREFIX)));
        assert!(schema
            .pool
            .get_message_by_name("google.protobuf.Timestamp")
            .is_some());
    }

    #[test]
    fn a_syntax_error_on_disk_names_the_file_and_line() {
        let dir = TempDir::new();
        let root = dir.write(
            "broken.proto",
            "syntax = \"proto3\";\nmessage Broken { string a = 1 }\n",
        );

        let error = schemas()
            .import_files(&[root], &[])
            .expect_err("syntax error");

        assert!(error.to_string().starts_with("broken.proto:2:"), "{error}");
    }

    #[test]
    fn choosing_nothing_is_refused() {
        assert!(matches!(
            schemas().import_files(&[], &[]),
            Err(AppError::InvalidRequest(_))
        ));
    }

    #[test]
    fn an_unknown_schema_id_is_not_found_and_says_what_to_do() {
        let error = schemas()
            .method("proto_nope", "/demo.v1.Shop/Get")
            .expect_err("not loaded");

        assert!(matches!(error, AppError::NotFound(_)));
        assert!(error.to_string().contains("reflection"), "{error}");
    }

    #[test]
    fn an_example_is_of_the_methods_input_type() {
        let dir = TempDir::new();
        let root = dir.write("shop.proto", &shop_importing("money.proto"));
        dir.write("money.proto", fixture::MONEY);
        let schemas = schemas();
        let schema = schemas.import_files(&[root], &[]).expect("imports");

        let example: serde_json::Value = serde_json::from_str(
            &schemas
                .example(&schema.id, "/demo.v1.Shop/Get")
                .expect("example"),
        )
        .expect("json");

        assert_eq!(example["id"], "id");
        assert_eq!(example["big"], "0");
    }

    #[test]
    fn a_failed_reflection_is_reported_and_nothing_is_kept() {
        let schemas = schemas();
        let target = GrpcCallRequest {
            target: crate::domain::models::GrpcTarget {
                authority: "127.0.0.1:1".to_string(),
                tls: false,
            },
            path: String::new(),
            metadata: Vec::new(),
            auth: crate::domain::models::Auth::None,
            settings: crate::domain::models::GrpcSettings::default(),
        };

        assert!(matches!(
            schemas.reflect(&target),
            Err(AppError::Transport(_))
        ));
        assert!(schemas.get("reflection:plain:127.0.0.1:1").is_err());
    }
}
