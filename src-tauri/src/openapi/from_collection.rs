// http_client/src-tauri/src/openapi/from_collection.rs
//
// Collection -> OpenAPI document. Pure: every input is passed in, nothing is
// read from storage or from the active environment here.
//
// That purity is a security property, not just a testing one. Resolving
// `{{api_key}}` against the live environment on the way out would write a real
// credential into a file that gets committed to git — so this function is not
// given the means to do it (see PLAN.md Phase 8, "what must never leave").
use std::collections::{BTreeMap, BTreeSet};

use crate::domain::models::{
    Auth, Collection, Example as SavedExample, Folder, HttpMethod, RequestBody, SavedRequest,
};
use crate::domain::secrets::{is_credential_param_name, is_placeholder_only};
use crate::openapi::document::{
    Components, Document, Example, ExampleBody, Info, MediaType, OpenApiVersion, Operation,
    Parameter, PathItem, Response, Schema, SchemaType, SecurityScheme, Server, ServerVariable, Tag,
};
use crate::openapi::format::ExportFormat;
use crate::openapi::infer::Observed;
use crate::openapi::url::map_url;

/// Headers that describe credentials or session state. They become security
/// schemes or nothing at all — never a `parameters` entry carrying the value.
const CREDENTIAL_HEADERS: [&str; 3] = ["authorization", "cookie", "proxy-authorization"];

/// What the export could not represent faithfully. Surfaced in the UI rather
/// than swallowed: an exporter that quietly drops half a collection is worse
/// than one that says what it dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportNote {
    /// Two saved requests share a path and method; OpenAPI has room for one.
    DuplicateOperation {
        method: String,
        path: String,
        kept: String,
        dropped: String,
    },
    /// A URL this app would not have sent anyway.
    UnmappableUrl { request: String, url: String },
    /// An auth credential was described as a scheme, with the value left out.
    CredentialOmitted { request: String, scheme: String },
    /// A header was dropped because it carries credentials or session state.
    CredentialHeaderOmitted { request: String, header: String },
    /// Nested folders flatten: OpenAPI tags have no hierarchy we rely on.
    FolderNestingFlattened { folder: String },
    /// A query parameter whose name marks it as a credential (`token`,
    /// `api_key`, …) was described without its value.
    CredentialParameterValueOmitted { request: String, parameter: String },
    /// Before 3.2 there is no `serializedValue`, so saved bodies were decoded
    /// into `value` instead. Reported because it is the one way a 3.1 or 3.0
    /// export differs in content from the 3.2 one.
    ExampleBodyDecoded,
}

/// Every item's Markdown documentation, keyed by id (PLAN.md Phase 12).
///
/// Passed alongside the items rather than carried on `Collection`, `Folder`
/// and `SavedRequest` themselves. Documentation is an attribute of the item,
/// not part of it: putting it on the models would mean every save carried it,
/// and a save that carries documentation is a save that can wipe it.
#[derive(Debug, Default)]
pub struct ExportDocs {
    pub collection: String,
    pub folders: BTreeMap<String, String>,
    pub requests: BTreeMap<String, String>,
}

impl ExportDocs {
    fn folder(&self, id: &str) -> Option<String> {
        self.folders.get(id).and_then(|docs| written(docs))
    }

    fn request(&self, id: &str) -> Option<String> {
        self.requests.get(id).and_then(|docs| written(docs))
    }
}

/// What `info.description` says when the collection has no documentation of
/// its own.
///
/// Public, and paired with `is_generated_description`, because the import
/// side has to be able to recognise it: without that, exporting an
/// undocumented collection and importing it back would leave this sentence
/// sitting in the user's collection documentation as though they had written
/// it.
pub fn generated_description(collection_name: &str) -> String {
    format!("Exported from the ResponderHTTP collection \"{collection_name}\".")
}

/// Whether a description is one this app generated rather than one a person
/// wrote. Matched on the stem rather than the whole sentence, so a collection
/// renamed between the export and the import is still recognised.
pub fn is_generated_description(description: &str) -> bool {
    description
        .trim()
        .starts_with("Exported from the ResponderHTTP collection ")
}

/// Documentation, if there is any, **exactly as the user wrote it**.
///
/// Deliberately not `non_empty`, which trims. Markdown is whitespace
/// sensitive — four leading spaces are a code block, two trailing ones are a
/// line break — so the test for "is there anything here" must not double as
/// an edit of what is there.
fn written(docs: &str) -> Option<String> {
    (!docs.trim().is_empty()).then(|| docs.to_string())
}

pub struct ExportInput<'a> {
    pub collection: &'a Collection,
    pub folders: &'a [Folder],
    pub requests: &'a [SavedRequest],
    /// Full saved responses per request id. Empty unless the user opted in —
    /// response bodies can carry tokens, so inclusion is a deliberate act.
    pub examples: &'a BTreeMap<String, Vec<SavedExample>>,
    pub docs: &'a ExportDocs,
}

