// http_client/src-tauri/src/proto/compile.rs
//
// `.proto` sources to a descriptor pool, in memory. protox is a protobuf
// compiler in Rust, so the user never needs `protoc` (CLAUDE.md section 11,
// rule 1 in spirit: nothing is shelled out).
//
// The compiler reads **only** from the `SourceMap` it is given, plus the
// well-known `google/protobuf/*` files protox carries. It never touches the
// disk, the same containment the OpenAPI import gives its schema loader.
// Reading the user's files, within the folders they chose, is 16f's job, and
// happens once, at import.
use std::collections::BTreeMap;
use std::path::Path;

use miette::Diagnostic as _;
use prost_reflect::DescriptorPool;
use protox::file::{ChainFileResolver, File, FileResolver, GoogleFileResolver};
use protox::Compiler;

use super::ProtoError;

/// Source files keyed by the name an `import` statement uses for them,
/// always with forward slashes: `common/money.proto`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceMap {
    files: BTreeMap<String, String>,
}

impl SourceMap {
    pub fn insert(&mut self, name: impl Into<String>, source: impl Into<String>) {
        self.files.insert(name.into(), source.into());
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.files.get(name).map(String::as_str)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The files by name, as the schema library stores them.
    pub fn to_map(&self) -> BTreeMap<String, String> {
        self.files.clone()
    }
}

impl From<BTreeMap<String, String>> for SourceMap {
    fn from(files: BTreeMap<String, String>) -> Self {
        Self { files }
    }
}

/// A compiled schema: the pool to work with, and the encoded
/// `FileDescriptorSet` (imports included) to store. The encoded form is what
/// the schema library keeps, so a saved schema never needs its sources, or
/// the disk, to load again.
#[derive(Debug, Clone)]
pub struct CompiledSchema {
    pub pool: DescriptorPool,
    pub encoded: Vec<u8>,
}

/// Resolves imports from the map and nowhere else.
struct MapResolver(SourceMap);

impl FileResolver for MapResolver {
    /// protox asks for a name for each root path it is given. Answering
    /// here, rather than leaving it to the default, keeps the lookup inside
    /// the map even if the working directory happens to contain a file of
    /// the same relative path.
    fn resolve_path(&self, path: &Path) -> Option<String> {
        let name = path.to_str()?.replace('\\', "/");
        self.0.get(&name).map(|_| name)
    }