pub fn to_document(input: ExportInput<'_>, version: OpenApiVersion) -> (Document, Vec<ExportNote>) {
    let mut notes = Vec::new();
    let mut paths: BTreeMap<String, PathItem> = BTreeMap::new();
    let mut servers: BTreeMap<String, Server> = BTreeMap::new();
    let mut schemes: BTreeMap<String, SecurityScheme> = BTreeMap::new();
    let mut used_operation_ids: BTreeSet<String> = BTreeSet::new();

    let folder_names: BTreeMap<&str, &str> = input
        .folders
        .iter()
        .map(|folder| (folder.id.as_str(), folder.name.as_str()))
        .collect();
    for folder in input.folders {
        if folder.parent_folder_id.is_some() {
            notes.push(ExportNote::FolderNestingFlattened {
                folder: folder.name.clone(),
            });
        }
    }

    for saved in input.requests {
        let Some(mapped) = map_url(&saved.request.url) else {
            notes.push(ExportNote::UnmappableUrl {
                request: saved.name.clone(),
                url: saved.request.url.clone(),
            });
            continue;
        };

        let server = servers
            .entry(mapped.server.clone())
            .or_insert_with(|| Server {
                url: mapped.server.clone(),
                description: None,
                name: None,
                variables: BTreeMap::new(),
            });
        for variable in &mapped.server_variables {
            server
                .variables
                .entry(variable.clone())
                .or_insert_with(|| ServerVariable {
                    // A placeholder, never the environment's value. The spec
                    // requires a default; it does not require a real one.
                    default: format!("<{variable}>"),
                    description: Some(format!(
                        "Supplied by the {variable} environment variable in ResponderHTTP."
                    )),
                });
        }

        let security = security_for(&saved.request.auth, &mut schemes);
        if let Some(scheme) = security.as_ref().and_then(|s| s.first()) {
            if let Some(name) = scheme.keys().next() {
                notes.push(ExportNote::CredentialOmitted {
                    request: saved.name.clone(),
                    scheme: name.clone(),
                });
            }
        }

        let mut parameters = Parameters::default();
        for name in &mapped.path_parameters {
            parameters.push(Parameter {
                name: name.clone(),
                location: "path".into(),
                // The spec allows nothing else for a path parameter.
                required: Some(true),
                schema: Some(string_schema()),
                example: None,
            });
        }
        // The URL's own query first, then the Params tab: the order libcurl
        // puts them on the wire. A name in both is one parameter, described
        // by whichever comes first.
        let url_query = mapped
            .query_parameters
            .iter()
            .map(|pair| (pair.name.as_str(), pair.value.as_str()));
        let params_tab = saved
            .request
            .query_params
            .iter()
            .map(|pair| (pair.name.as_str(), pair.value.as_str()));
        for (name, value) in url_query.chain(params_tab) {
            let name = name.trim();
            if name.is_empty() || is_auth_query_param(&saved.request.auth, name) {
                continue;
            }
            let example = if is_credential_param_name(name) && !is_placeholder_only(value) {
                if !value.trim().is_empty() && !parameters.has("query", name) {
                    notes.push(ExportNote::CredentialParameterValueOmitted {
                        request: saved.name.clone(),
                        parameter: name.to_string(),
                    });
                }
                None
            } else {
                non_empty(value)
            };
            parameters.push(Parameter {
                name: name.to_string(),
                location: "query".into(),
                required: None,
                schema: Some(string_schema()),
                example,
            });
        }
        for pair in &saved.request.headers {
            let name = pair.name.trim();
            if name.is_empty() {
                continue;
            }
            if CREDENTIAL_HEADERS.contains(&name.to_ascii_lowercase().as_str())
                || is_auth_header(&saved.request.auth, name)
            {
                notes.push(ExportNote::CredentialHeaderOmitted {
                    request: saved.name.clone(),
                    header: name.to_string(),
                });
                continue;
            }
            parameters.push(Parameter {
                name: name.to_string(),
                location: "header".into(),
                required: None,
                schema: Some(string_schema()),
                example: non_empty(&pair.value),
            });
        }

        let operation = Operation {
            tags: saved
                .folder_id
                .as_deref()
                .and_then(|id| folder_names.get(id))
                .map(|name| vec![(*name).to_string()])
                .unwrap_or_default(),
            summary: non_empty(&saved.name),
            description: input.docs.request(&saved.id),
            operation_id: Some(unique_operation_id(&saved.name, &mut used_operation_ids)),
            parameters: parameters.into_vec(),
            request_body: request_body_for(&saved.request.body, version),
            responses: responses_for(input.examples.get(&saved.id), version),
            security,
        };

        let item = paths.entry(mapped.path.clone()).or_default();
        if let Some(existing) = slot(item, saved.request.method) {
            notes.push(ExportNote::DuplicateOperation {
                method: saved.request.method.as_str().to_string(),
                path: mapped.path.clone(),
                kept: existing.summary.clone().unwrap_or_default(),
                dropped: saved.name.clone(),
            });
            continue;
        }
        *slot_mut(item, saved.request.method) = Some(operation);
    }

    let tags = input
        .folders
        .iter()
        .map(|folder| Tag {
            name: folder.name.clone(),
            description: input.docs.folder(&folder.id),
        })
        .collect();

    // Conditioned on saved responses rather than on the version alone: those
    // are the bodies the user opted in to and the ones whose shape changed. A
    // note that fires on every pre-3.2 export regardless of content is noise.
    if !version.has_serialized_value() && !input.examples.is_empty() {
        notes.push(ExportNote::ExampleBodyDecoded);
    }

    let document = Document {
        openapi: version.openapi_field().to_string(),
        info: Info {
            title: input.collection.name.clone(),
            // The API's version, not the app's. A collection records no such
            // thing, so this is a starting point for the user to edit.
            version: "1.0.0".into(),
            summary: None,
            // The collection's own documentation when it has any. The
            // generated provenance line is a placeholder for an empty
            // description, not something to prepend to real prose the user
            // wrote — an export is a document someone else will read.
            description: written(&input.docs.collection)
                .or_else(|| Some(generated_description(&input.collection.name))),
        },
        servers: servers.into_values().collect(),
        paths,
        tags,
        components: (!schemes.is_empty()).then_some(Components {
            security_schemes: schemes,
        }),
    };
    (document, notes)
}

/// An operation's parameters, unique by name and location as the spec
/// requires. The first one pushed wins; later ones with the same identity are
/// the same parameter seen again (`?id=1&id=2`, or a name in both the URL and
/// the Params tab), not new information. Header names compare
/// case-insensitively, as HTTP does; everything else exactly.
#[derive(Default)]
struct Parameters(Vec<Parameter>);

impl Parameters {
    fn has(&self, location: &str, name: &str) -> bool {
        self.0.iter().any(|existing| {
            existing.location == location
                && if location == "header" {
                    existing.name.eq_ignore_ascii_case(name)
                } else {
                    existing.name == name
                }
        })
    }

    fn push(&mut self, parameter: Parameter) {
        if !self.has(&parameter.location, &parameter.name) {
            self.0.push(parameter);
        }
    }

    fn into_vec(self) -> Vec<Parameter> {
        self.0
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Describes the auth scheme and registers it in components. The credential
/// itself never reaches the document.
fn security_for(
    auth: &Auth,
    schemes: &mut BTreeMap<String, SecurityScheme>,
) -> Option<Vec<BTreeMap<String, Vec<String>>>> {
    let (name, scheme) = match auth {
        Auth::None => return None,
        Auth::Basic { .. } => (
            "basicAuth".to_string(),
            SecurityScheme {
                kind: "http".into(),
                description: None,
                scheme: Some("basic".into()),
                name: None,
                location: None,
            },
        ),
        Auth::Bearer { .. } => (
            "bearerAuth".to_string(),
            SecurityScheme {
                kind: "http".into(),
                description: None,
                scheme: Some("bearer".into()),
                name: None,
                location: None,
            },
        ),
        Auth::ApiKey { key, location, .. } => {
            let where_ = match location {
                crate::domain::models::ApiKeyLocation::Header => "header",
                crate::domain::models::ApiKeyLocation::Query => "query",
            };
            (
                format!("apiKey_{where_}_{}", slug(key)),
                SecurityScheme {
                    kind: "apiKey".into(),
                    description: None,
                    scheme: None,
                    name: Some(key.trim().to_string()),
                    location: Some(where_.into()),
                },
            )
        }
        // No OpenAPI type describes "an arbitrary header the user chose", so
        // apiKey-in-header is the closest honest fit rather than an exact one.
        Auth::Custom { header_name, .. } => (
            format!("custom_{}", slug(header_name)),
            SecurityScheme {
                kind: "apiKey".into(),
                description: Some(
                    "Exported from a custom auth header; verify the scheme by hand.".into(),
                ),
                scheme: None,
                name: Some(header_name.trim().to_string()),
                location: Some("header".into()),
            },
        ),
    };
    schemes.entry(name.clone()).or_insert(scheme);
    Some(vec![BTreeMap::from([(name, Vec::new())])])
}

fn is_auth_query_param(auth: &Auth, name: &str) -> bool {
    matches!(
        auth,
        Auth::ApiKey { key, location: crate::domain::models::ApiKeyLocation::Query, .. }
            if key.trim().eq_ignore_ascii_case(name.trim())
    )
}

fn is_auth_header(auth: &Auth, name: &str) -> bool {
    match auth {
        Auth::ApiKey {
            key,
            location: crate::domain::models::ApiKeyLocation::Header,
            ..
        } => key.trim().eq_ignore_ascii_case(name),
        Auth::Custom { header_name, .. } => header_name.trim().eq_ignore_ascii_case(name),
        _ => false,
    }
}

fn request_body_for(
    body: &RequestBody,
    version: OpenApiVersion,
) -> Option<crate::openapi::document::RequestBody> {
    let (content_type, example) = match body {
        RequestBody::None => return None,
        RequestBody::Raw { content_type, text } => (non_empty(content_type)?, non_empty(text)),
        RequestBody::FormUrlEncoded(fields) if !fields.is_empty() => {
            ("application/x-www-form-urlencoded".to_string(), None)
        }
        RequestBody::Multipart(parts) if !parts.is_empty() => {
            ("multipart/form-data".to_string(), None)
        }
        _ => return None,
    };
    // Parsed once and used twice: the schema is inferred from the same value
    // the example carries, so a document can never describe a shape its own
    // example contradicts.
    let decoded = example.as_deref().map(|text| decode(&content_type, text));
    let schema = decoded.as_ref().and_then(|value| {
        let mut observed = Observed::default();
        observed.observe(value);
        observed.to_schema(version)
    });
    let examples = match (example, decoded) {
        (Some(text), Some(value)) => BTreeMap::from([(
            "saved".to_string(),
            Example::new(
                Some("As saved in ResponderHTTP".into()),
                Some(example_body(version, &text, value)),
            ),
        )]),
        _ => BTreeMap::new(),
    };
    let mut content = BTreeMap::new();
    content.insert(content_type, MediaType { schema, examples });
    Some(crate::openapi::document::RequestBody {
        description: None,
        content,
    })
}

/// Saved responses become the `responses` map. Without them every operation
/// gets the `default` placeholder below, which is why the export dialog says
/// what including examples buys.
fn responses_for(
    examples: Option<&Vec<SavedExample>>,
    version: OpenApiVersion,
) -> BTreeMap<String, Response> {
    let Some(examples) = examples.filter(|list| !list.is_empty()) else {
        return BTreeMap::from([(
            "default".to_string(),
            Response {
                description: "No saved response. Send the request and save one to describe it."
                    .into(),
                content: BTreeMap::new(),
            },
        )]);
    };

    // Observations are accumulated per (status, media type) and turned into a
    // schema only once every saved response has been folded in. That is what
    // makes `required` an intersection: a key is required only if every
    // response for this status carried it.
    let mut observations: BTreeMap<(String, String), Observed> = BTreeMap::new();
    let mut responses: BTreeMap<String, Response> = BTreeMap::new();

    for example in examples {
        let content_type = response_content_type(example);
        let status = example.status.to_string();

        let decoded = non_empty(&example.response_body).map(|body| decode(&content_type, &body));
        if let Some(value) = &decoded {
            observations
                .entry((status.clone(), content_type.clone()))
                .or_default()
                .observe(value);
        }

        let response = responses.entry(status).or_insert_with(|| Response {
            description: example.name.clone(),
            content: BTreeMap::new(),
        });
        response
            .content
            .entry(content_type)
            .or_default()
            .examples
            .insert(
                slug(&example.name),
                Example::new(
                    non_empty(&example.name),
                    decoded.map(|value| example_body(version, &example.response_body, value)),
                ),
            );
    }

    for ((status, content_type), observed) in observations {
        if let Some(media) = responses
            .get_mut(&status)
            .and_then(|response| response.content.get_mut(&content_type))
        {
            media.schema = observed.to_schema(version);
        }
    }
    responses
}

/// Every parameter is a string on the wire, whatever the server parses it
/// into. Inference does not apply here: a query row holds the text the user
/// typed, not a JSON value, so guessing `integer` from "42" would describe the
/// user's habits rather than the API.
fn string_schema() -> Schema {
    Schema {
        kind: Some(SchemaType::One("string".into())),
        ..Schema::default()
    }
}

/// A response's media type, minus any `; charset=` parameter. `text/plain`
/// when the server did not say — the same assumption HTTP itself makes.
fn response_content_type(example: &SavedExample) -> String {
    example
        .response_headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-type"))
        .and_then(|header| header.value.split(';').next())
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| "text/plain".to_string())
}

/// Which field a body goes in, and in what form.
///
/// 3.2 keeps the bytes as they were received. 3.1 and 3.0 have no
/// `serializedValue`, so the body goes in `value` — which both specs define as
/// the *decoded* representation, not the encoded one: leaving it as a string
/// would emit an example claiming the endpoint returns a quoted blob of JSON
/// text.
///
/// Takes the decoded value rather than decoding again, so the example and the
/// inferred schema are guaranteed to come from one parse of one body.
fn example_body(version: OpenApiVersion, raw: &str, decoded: serde_json::Value) -> ExampleBody {
    if version.has_serialized_value() {
        ExampleBody::Serialized(raw.to_string())
    } else {
        ExampleBody::Decoded(decoded)
    }
}

/// A body that claims to be JSON and parses becomes that JSON. Everything else
/// stays a string — which is not a fallback but the right answer: the decoded
/// form of `text/plain`, `text/html` or `text/csv` *is* a string. A body that
/// claims JSON and does not parse also stays a string, because reproducing
/// what the server actually sent beats discarding it.
fn decode(media_type: &str, raw: &str) -> serde_json::Value {
    if is_json(media_type) {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw) {
            return parsed;
        }
    }
    serde_json::Value::String(raw.to_string())
}

/// `application/json` and the `+json` structured suffix (RFC 6839), which is
/// how `application/problem+json` and friends declare themselves.
fn is_json(media_type: &str) -> bool {
    let lowered = media_type.trim().to_ascii_lowercase();
    lowered == "application/json" || lowered.ends_with("+json")
}