    fn open_file(&self, name: &str) -> Result<File, protox::Error> {
        match self.0.get(name) {
            Some(source) => File::from_source(name, source),
            None => Err(protox::Error::file_not_found(name)),
        }
    }
}

/// Compiles `roots` and everything they import.
///
/// The well-known types resolve first, so a user file named like one of
/// them cannot replace it: `google.protobuf.Timestamp` always means the real
/// one, which the JSON mapping treats specially.
pub fn compile(sources: &SourceMap, roots: &[String]) -> Result<CompiledSchema, ProtoError> {
    if roots.is_empty() {
        return Err(ProtoError::Schema("no .proto file was chosen".to_string()));
    }
    let mut resolver = ChainFileResolver::new();
    resolver.add(GoogleFileResolver::new());
    resolver.add(MapResolver(sources.clone()));

    let mut compiler = Compiler::with_file_resolver(resolver);
    compiler.include_imports(true).include_source_info(false);
    compiler
        .open_files(roots)
        .map_err(|error| schema_error(&error, sources))?;

    Ok(CompiledSchema {
        pool: compiler.descriptor_pool(),
        encoded: compiler.encode_file_descriptor_set(),
    })
}

/// The names a file imports, read without compiling it, so the files of an
/// import can be gathered from disk one import at a time (16f). A syntax
/// error here is reported like one from `compile`, with its position.
pub fn imports_of(name: &str, source: &str) -> Result<Vec<String>, ProtoError> {
    match File::from_source(name, source) {
        Ok(file) => Ok(file.file_descriptor_proto().dependency.clone()),
        Err(error) => {
            let mut sources = SourceMap::default();
            sources.insert(name, source);
            Err(schema_error(&error, &sources))
        }
    }
}

/// Loads a schema the library stored earlier.
pub fn load(encoded: &[u8]) -> Result<DescriptorPool, ProtoError> {
    DescriptorPool::decode(encoded).map_err(|error| {
        ProtoError::Schema(format!("the stored schema could not be read: {error}"))
    })
}

/// protox's own message, led by where it happened: `file:line:column`
/// when protox points at a place in a file the user supplied, the file
/// alone when it only names one.
///
/// protox reports the place as a miette label, a byte offset into that
/// file. The offset is turned into a line and a column here, against the
/// same source text the compiler read. A well-known Google file is not in
/// the map, so an error inside one gets its name only, which is right: the
/// user cannot edit it.
fn schema_error(error: &protox::Error, sources: &SourceMap) -> ProtoError {
    let message = error.to_string();
    let Some(file) = error.file() else {
        return ProtoError::Schema(message);
    };
    let position = error
        .labels()
        .and_then(|mut labels| labels.next())
        .zip(sources.get(file))
        .map(|(label, source)| line_and_column(source, label.offset()));
    let place = match position {
        Some((line, column)) => format!("{file}:{line}:{column}"),
        None => file.to_string(),
    };
    ProtoError::Schema(format!("{place}: {message}"))
}

/// 1-based line and column of a byte offset. The column counts characters,
/// not bytes, so it matches what an editor shows on a line with non-ASCII
/// text in a comment or string. An offset past the end, or inside a
/// multi-byte character, is clamped to the nearest character boundary
/// before it.
/// The line (from 1) of the `import` statement for `name` in a file's
/// source, so an import that cannot be found is reported where it is
/// written. None if no line imports it, which a caller treats as "name the
/// file only".
pub fn import_line(source: &str, name: &str) -> Option<usize> {
    let quoted = [format!("\"{name}\""), format!("'{name}'")];
    source
        .lines()
        .position(|line| {
            line.trim_start().starts_with("import")
                && quoted
                    .iter()
                    .any(|spelling| line.contains(spelling.as_str()))
        })
        .map(|index| index + 1)
}

fn line_and_column(source: &str, offset: usize) -> (usize, usize) {
    let mut end = offset.min(source.len());
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    let before = &source[..end];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
    let column = before[line_start..].chars().count() + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;

    #[test]
    fn an_import_is_found_on_its_own_line_in_either_quote_style() {
        let source = "syntax = \"proto3\";\n\nimport \"hello/messages.proto\";\n  import public 'common/money.proto';\n";

        assert_eq!(import_line(source, "hello/messages.proto"), Some(3));
        assert_eq!(import_line(source, "common/money.proto"), Some(4));
        assert_eq!(import_line(source, "hello/other.proto"), None);
    }

    #[test]
    fn a_name_that_only_appears_in_a_comment_is_not_an_import() {
        let source = "// see \"hello/messages.proto\"\nimport \"hello/messages.proto\";\n";

        assert_eq!(import_line(source, "hello/messages.proto"), Some(2));
    }

    #[test]
    fn a_schema_with_an_import_and_a_well_known_type_compiles() {
        let schema = fixture::shop();

        assert!(schema.pool.get_message_by_name("demo.v1.Item").is_some());
        assert!(schema.pool.get_message_by_name("common.Money").is_some());
        assert!(schema
            .pool
            .get_message_by_name("google.protobuf.Timestamp")
            .is_some());
    }

    #[test]
    fn the_encoded_set_loads_back_into_the_same_schema() {
        let schema = fixture::shop();

        let pool = load(&schema.encoded).expect("loads");

        assert!(pool.get_service_by_name("demo.v1.Shop").is_some());
        assert!(pool.get_message_by_name("common.Money").is_some());
    }

    #[test]
    fn imports_are_read_without_compiling() {
        let imports = imports_of("shop/shop.proto", fixture::SHOP).expect("parses");

        assert_eq!(
            imports,
            ["google/protobuf/timestamp.proto", "common/money.proto"]
        );
        assert_eq!(
            imports_of("common/money.proto", fixture::MONEY).expect("parses"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn reading_imports_reports_a_syntax_error_with_its_line() {
        let error = imports_of(
            "broken.proto",
            "syntax = \"proto3\";\nmessage Broken { string a = 1 }\n",
        )
        .expect_err("syntax");

        assert!(error.to_string().starts_with("broken.proto:2:"), "{error}");
    }

    #[test]
    fn a_missing_import_is_named() {
        let mut sources = SourceMap::default();
        sources.insert("shop/shop.proto", fixture::SHOP);

        let error = compile(&sources, &["shop/shop.proto".to_string()])
            .expect_err("common/money.proto is missing");

        assert!(error.to_string().contains("common/money.proto"), "{error}");
    }

    #[test]
    fn a_syntax_error_names_the_file_and_the_line() {
        let mut sources = SourceMap::default();
        sources.insert(
            "broken.proto",
            "syntax = \"proto3\";\nmessage Broken { string a = 1 }\n",
        );

        let error = compile(&sources, &["broken.proto".to_string()]).expect_err("syntax error");

        assert!(matches!(error, ProtoError::Schema(_)));
        // The missing `;` is on line 2. The column is protox's to choose
        // (the `}` or just before it), so only the line is pinned.
        assert!(error.to_string().starts_with("broken.proto:2:"), "{error}");
    }

    #[test]
    fn an_unknown_type_points_at_its_use() {
        let mut sources = SourceMap::default();
        sources.insert(
            "types.proto",
            "syntax = \"proto3\";\n\nmessage A {\n  Missing b = 1;\n}\n",
        );

        let error = compile(&sources, &["types.proto".to_string()]).expect_err("unknown type");

        assert!(error.to_string().starts_with("types.proto:4:"), "{error}");
    }

    #[test]
    fn offsets_become_one_based_lines_and_character_columns() {
        let source = "ab\ncd\n// é\nx";

        assert_eq!(line_and_column(source, 0), (1, 1));
        assert_eq!(line_and_column(source, 1), (1, 2));
        assert_eq!(line_and_column(source, 3), (2, 1));
        // "// é" is 5 bytes and 4 characters; `x` starts line 4.
        assert_eq!(line_and_column(source, source.len() - 1), (4, 1));
        // Inside the two bytes of `é`: clamped back to its start, column 4.
        let inside = source.find('é').expect("present") + 1;
        assert_eq!(line_and_column(source, inside), (3, 4));
        // Past the end: the end.
        assert_eq!(line_and_column(source, 999), (4, 2));
    }

    #[test]
    fn a_root_that_is_not_in_the_map_is_refused() {
        let error = compile(&fixture::sources(), &["elsewhere.proto".to_string()])
            .expect_err("not in the map");

        assert!(matches!(error, ProtoError::Schema(_)));
    }

    #[test]
    fn nothing_chosen_is_an_error_not_an_empty_schema() {
        assert!(compile(&fixture::sources(), &[]).is_err());
    }

    #[test]
    fn well_known_types_cannot_be_shadowed_by_a_user_file() {
        let mut sources = fixture::sources();
        sources.insert(
            "google/protobuf/timestamp.proto",
            "syntax = \"proto3\";\npackage google.protobuf;\nmessage Timestamp { string fake = 1; }\n",
        );

        let schema = compile(&sources, &["shop/shop.proto".to_string()]).expect("compiles");
        let timestamp = schema
            .pool
            .get_message_by_name("google.protobuf.Timestamp")
            .expect("present");

        assert!(timestamp.get_field_by_name("seconds").is_some());
        assert!(timestamp.get_field_by_name("fake").is_none());
    }
}