/// operationId must be unique across the document, and tooling uses it to name
/// generated client methods, so it is a slug rather than the raw name.
fn unique_operation_id(name: &str, used: &mut BTreeSet<String>) -> String {
    let base = {
        let slugged = slug(name);
        if slugged.is_empty() {
            "operation".to_string()
        } else {
            slugged
        }
    };
    let mut candidate = base.clone();
    let mut suffix = 2;
    while !used.insert(candidate.clone()) {
        candidate = format!("{base}_{suffix}");
        suffix += 1;
    }
    candidate
}

fn slug(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut last_was_separator = false;
    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
            last_was_separator = false;
        } else if !out.is_empty() && !last_was_separator {
            out.push('_');
            last_was_separator = true;
        }
    }
    out.trim_end_matches('_').to_string()
}

fn slot(item: &PathItem, method: HttpMethod) -> Option<&Operation> {
    match method {
        HttpMethod::Get => item.get.as_ref(),
        HttpMethod::Post => item.post.as_ref(),
        HttpMethod::Put => item.put.as_ref(),
        HttpMethod::Patch => item.patch.as_ref(),
        HttpMethod::Delete => item.delete.as_ref(),
        HttpMethod::Head => item.head.as_ref(),
        HttpMethod::Options => item.options.as_ref(),
    }
}

fn slot_mut(item: &mut PathItem, method: HttpMethod) -> &mut Option<Operation> {
    match method {
        HttpMethod::Get => &mut item.get,
        HttpMethod::Post => &mut item.post,
        HttpMethod::Put => &mut item.put,
        HttpMethod::Patch => &mut item.patch,
        HttpMethod::Delete => &mut item.delete,
        HttpMethod::Head => &mut item.head,
        HttpMethod::Options => &mut item.options,
    }
}

/// What the Save As dialog opens with. Shares `slug` with operationId so an
/// exported file and the ids inside it are spelled the same way.
///
/// The version is in the name so that exporting the same collection twice, at
/// two versions, does not offer to overwrite the first file.
/// The version is in the name so exporting one collection at two versions
/// does not offer to overwrite the first file; the extension follows the
/// format so the Save As dialog and the file agree.
pub fn document_file_name(
    collection_name: &str,
    version: OpenApiVersion,
    format: ExportFormat,
) -> String {
    let base = slug(collection_name);
    let name = if base.is_empty() {
        "collection"
    } else {
        base.as_str()
    };
    format!("{name}.openapi-{}.{}", version.label(), format.extension())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{ApiKeyLocation, Auth, HttpRequest, KeyValue, RequestSettings};

    fn request(name: &str, method: HttpMethod, url: &str) -> SavedRequest {
        SavedRequest {
            id: format!("req_{name}"),
            collection_id: "col_1".into(),
            folder_id: None,
            name: name.into(),
            request: HttpRequest {
                method,
                url: url.into(),
                headers: Vec::new(),
                query_params: Vec::new(),
                body: RequestBody::None,
                auth: Auth::None,
                settings: RequestSettings::default(),
            },
            secret_state: crate::domain::secrets::SecretState::Ok,
        }
    }

    fn collection() -> Collection {
        Collection {
            id: "col_1".into(),
            name: "Work API".into(),
        }
    }

    fn folder(id: &str, name: &str) -> Folder {
        Folder {
            id: id.into(),
            collection_id: "col_1".into(),
            parent_folder_id: None,
            name: name.into(),
        }
    }

    fn export(requests: &[SavedRequest], folders: &[Folder]) -> (Document, Vec<ExportNote>) {
        export_as(requests, folders, &BTreeMap::new(), OpenApiVersion::V3_2)
    }

    fn export_as(
        requests: &[SavedRequest],
        folders: &[Folder],
        examples: &BTreeMap<String, Vec<SavedExample>>,
        version: OpenApiVersion,
    ) -> (Document, Vec<ExportNote>) {
        export_documented(requests, folders, examples, version, &ExportDocs::default())
    }

    fn export_documented(
        requests: &[SavedRequest],
        folders: &[Folder],
        examples: &BTreeMap<String, Vec<SavedExample>>,
        version: OpenApiVersion,
        docs: &ExportDocs,
    ) -> (Document, Vec<ExportNote>) {
        let col = collection();
        to_document(
            ExportInput {
                collection: &col,
                folders,
                requests,
                examples,
                docs,
            },
            version,
        )
    }

    /// All five saved-response tests read the same corner of the document.
    fn saved_response_example<'a>(doc: &'a Document, media_type: &str) -> &'a Example {
        let responses = &doc.paths["/users"]
            .get
            .as_ref()
            .expect("a GET on /users")
            .responses;
        &responses["200"].content[media_type].examples["200_ok"]
    }

    fn saved_example(request_id: &str, content_type: &str, body: &str) -> SavedExample {
        SavedExample {
            id: format!("ex_{request_id}"),
            request_id: request_id.into(),
            name: "200 OK".into(),
            created_at: "2026-09-15T00:00:00Z".into(),
            request: request("List", HttpMethod::Get, "https://api.x.test/users").request,
            status: 200,
            response_headers: vec![KeyValue::new("Content-Type", content_type)],
            response_body: body.into(),
        }
    }

    #[test]
    fn a_collection_becomes_a_document_titled_after_it() {
        let (doc, notes) = export(
            &[request(
                "List users",
                HttpMethod::Get,
                "https://api.x.test/users",
            )],
            &[],
        );

        assert_eq!(doc.openapi, "3.2.0");
        assert_eq!(doc.info.title, "Work API");
        assert_eq!(doc.servers.len(), 1);
        assert_eq!(doc.servers[0].url, "https://api.x.test");
        assert!(doc.paths.contains_key("/users"));
        assert!(notes.is_empty(), "{notes:?}");
    }

    #[test]
    fn a_leading_variable_becomes_a_server_variable_with_a_placeholder_default() {
        let (doc, _) = export(
            &[request("List", HttpMethod::Get, "{{base_url}}/users")],
            &[],
        );

        let server = &doc.servers[0];
        assert_eq!(server.url, "{base_url}");
        // Never the live value from the active environment.
        assert_eq!(server.variables["base_url"].default, "<base_url>");
    }

    #[test]
    fn methods_on_one_path_share_a_path_item() {
        let (doc, notes) = export(
            &[
                request("List", HttpMethod::Get, "https://api.x.test/users"),
                request("Create", HttpMethod::Post, "https://api.x.test/users"),
            ],
            &[],
        );

        let item = &doc.paths["/users"];
        assert!(item.get.is_some() && item.post.is_some());
        assert!(notes.is_empty());
    }

    /// OpenAPI has one slot per method per path; two saved requests competing
    /// for it is a real collision the user needs told about.
    #[test]
    fn a_duplicate_method_and_path_is_reported_rather_than_silently_dropped() {
        let (doc, notes) = export(
            &[
                request("List users", HttpMethod::Get, "https://api.x.test/users"),
                request(
                    "List users again",
                    HttpMethod::Get,
                    "https://api.x.test/users",
                ),
            ],
            &[],
        );

        assert_eq!(
            doc.paths["/users"].get.as_ref().unwrap().summary.as_deref(),
            Some("List users")
        );
        assert!(
            matches!(
                notes.as_slice(),
                [ExportNote::DuplicateOperation { dropped, .. }] if dropped == "List users again"
            ),
            "{notes:?}"
        );
    }

    #[test]
    fn a_bearer_token_becomes_a_scheme_and_the_token_never_appears() {
        let mut saved = request("Me", HttpMethod::Get, "https://api.x.test/me");
        saved.request.auth = Auth::Bearer {
            token: "sk_live_SECRET".into(),
        };

        let (doc, notes) = export(&[saved], &[]);

        let components = doc.components.as_ref().expect("components");
        assert_eq!(
            components.security_schemes["bearerAuth"].scheme.as_deref(),
            Some("bearer")
        );
        let rendered = format!("{doc:?}");
        assert!(
            !rendered.contains("sk_live_SECRET"),
            "credential leaked into the document"
        );
        assert!(notes
            .iter()
            .any(|n| matches!(n, ExportNote::CredentialOmitted { .. })));
    }

    /// ApiKeyLocation::Query puts the credential in the URL, so the parameter
    /// has to be suppressed as well as described.
    #[test]
    fn an_api_key_query_parameter_is_described_not_echoed() {
        let mut saved = request("Search", HttpMethod::Get, "https://api.x.test/search");
        saved.request.auth = Auth::ApiKey {
            key: "api_key".into(),
            value: "SECRET".into(),
            location: ApiKeyLocation::Query,
        };
        saved.request.query_params = vec![
            KeyValue::new("api_key", "SECRET"),
            KeyValue::new("q", "hello"),
        ];

        let (doc, _) = export(&[saved], &[]);

        let op = doc.paths["/search"].get.as_ref().unwrap();
        let names: Vec<&str> = op.parameters.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["q"], "the key parameter must not be echoed");
        assert!(!format!("{doc:?}").contains("SECRET"));
    }

    fn query_parameters(doc: &Document, path: &str) -> Vec<(String, Option<String>)> {
        doc.paths[path]
            .get
            .as_ref()
            .unwrap()
            .parameters
            .iter()
            .filter(|p| p.location == "query")
            .map(|p| (p.name.clone(), p.example.clone()))
            .collect()
    }

    /// The case that prompted this: `?name={{name}}` typed into the URL used
    /// to vanish from the document, because only Params rows were described.
    #[test]
    fn a_query_typed_into_the_url_becomes_query_parameters() {
        let saved = request(
            "root",
            HttpMethod::Get,
            "https://postman-echo.com/get?name={{name}}&page=2&verbose",
        );

        let (doc, notes) = export(&[saved], &[]);

        assert_eq!(
            query_parameters(&doc, "/get"),
            vec![
                ("name".to_string(), Some("{{name}}".to_string())),
                ("page".to_string(), Some("2".to_string())),
                ("verbose".to_string(), None),
            ]
        );
        let op = doc.paths["/get"].get.as_ref().unwrap();
        assert!(op
            .parameters
            .iter()
            .all(|p| p.schema.is_some() && p.required.is_none()));
        assert!(notes.is_empty(), "{notes:?}");
    }

    #[test]
    fn a_name_in_both_the_url_and_the_params_tab_is_one_parameter() {
        let mut saved = request(
            "Search",
            HttpMethod::Get,
            "https://x.test/search?q=from-url&id=1&id=2",
        );
        saved.request.query_params = vec![
            KeyValue::new("q", "from-tab"),
            KeyValue::new("limit", "10"),
            KeyValue::new("limit", "20"),
        ];

        let (doc, _) = export(&[saved], &[]);

        assert_eq!(
            query_parameters(&doc, "/search"),
            vec![
                ("q".to_string(), Some("from-url".to_string())),
                ("id".to_string(), Some("1".to_string())),
                ("limit".to_string(), Some("10".to_string())),
            ]
        );
    }

    /// The URL is where a pasted-in key most often hides. Its name says what
    /// it is, so the parameter is described and the value is not.
    #[test]
    fn a_credential_in_the_url_query_is_described_without_its_value() {
        let saved = request(
            "Search",
            HttpMethod::Get,
            "https://x.test/search?q=cats&access_token=LEAKME&Signature=LEAKME2",
        );

        let (doc, notes) = export(&[saved], &[]);

        assert_eq!(
            query_parameters(&doc, "/search"),
            vec![
                ("q".to_string(), Some("cats".to_string())),
                ("access_token".to_string(), None),
                ("Signature".to_string(), None),
            ]
        );
        let rendered = format!("{doc:?}");
        assert!(!rendered.contains("LEAKME"), "{rendered}");
        let omitted: Vec<&str> = notes
            .iter()
            .filter_map(|note| match note {
                ExportNote::CredentialParameterValueOmitted { parameter, .. } => {
                    Some(parameter.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(omitted, vec!["access_token", "Signature"]);
    }

    /// The same rule for the Params tab, which used to export such a value.
    #[test]
    fn a_credential_in_the_params_tab_is_described_without_its_value() {
        let mut saved = request("Search", HttpMethod::Get, "https://x.test/search");
        saved.request.query_params = vec![KeyValue::new("token", "LEAKME")];

        let (doc, notes) = export(&[saved], &[]);

        assert_eq!(
            query_parameters(&doc, "/search"),
            vec![("token".to_string(), None)]
        );
        assert!(!format!("{doc:?}").contains("LEAKME"));
        assert_eq!(notes.len(), 1, "{notes:?}");
    }

    /// A placeholder names a variable rather than holding a secret, so it is
    /// kept, and there is nothing to report.
    #[test]
    fn a_credential_parameter_holding_only_a_placeholder_keeps_it() {
        let saved = request(
            "Search",
            HttpMethod::Get,
            "https://x.test/search?api_key={{api_key}}",
        );

        let (doc, notes) = export(&[saved], &[]);

        assert_eq!(
            query_parameters(&doc, "/search"),
            vec![("api_key".to_string(), Some("{{api_key}}".to_string()))]
        );
        assert!(notes.is_empty(), "{notes:?}");
    }

    /// A credential named in both places is reported once, not twice.
    #[test]
    fn a_credential_repeated_across_url_and_tab_is_reported_once() {
        let mut saved = request("Search", HttpMethod::Get, "https://x.test/search?token=A");
        saved.request.query_params = vec![KeyValue::new("token", "B")];

        let (doc, notes) = export(&[saved], &[]);

        assert_eq!(query_parameters(&doc, "/search").len(), 1);
        assert_eq!(notes.len(), 1, "{notes:?}");
        let rendered = format!("{doc:?}");
        assert!(
            !rendered.contains("\"A\"") && !rendered.contains("\"B\""),
            "{rendered}"
        );
    }

    /// An API key the Auth tab sends in the query is suppressed wherever the
    /// same name appears, including the URL.
    #[test]
    fn an_auth_query_key_typed_into_the_url_is_suppressed_too() {
        let mut saved = request(
            "Search",
            HttpMethod::Get,
            "https://x.test/search?k=SECRET&q=1",
        );
        saved.request.auth = Auth::ApiKey {
            key: "k".into(),
            value: "SECRET".into(),
            location: ApiKeyLocation::Query,
        };

        let (doc, _) = export(&[saved], &[]);

        assert_eq!(
            query_parameters(&doc, "/search"),
            vec![("q".to_string(), Some("1".to_string()))]
        );
        assert!(!format!("{doc:?}").contains("SECRET"));
    }

    /// OpenAPI requires parameters to be unique by name and location. HTTP
    /// header names are case-insensitive, so `Accept` and `accept` collide.
    #[test]
    fn a_repeated_header_is_described_once() {
        let mut saved = request("Me", HttpMethod::Get, "https://x.test/me");
        saved.request.headers = vec![
            KeyValue::new("Accept", "application/json"),
            KeyValue::new("accept", "text/plain"),
        ];

        let (doc, _) = export(&[saved], &[]);

        let op = doc.paths["/me"].get.as_ref().unwrap();
        let headers: Vec<(&str, Option<&str>)> = op
            .parameters
            .iter()
            .filter(|p| p.location == "header")
            .map(|p| (p.name.as_str(), p.example.as_deref()))
            .collect();
        assert_eq!(headers, vec![("Accept", Some("application/json"))]);
    }

    /// A path parameter and a query parameter may share a name: they differ
    /// in location, which is part of a parameter's identity.
    #[test]
    fn a_path_and_a_query_parameter_may_share_a_name() {
        let saved = request("User", HttpMethod::Get, "https://x.test/users/{{id}}?id=7");

        let (doc, _) = export(&[saved], &[]);

        let op = doc.paths["/users/{id}"].get.as_ref().unwrap();
        let identities: Vec<(&str, &str)> = op
            .parameters
            .iter()
            .map(|p| (p.name.as_str(), p.location.as_str()))
            .collect();
        assert_eq!(identities, vec![("id", "path"), ("id", "query")]);
    }

    #[test]
    fn an_authorization_header_is_dropped_with_a_note() {
        let mut saved = request("Me", HttpMethod::Get, "https://api.x.test/me");
        saved.request.headers = vec![
            KeyValue::new("Authorization", "Bearer SECRET"),
            KeyValue::new("X-Trace", "abc"),
        ];

        let (doc, notes) = export(&[saved], &[]);

        let op = doc.paths["/me"].get.as_ref().unwrap();
        let names: Vec<&str> = op.parameters.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["X-Trace"]);
        assert!(!format!("{doc:?}").contains("SECRET"));
        assert!(notes
            .iter()
            .any(|n| matches!(n, ExportNote::CredentialHeaderOmitted { .. })));
    }

    #[test]
    fn path_parameters_are_required_because_the_spec_allows_nothing_else() {
        let (doc, _) = export(
            &[request(
                "Get user",
                HttpMethod::Get,
                "{{base_url}}/users/{{userId}}",
            )],
            &[],
        );

        let op = doc.paths["/users/{userId}"].get.as_ref().unwrap();
        let param = op
            .parameters
            .iter()
            .find(|p| p.name == "userId")
            .expect("path parameter");
        assert_eq!(param.location, "path");
        assert_eq!(param.required, Some(true));
    }

    #[test]
    fn operation_ids_are_unique_even_when_two_requests_share_a_name() {
        let (doc, _) = export(
            &[
                request("List users", HttpMethod::Get, "https://api.x.test/a"),
                request("List users", HttpMethod::Get, "https://api.x.test/b"),
            ],
            &[],
        );

        let first = doc.paths["/a"].get.as_ref().unwrap().operation_id.clone();
        let second = doc.paths["/b"].get.as_ref().unwrap().operation_id.clone();
        assert_eq!(first.as_deref(), Some("list_users"));
        assert_eq!(second.as_deref(), Some("list_users_2"));
    }

    #[test]
    fn a_folder_becomes_a_tag_on_its_requests() {
        let folder = Folder {
            id: "fld_1".into(),
            collection_id: "col_1".into(),
            parent_folder_id: None,
            name: "Users".into(),
        };
        let mut saved = request("List", HttpMethod::Get, "https://api.x.test/users");
        saved.folder_id = Some("fld_1".into());

        let (doc, _) = export(&[saved], &[folder]);

        assert_eq!(doc.tags[0].name, "Users");
        assert_eq!(
            doc.paths["/users"].get.as_ref().unwrap().tags,
            vec!["Users"]
        );
    }

    /// Without saved responses there is nothing truthful to put in `responses`,
    /// and the placeholder says so rather than inventing a 200.
    #[test]
    fn an_operation_with_no_saved_response_gets_an_honest_placeholder() {
        let (doc, _) = export(
            &[request("List", HttpMethod::Get, "https://api.x.test/users")],
            &[],
        );

        let responses = &doc.paths["/users"].get.as_ref().unwrap().responses;
        assert!(responses.contains_key("default"));
        assert!(!responses.contains_key("200"));
    }

    #[test]
    fn a_raw_body_is_carried_as_a_serialized_example() {
        let mut saved = request("Create", HttpMethod::Post, "https://api.x.test/users");
        saved.request.body = RequestBody::Raw {
            content_type: "application/json".into(),
            text: "{\"name\":\"Ada\"}".into(),
        };

        let (doc, _) = export(&[saved], &[]);

        let body = doc.paths["/users"]
            .post
            .as_ref()
            .unwrap()
            .request_body
            .as_ref()
            .unwrap();
        let media = &body.content["application/json"];
        assert_eq!(
            media.examples["saved"].serialized_value.as_deref(),
            Some("{\"name\":\"Ada\"}")
        );
    }

    #[test]
    fn the_file_name_is_a_slug_of_the_collection_and_names_the_version() {
        let json = ExportFormat::Json;
        assert_eq!(
            document_file_name("Work API", OpenApiVersion::V3_2, json),
            "work_api.openapi-3.2.json"
        );
        // Exporting the same collection at both versions must not offer to
        // overwrite the first file.
        assert_eq!(
            document_file_name("Work API", OpenApiVersion::V3_1, json),
            "work_api.openapi-3.1.json"
        );
        assert_eq!(
            document_file_name("Work API", OpenApiVersion::V3_0, json),
            "work_api.openapi-3.0.json"
        );
        assert_eq!(
            document_file_name("  ", OpenApiVersion::V3_1, json),
            "collection.openapi-3.1.json"
        );
        assert_eq!(
            document_file_name("¿?", OpenApiVersion::V3_2, json),
            "collection.openapi-3.2.json"
        );
    }

    #[test]
    fn a_yaml_export_is_named_with_the_extension_the_spec_recommends() {
        assert_eq!(
            document_file_name("Work API", OpenApiVersion::V3_2, ExportFormat::Yaml),
            "work_api.openapi-3.2.yaml"
        );
    }

    #[test]
    fn a_document_declares_the_version_it_was_asked_for() {
        let requests = [request("List", HttpMethod::Get, "https://api.x.test/users")];

        for (version, expected) in [
            (OpenApiVersion::V3_0, "3.0.0"),
            (OpenApiVersion::V3_1, "3.1.0"),
            (OpenApiVersion::V3_2, "3.2.0"),
        ] {
            let (doc, _) = export_as(&requests, &[], &BTreeMap::new(), version);

            assert_eq!(doc.openapi, expected);
        }
    }

    /// `paths` is REQUIRED at the top level in 3.0 (it only became optional in
    /// 3.1), and `Document.paths` is the one map serialised unconditionally.
    /// A collection whose every URL was unmappable still has to emit `{}`.
    #[test]
    fn a_3_0_document_always_carries_paths_even_when_empty() {
        let requests = [request("Bad", HttpMethod::Get, "not-a-url")];
        let (doc, _) = export_as(&requests, &[], &BTreeMap::new(), OpenApiVersion::V3_0);

        assert!(doc.paths.is_empty());
        let json = serde_json::to_string(&doc).expect("document should serialise");
        assert!(json.contains(r#""paths":{}"#), "{json}");
    }

    /// 3.0's Info Object has no `summary` — that arrived in 3.1 — and its
    /// schema sets `additionalProperties: false`. We have nothing to put there
    /// anyway; this is the tripwire for the day someone does.
    #[test]
    fn a_3_0_document_carries_no_info_summary() {
        let requests = [request("List", HttpMethod::Get, "https://api.x.test/users")];
        let (doc, _) = export_as(&requests, &[], &BTreeMap::new(), OpenApiVersion::V3_0);

        assert!(doc.info.summary.is_none());
    }

    #[test]
    fn a_3_0_export_decodes_bodies_the_same_way_3_1_does() {
        let saved = request("List", HttpMethod::Get, "https://api.x.test/users");
        let examples = BTreeMap::from([(
            saved.id.clone(),
            vec![saved_example(&saved.id, "application/json", r#"{"id":7}"#)],
        )]);

        let (doc, notes) = export_as(&[saved], &[], &examples, OpenApiVersion::V3_0);

        let example = saved_response_example(&doc, "application/json");
        assert_eq!(example.value, Some(serde_json::json!({ "id": 7 })));
        assert!(example.serialized_value.is_none());
        assert!(!format!("{doc:?}").contains("serializedValue"));
        assert!(notes.contains(&ExportNote::ExampleBodyDecoded), "{notes:?}");
    }

    /// The whole reason 3.1 needed work at all: its Example Object sets
    /// `unevaluatedProperties: false`, so `serializedValue` there is not
    /// ignored — it invalidates the document.
    #[test]
    fn a_json_response_body_is_decoded_into_value_for_3_1() {
        let saved = request("List", HttpMethod::Get, "https://api.x.test/users");
        let examples = BTreeMap::from([(
            saved.id.clone(),
            vec![saved_example(
                &saved.id,
                "application/json; charset=utf-8",
                r#"{"id":7}"#,
            )],
        )]);

        let (doc, notes) = export_as(&[saved], &[], &examples, OpenApiVersion::V3_1);

        let example = saved_response_example(&doc, "application/json");
        assert_eq!(example.value, Some(serde_json::json!({ "id": 7 })));
        assert!(example.serialized_value.is_none());
        assert!(!format!("{doc:?}").contains("serializedValue"));
        assert!(notes.contains(&ExportNote::ExampleBodyDecoded), "{notes:?}");
    }

    /// `text/plain`'s decoded form *is* a string, so this is the right answer
    /// rather than a fallback.
    #[test]
    fn a_non_json_body_stays_a_string_in_value() {
        let saved = request("List", HttpMethod::Get, "https://api.x.test/users");
        let examples = BTreeMap::from([(
            saved.id.clone(),
            vec![saved_example(&saved.id, "text/plain", "hello")],
        )]);

        let (doc, _) = export_as(&[saved], &[], &examples, OpenApiVersion::V3_1);

        let example = saved_response_example(&doc, "text/plain");
        assert_eq!(example.value, Some(serde_json::json!("hello")));
    }

    /// Reproducing what the server actually sent beats discarding it.
    #[test]
    fn a_body_that_claims_json_but_does_not_parse_is_kept_verbatim() {
        let saved = request("List", HttpMethod::Get, "https://api.x.test/users");
        let examples = BTreeMap::from([(
            saved.id.clone(),
            vec![saved_example(&saved.id, "application/json", "not json {")],
        )]);

        let (doc, _) = export_as(&[saved], &[], &examples, OpenApiVersion::V3_1);

        let example = saved_response_example(&doc, "application/json");
        assert_eq!(example.value, Some(serde_json::json!("not json {")));
    }

    #[test]
    fn the_structured_json_suffix_counts_as_json() {
        let saved = request("List", HttpMethod::Get, "https://api.x.test/users");
        let examples = BTreeMap::from([(
            saved.id.clone(),
            vec![saved_example(
                &saved.id,
                "application/problem+json",
                r#"{"title":"nope"}"#,
            )],
        )]);

        let (doc, _) = export_as(&[saved], &[], &examples, OpenApiVersion::V3_1);

        let example = saved_response_example(&doc, "application/problem+json");
        assert_eq!(example.value, Some(serde_json::json!({ "title": "nope" })));
    }

    #[test]
    fn the_same_body_keeps_its_encoding_for_3_2() {
        let saved = request("List", HttpMethod::Get, "https://api.x.test/users");
        let examples = BTreeMap::from([(
            saved.id.clone(),
            vec![saved_example(&saved.id, "application/json", r#"{"id":7}"#)],
        )]);

        let (doc, notes) = export_as(&[saved], &[], &examples, OpenApiVersion::V3_2);

        let example = saved_response_example(&doc, "application/json");
        assert_eq!(example.serialized_value.as_deref(), Some(r#"{"id":7}"#));
        assert!(example.value.is_none());
        assert!(
            !notes.contains(&ExportNote::ExampleBodyDecoded),
            "{notes:?}"
        );
    }

    /// A 3.1 export with nothing opted in has nothing to say about bodies.
    #[test]
    fn no_saved_responses_means_no_downgrade_note() {
        let requests = [request("List", HttpMethod::Get, "https://api.x.test/users")];
        let (_, notes) = export_as(&requests, &[], &BTreeMap::new(), OpenApiVersion::V3_1);

        assert!(notes.is_empty(), "{notes:?}");
    }

    /// The schema and the example come from one parse of one body, so a
    /// document can never describe a shape its own example contradicts.
    #[test]
    fn a_json_request_body_carries_an_inferred_schema_beside_its_example() {
        let mut saved = request("Create", HttpMethod::Post, "https://api.x.test/users");
        saved.request.body = RequestBody::Raw {
            content_type: "application/json".into(),
            text: r#"{"name":"Ada","age":36}"#.into(),
        };

        let (doc, _) = export(&[saved], &[]);

        let body = doc.paths["/users"]
            .post
            .as_ref()
            .expect("a POST")
            .request_body
            .as_ref()
            .expect("a request body");
        let schema = body.content["application/json"]
            .schema
            .as_ref()
            .expect("an inferred schema");
        assert_eq!(schema.kind, Some(SchemaType::One("object".into())));
        assert_eq!(schema.required, vec!["age".to_string(), "name".to_string()]);
    }

    /// Two saved responses for one status code are two samples of one shape.
    /// `note` is missing from the second, so it is a property but not required
    /// — the intersection rule, reaching the document.
    #[test]
    fn response_schemas_intersect_required_across_saved_examples() {
        let saved = request("List", HttpMethod::Get, "https://api.x.test/users");
        let mut first = saved_example(&saved.id, "application/json", r#"{"id":1,"note":"hi"}"#);
        first.name = "First".into();
        let mut second = saved_example(&saved.id, "application/json", r#"{"id":2}"#);
        second.id = "ex_second".into();
        second.name = "Second".into();
        let examples = BTreeMap::from([(saved.id.clone(), vec![first, second])]);

        let (doc, _) = export_as(&[saved], &[], &examples, OpenApiVersion::V3_1);

        let schema = doc.paths["/users"].get.as_ref().expect("a GET").responses["200"].content
            ["application/json"]
            .schema
            .as_ref()
            .expect("an inferred schema");
        assert_eq!(schema.required, vec!["id".to_string()]);
        assert!(schema.properties.contains_key("note"));
    }

    #[test]
    fn a_url_the_app_could_not_send_is_reported_not_exported() {
        let (doc, notes) = export(&[request("Bad", HttpMethod::Get, "not-a-url")], &[]);

        assert!(doc.paths.is_empty());
        assert!(matches!(
            notes.as_slice(),
            [ExportNote::UnmappableUrl { .. }]
        ));
    }
    // --- Item documentation (PLAN.md Phase 12) ---------------------------

    #[test]
    fn a_collections_documentation_becomes_the_documents_description() {
        let docs = ExportDocs {
            collection: "# Billing API\n\nEverything about invoices.".into(),
            ..ExportDocs::default()
        };

        let (doc, _) = export_documented(
            &[request(
                "List users",
                HttpMethod::Get,
                "https://api.example.com/users",
            )],
            &[],
            &BTreeMap::new(),
            OpenApiVersion::V3_2,
            &docs,
        );

        assert_eq!(
            doc.info.description.as_deref(),
            Some("# Billing API\n\nEverything about invoices.")
        );
    }

    /// The generated provenance line is a placeholder for an empty
    /// description, not a preamble to glue onto prose the user wrote.
    #[test]
    fn an_undocumented_collection_keeps_the_generated_description() {
        let (doc, _) = export(
            &[request(
                "List users",
                HttpMethod::Get,
                "https://api.example.com/users",
            )],
            &[],
        );

        assert!(doc
            .info
            .description
            .as_deref()
            .expect("a description")
            .contains("Exported from the ResponderHTTP collection"));
    }

    #[test]
    fn a_folders_documentation_becomes_its_tags_description() {
        let folder = folder("fld_1", "Users");
        let mut saved = request(
            "List users",
            HttpMethod::Get,
            "https://api.example.com/users",
        );
        saved.folder_id = Some("fld_1".into());
        let docs = ExportDocs {
            folders: BTreeMap::from([(
                "fld_1".to_string(),
                "User management endpoints.".to_string(),
            )]),
            ..ExportDocs::default()
        };

        let (doc, _) = export_documented(
            &[saved],
            &[folder],
            &BTreeMap::new(),
            OpenApiVersion::V3_2,
            &docs,
        );

        assert_eq!(doc.tags.len(), 1);
        assert_eq!(
            doc.tags[0].description.as_deref(),
            Some("User management endpoints.")
        );
    }

    #[test]
    fn a_requests_documentation_becomes_its_operations_description() {
        let saved = request(
            "List users",
            HttpMethod::Get,
            "https://api.example.com/users",
        );
        let docs = ExportDocs {
            requests: BTreeMap::from([(
                saved.id.clone(),
                "Returns 409 on a duplicate.".to_string(),
            )]),
            ..ExportDocs::default()
        };

        let (doc, _) =
            export_documented(&[saved], &[], &BTreeMap::new(), OpenApiVersion::V3_2, &docs);

        let operation = doc.paths["/users"].get.as_ref().expect("a GET");
        assert_eq!(
            operation.description.as_deref(),
            Some("Returns 409 on a duplicate.")
        );
        // The name still travels as the summary; documentation is the prose
        // under it, not a replacement for it.
        assert_eq!(operation.summary.as_deref(), Some("List users"));
    }

    /// An empty column is the common case — most items are never documented
    /// — and it must not write `description: ""` into every operation.
    #[test]
    fn an_undocumented_item_writes_no_description_at_all() {
        let saved = request(
            "List users",
            HttpMethod::Get,
            "https://api.example.com/users",
        );
        let docs = ExportDocs {
            requests: BTreeMap::from([(saved.id.clone(), "   \n  ".to_string())]),
            ..ExportDocs::default()
        };

        let (doc, _) =
            export_documented(&[saved], &[], &BTreeMap::new(), OpenApiVersion::V3_2, &docs);

        assert!(doc.paths["/users"]
            .get
            .as_ref()
            .expect("a GET")
            .description
            .is_none());
    }

    /// Markdown is whitespace sensitive: four leading spaces are a code
    /// block. The emptiness test must not double as an edit.
    #[test]
    fn documentation_is_exported_exactly_as_written() {
        let saved = request(
            "List users",
            HttpMethod::Get,
            "https://api.example.com/users",
        );
        let text = "    indented code\n\nand a break  \n";
        let docs = ExportDocs {
            requests: BTreeMap::from([(saved.id.clone(), text.to_string())]),
            ..ExportDocs::default()
        };

        let (doc, _) =
            export_documented(&[saved], &[], &BTreeMap::new(), OpenApiVersion::V3_2, &docs);

        assert_eq!(
            doc.paths["/users"]
                .get
                .as_ref()
                .expect("a GET")
                .description
                .as_deref(),
            Some(text)
        );
    }
}
