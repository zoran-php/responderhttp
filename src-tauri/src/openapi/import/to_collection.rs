// http_client/src-tauri/src/openapi/import/to_collection.rs
//
// A validated OpenAPI document → an `ImportPlan`, plus notes on everything the
// app could not carry over (PLAN.md Phase 8d, step 4). Pure.
//
// Two rules hold throughout:
//
// - **No literal secret is ever created.** Every credential slot — auth
//   fields, credential-named parameters — gets a `{{placeholder}}`, and the
//   environment variable behind it is created empty and flagged secret. An
//   example value in the document never reaches a credential field.
// - **The URL is the only place query parameters live**, as it has been since
//   the Params tab was synced with the URL bar; the params table is left
//   empty.
//
// Notes that could repeat once per operation are tallied, so a spec with a
// thousand operations produces a report of a dozen lines, not a thousand.
use std::collections::BTreeMap;

use serde_json::Value;

use crate::domain::import_plan::{ImportPlan, PlannedEnvironment, PlannedExample, PlannedRequest};
use crate::domain::models::{
    ApiKeyLocation, Auth, EnvironmentVariable, HttpMethod, HttpRequest, KeyValue, MultipartPart,
    RequestBody, RequestSettings,
};
use crate::domain::secrets::is_credential_param_name;
use crate::openapi::import::grouping::{self, Arrangement, Grouping, Placement};
use crate::openapi::import::refs::Refs;
use crate::openapi::import::sample::{declared_value, sample};

/// The methods this app can send, in the order a Path Item lists them.
const SUPPORTED_METHODS: [(&str, HttpMethod); 7] = [
    ("get", HttpMethod::Get),
    ("put", HttpMethod::Put),
    ("post", HttpMethod::Post),
    ("delete", HttpMethod::Delete),
    ("options", HttpMethod::Options),
    ("head", HttpMethod::Head),
    ("patch", HttpMethod::Patch),
];
/// Operations the spec defines and this app cannot send.
const UNSUPPORTED_METHODS: [&str; 2] = ["trace", "query"];

/// Used when the document has no `servers`, or a server with no URL.
pub const BASE_URL_VARIABLE: &str = "baseUrl";
const FALLBACK_TITLE: &str = "Imported API";
const DEPRECATED_SUFFIX: &str = " (deprecated)";
/// Header parameters the spec says are ignored: the body, content
/// negotiation and auth set these.
const IGNORED_HEADER_PARAMETERS: [&str; 3] = ["accept", "content-type", "authorization"];
/// Header names that carry credentials or session state even though the
/// credential list for query names does not cover them.
const CREDENTIAL_HEADERS: [&str; 2] = ["cookie", "proxy-authorization"];
/// Placeholder variables for the auth schemes the app supports.
const USERNAME_VARIABLE: &str = "username";
const PASSWORD_VARIABLE: &str = "password";
const TOKEN_VARIABLE: &str = "token";
const API_KEY_VARIABLE: &str = "apiKey";
/// How many request names a tallied note quotes.
const TALLY_SAMPLE: usize = 3;
/// How many alternative servers a note lists.
const MAX_LISTED_SERVERS: usize = 5;
/// Status used for a `default` response's example.
const DEFAULT_RESPONSE_STATUS: u16 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportOptions {
    pub grouping: Grouping,
    pub include_examples: bool,
    pub create_environment: bool,
}

/// How often something happened, and a few of the requests it happened on.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tally {
    pub count: usize,
    pub sample: Vec<String>,
}

impl Tally {
    fn add(&mut self, request: &str) {
        self.count += 1;
        if self.sample.len() < TALLY_SAMPLE && !self.sample.iter().any(|name| name == request) {
            self.sample.push(request.to_string());
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IgnoredFeature {
    Webhooks,
    Callbacks,
    Links,
    ExternalExamples,
}

/// Something the import could not carry over, or had to guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportNote {
    /// The document names a JSON Schema dialect of its own, so validation
    /// checked its structure but not its Schema Objects.
    SchemaObjectsUnchecked,
    UnsupportedMethod {
        method: String,
        path: String,
    },
    Ignored {
        feature: IgnoredFeature,
        count: usize,
    },
    NoServer,
    RelativeServer {
        url: String,
    },
    OtherServersIgnored {
        urls: Vec<String>,
    },
    /// Requests whose path or operation names its own server; their URLs
    /// were written out in full instead of starting with the base variable.
    OwnServer(Tally),
    CookieParameters(Tally),
    QuerystringParameters(Tally),
    OptionalParametersLeftOut(Tally),
    UnsupportedBody {
        media_type: String,
        requests: Tally,
    },
    FilePartsLeftOut(Tally),
    UnsupportedSecurity {
        scheme: String,
        kind: String,
        requests: Tally,
    },
    UnknownSecurityScheme {
        scheme: String,
        requests: Tally,
    },
    /// A requirement that combines several schemes; only the first was used.
    CombinedSecurity(Tally),
    TagNotNested {
        tag: String,
    },
    /// Response codes like `2XX` or `default` have no single status, so their
    /// examples got one.
    ExampleStatusGuessed(Tally),
}

/// The plan and its report.
#[derive(Debug, Clone)]
pub struct Mapped {
    pub plan: ImportPlan,
    pub notes: Vec<ImportNote>,
}

/// One operation the app can import.
pub struct OperationRef<'a> {
    pub path: &'a str,
    pub method: HttpMethod,
    pub path_item: &'a Value,
    pub operation: &'a Value,
}

/// Everything in `paths` this app can send, in document order, plus notes
/// on what it cannot.
pub fn operations<'a>(refs: Refs<'a>) -> (Vec<OperationRef<'a>>, Vec<ImportNote>) {
    let mut found = Vec::new();
    let mut notes = Vec::new();
    let Some(paths) = refs.root().get("paths").and_then(Value::as_object) else {
        return (found, notes);
    };
    for (path, raw_item) in paths {
        // The Paths Object allows `x-` extensions beside the paths.
        if path.starts_with("x-") {
            continue;
        }
        let Some(path_item) = refs.resolve(raw_item) else {
            continue;
        };
        for (key, method) in SUPPORTED_METHODS {
            if let Some(operation) = path_item.get(key).and_then(|op| refs.resolve(op)) {
                found.push(OperationRef {
                    path,
                    method,
                    path_item,
                    operation,
                });
            }
        }
        for key in UNSUPPORTED_METHODS {
            if path_item.get(key).is_some() {
                notes.push(ImportNote::UnsupportedMethod {
                    method: key.to_ascii_uppercase(),
                    path: path.clone(),
                });
            }
        }
        if let Some(extra) = path_item
            .get("additionalOperations")
            .and_then(Value::as_object)
        {
            for method in extra.keys() {
                notes.push(ImportNote::UnsupportedMethod {
                    method: method.clone(),
                    path: path.clone(),
                });
            }
        }
    }
    (found, notes)
}

pub fn placements<'a>(operations: &[OperationRef<'a>]) -> Vec<Placement<'a>> {
    operations
        .iter()
        .map(|operation| Placement {
            path: operation.path,
            tags: operation
                .operation
                .get("tags")
                .and_then(Value::as_array)
                .map(|tags| tags.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default(),
        })
        .collect()
}

/// The folders each grouping would create, for the preview.
pub fn arrangement_for(refs: Refs<'_>, grouping: Grouping) -> Arrangement {
    let (operations, _) = operations(refs);
    grouping::arrange(
        grouping,
        &placements(&operations),
        &grouping::declared_tags(refs.root()),
    )
}

pub fn default_grouping(refs: Refs<'_>) -> Grouping {
    let (operations, _) = operations(refs);
    grouping::default_grouping(&placements(&operations))
}

pub fn to_plan(document: &Value, options: &ImportOptions) -> Mapped {
    let refs = Refs::new(document);
    let mut notes = Notes::default();
    let mut environment = EnvironmentBuilder::default();

    let (operations, operation_notes) = operations(refs);
    notes.fixed.extend(operation_notes);
    note_ignored_features(document, &operations, &mut notes);

    let base = base_url(document, options.create_environment, &mut notes);
    if let Some(variable) = &base.variable {
        environment.add(variable.clone());
    }

    let arrangement = grouping::arrange(
        options.grouping,
        &placements(&operations),
        &grouping::declared_tags(document),
    );
    for tag in &arrangement.unnested_tags {
        notes
            .fixed
            .push(ImportNote::TagNotNested { tag: tag.clone() });
    }

    let requests = operations
        .iter()
        .zip(&arrangement.assignment)
        .map(|(operation, folder)| {
            let mut context = RequestContext {
                refs,
                options,
                base: &base,
                notes: &mut notes,
                environment: &mut environment,
            };
            context.request(operation, *folder)
        })
        .collect();

    let collection_name = title(document);
    let plan = ImportPlan {
        environment: options.create_environment.then(|| PlannedEnvironment {
            name: collection_name.clone(),
            variables: environment.variables,
        }),
        collection_name,
        folders: arrangement.folders,
        requests,
    };
    Mapped {
        plan,
        notes: notes.finish(),
    }
}

fn title(document: &Value) -> String {
    document
        .pointer("/info/title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or(FALLBACK_TITLE)
        .to_string()
}

/// Collects notes, tallying the ones that repeat per request.
#[derive(Default)]
struct Notes {
    fixed: Vec<ImportNote>,
    own_server: Tally,
    cookie_parameters: Tally,
    querystring_parameters: Tally,
    optional_left_out: Tally,
    file_parts: Tally,
    combined_security: Tally,
    status_guessed: Tally,
    unsupported_bodies: BTreeMap<String, Tally>,
    unsupported_security: BTreeMap<(String, String), Tally>,
    unknown_schemes: BTreeMap<String, Tally>,
    external_examples: usize,
}

impl Notes {
    fn finish(self) -> Vec<ImportNote> {
        let mut notes = self.fixed;
        if self.external_examples > 0 {
            notes.push(ImportNote::Ignored {
                feature: IgnoredFeature::ExternalExamples,
                count: self.external_examples,
            });
        }
        let tallies = [
            (
                self.own_server,
                ImportNote::OwnServer as fn(Tally) -> ImportNote,
            ),
            (
                self.optional_left_out,
                ImportNote::OptionalParametersLeftOut,
            ),
            (self.cookie_parameters, ImportNote::CookieParameters),
            (
                self.querystring_parameters,
                ImportNote::QuerystringParameters,
            ),
            (self.file_parts, ImportNote::FilePartsLeftOut),
            (self.combined_security, ImportNote::CombinedSecurity),
            (self.status_guessed, ImportNote::ExampleStatusGuessed),
        ];
        for (tally, make) in tallies {
            if tally.count > 0 {
                notes.push(make(tally));
            }
        }
        for (media_type, requests) in self.unsupported_bodies {
            notes.push(ImportNote::UnsupportedBody {
                media_type,
                requests,
            });
        }
        for ((scheme, kind), requests) in self.unsupported_security {
            notes.push(ImportNote::UnsupportedSecurity {
                scheme,
                kind,
                requests,
            });
        }
        for (scheme, requests) in self.unknown_schemes {
            notes.push(ImportNote::UnknownSecurityScheme { scheme, requests });
        }
        notes
    }
}

fn note_ignored_features(document: &Value, operations: &[OperationRef<'_>], notes: &mut Notes) {
    let webhooks = document
        .get("webhooks")
        .and_then(Value::as_object)
        .map_or(0, |hooks| hooks.len());
    let callbacks = operations
        .iter()
        .filter(|operation| {
            operation
                .operation
                .get("callbacks")
                .and_then(Value::as_object)
                .is_some_and(|callbacks| !callbacks.is_empty())
        })
        .count();
    let links = operations
        .iter()
        .flat_map(|operation| {
            operation
                .operation
                .get("responses")
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|responses| responses.values())
        })
        .filter(|response| {
            response
                .get("links")
                .and_then(Value::as_object)
                .is_some_and(|links| !links.is_empty())
        })
        .count();
    for (feature, count) in [
        (IgnoredFeature::Webhooks, webhooks),
        (IgnoredFeature::Callbacks, callbacks),
        (IgnoredFeature::Links, links),
    ] {
        if count > 0 {
            notes.fixed.push(ImportNote::Ignored { feature, count });
        }
    }
}

/// Environment variables in first-seen order. A name seen twice keeps its
/// first value; if either sighting was a secret, the variable is one.
#[derive(Default)]
struct EnvironmentBuilder {
    variables: Vec<EnvironmentVariable>,
    index: BTreeMap<String, usize>,
}

impl EnvironmentBuilder {
    fn add(&mut self, variable: EnvironmentVariable) {
        match self.index.get(&variable.name) {
            Some(&existing) => {
                let current = &mut self.variables[existing];
                if variable.secret && !current.secret {
                    current.secret = true;
                    current.value = String::new();
                }
            }
            None => {
                self.index
                    .insert(variable.name.clone(), self.variables.len());
                self.variables.push(variable);
            }
        }
    }

    fn secret(&mut self, name: &str) {
        self.add(EnvironmentVariable::secret(name, ""));
    }
}

/// What goes in front of every path.
struct BaseUrl {
    prefix: String,
    variable: Option<EnvironmentVariable>,
}

fn base_url(document: &Value, create_environment: bool, notes: &mut Notes) -> BaseUrl {
    let servers: Vec<&Value> = document
        .get("servers")
        .and_then(Value::as_array)
        .map(|servers| servers.iter().collect())
        .unwrap_or_default();

    if servers.len() > 1 {
        notes.fixed.push(ImportNote::OtherServersIgnored {
            urls: servers[1..]
                .iter()
                .filter_map(|server| server.get("url").and_then(Value::as_str))
                .take(MAX_LISTED_SERVERS)
                .map(str::to_string)
                .collect(),
        });
    }

    let first = servers.first().copied();
    let template = first
        .and_then(|server| server.get("url"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let concrete = first.map_or_else(String::new, concrete_server_url);

    if concrete.is_empty() {
        notes.fixed.push(ImportNote::NoServer);
    } else if !is_absolute(&concrete) {
        notes.fixed.push(ImportNote::RelativeServer {
            url: concrete.clone(),
        });
    }

    if !create_environment {
        return BaseUrl {
            prefix: concrete,
            variable: None,
        };
    }

    // A server that is nothing but one variable — which is what the export
    // writes for a `{{base_url}}` URL — keeps that variable's own name.
    if let Some(name) = sole_variable(template) {
        let default = first
            .and_then(|server| server.pointer(&format!("/variables/{}/default", escape(name))))
            .and_then(Value::as_str)
            .unwrap_or("");
        let value = if is_placeholder_default(default) {
            ""
        } else {
            default.trim_end_matches('/')
        };
        return BaseUrl {
            prefix: format!("{{{{{name}}}}}"),
            variable: Some(EnvironmentVariable::plain(name, value)),
        };
    }

    BaseUrl {
        prefix: format!("{{{{{BASE_URL_VARIABLE}}}}}"),
        variable: Some(EnvironmentVariable::plain(BASE_URL_VARIABLE, concrete)),
    }
}

/// A Server Object's URL with every `{variable}` replaced by its default, and
/// no trailing slash — paths bring their own.
fn concrete_server_url(server: &Value) -> String {
    let mut url = server
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if let Some(variables) = server.get("variables").and_then(Value::as_object) {
        for (name, variable) in variables {
            let default = variable
                .get("default")
                .and_then(Value::as_str)
                .unwrap_or("");
            url = url.replace(&format!("{{{name}}}"), default);
        }
    }
    url.trim_end_matches('/').to_string()
}

fn is_absolute(url: &str) -> bool {
    let lowered = url.to_ascii_lowercase();
    lowered.starts_with("http://") || lowered.starts_with("https://")
}

fn sole_variable(template: &str) -> Option<&str> {
    let name = template.strip_prefix('{')?.strip_suffix('}')?;
    (!name.is_empty() && !name.contains(['{', '}', '/'])).then_some(name)
}

/// The export writes `<name>` as a Server Variable default because the spec
/// demands a default and the real value must not leave the machine.
fn is_placeholder_default(default: &str) -> bool {
    default.len() > 2 && default.starts_with('<') && default.ends_with('>')
}

/// JSON pointer escaping for one token.
fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

fn placeholder(name: &str) -> String {
    format!("{{{{{name}}}}}")
}

/// `/users/{id}` → `/users/{{id}}`.
fn templated_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 8);
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        let Some(length) = rest[start..].find('}') else {
            break;
        };
        let name = &rest[start + 1..start + length];
        out.push_str(&rest[..start]);
        if name.is_empty() {
            out.push_str("{}");
        } else {
            out.push_str(&placeholder(name));
        }
        rest = &rest[start + length + 1..];
    }
    out.push_str(rest);
    if !out.starts_with('/') {
        out.insert(0, '/');
    }
    out
}

/// Structural characters escaped so a value cannot split the query; the rest
/// is left for "Encode URL automatically" at send time, as a typed URL is.
fn encode_query_component(text: &str, is_name: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '%' => out.push_str("%25"),
            '&' => out.push_str("%26"),
            '#' => out.push_str("%23"),
            '+' => out.push_str("%2B"),
            ' ' => out.push_str("%20"),
            '=' if is_name => out.push_str("%3D"),
            other => out.push(other),
        }
    }
    out
}

/// A parameter or field value as text. Arrays use the default `form` style's
/// comma separation; objects stay JSON.
fn value_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Array(_) | Value::Object(_) => item.to_string(),
                scalar => value_text(scalar),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => value.to_string(),
        scalar => scalar.to_string(),
    }
}

/// An example as the document gives it: already serialised text (3.2's
/// `serializedValue`), or a value to be serialised.
enum ExampleValue<'a> {
    Serialized(&'a str),
    Data(&'a Value),
}

/// The value an Example Object carries. `externalValue` points at a file or
/// URL, which the import never reads.
fn example_object_value<'a>(example: &'a Value, notes: &mut Notes) -> Option<ExampleValue<'a>> {
    if let Some(text) = example.get("serializedValue").and_then(Value::as_str) {
        return Some(ExampleValue::Serialized(text));
    }
    if let Some(value) = example.get("dataValue").or_else(|| example.get("value")) {
        return Some(ExampleValue::Data(value));
    }
    if example.get("externalValue").is_some() {
        notes.external_examples += 1;
    }
    None
}

fn is_json(media_type: &str) -> bool {
    let essence = essence(media_type);
    essence == "application/json" || essence.ends_with("+json")
}

fn is_xml(media_type: &str) -> bool {
    let essence = essence(media_type);
    essence == "application/xml" || essence == "text/xml" || essence.ends_with("+xml")
}

fn essence(media_type: &str) -> String {
    media_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BodyKind {
    Json,
    Form,
    Multipart,
    Xml,
    Text,
}

fn body_kind(media_type: &str) -> Option<BodyKind> {
    let essence = essence(media_type);
    if is_json(media_type) {
        Some(BodyKind::Json)
    } else if essence == "application/x-www-form-urlencoded" {
        Some(BodyKind::Form)
    } else if essence == "multipart/form-data" {
        Some(BodyKind::Multipart)
    } else if is_xml(media_type) {
        Some(BodyKind::Xml)
    } else if essence.starts_with("text/") {
        Some(BodyKind::Text)
    } else {
        None
    }
}

/// Body text for a JSON, XML or text media type. A string given for a JSON
/// media type is taken as the body itself — authors often write the JSON out
/// as a string, and the export does the same for a body that is not JSON.
fn body_text(example: Option<ExampleValue<'_>>, json: bool) -> String {
    match example {
        None => String::new(),
        Some(ExampleValue::Serialized(text)) => text.to_string(),
        Some(ExampleValue::Data(Value::String(text))) => text.clone(),
        Some(ExampleValue::Data(value)) if json => {
            serde_json::to_string_pretty(value).unwrap_or_default()
        }
        // Structured data for XML or text has no serialisation the document
        // defines; the body is left for the user to write.
        Some(ExampleValue::Data(Value::Object(_) | Value::Array(_))) => String::new(),
        Some(ExampleValue::Data(value)) => value_text(value),
    }
}

struct RequestContext<'a, 'n> {
    refs: Refs<'a>,
    options: &'n ImportOptions,
    base: &'n BaseUrl,
    notes: &'n mut Notes,
    environment: &'n mut EnvironmentBuilder,
}

/// One parameter, merged from the Path Item and the operation.
struct Parameter<'a> {
    name: &'a str,
    location: &'a str,
    required: bool,
    object: &'a Value,
}

impl<'a> RequestContext<'a, '_> {
    fn request(&mut self, operation: &OperationRef<'a>, folder: Option<usize>) -> PlannedRequest {
        let name = request_name(operation);

        let (prefix, own_server) = match self.own_server(operation) {
            Some(url) => (url, true),
            None => (self.base.prefix.clone(), false),
        };
        if own_server {
            self.notes.own_server.add(&name);
        }

        let auth = self.auth(operation, &name);
        let parameters = self.parameters(operation);
        let mut query = Vec::new();
        let mut headers = Vec::new();
        let mut optional_left_out = false;
        let mut cookie = false;
        let mut querystring = false;

        for parameter in &parameters {
            match parameter.location {
                "path" if self.options.create_environment => {
                    let value = self.parameter_value(parameter.object).unwrap_or_default();
                    self.environment
                        .add(EnvironmentVariable::plain(parameter.name, value));
                }
                "query" => {
                    if auth_uses(&auth, ApiKeyLocation::Query, parameter.name) {
                        continue;
                    }
                    match self.row_value(parameter, false) {
                        Some(value) => query.push(format!(
                            "{}={}",
                            encode_query_component(parameter.name, true),
                            value
                        )),
                        None => optional_left_out = true,
                    }
                }
                "header" => {
                    let lowered = parameter.name.to_ascii_lowercase();
                    if IGNORED_HEADER_PARAMETERS.contains(&lowered.as_str())
                        || auth_uses(&auth, ApiKeyLocation::Header, parameter.name)
                    {
                        continue;
                    }
                    match self.row_value(parameter, true) {
                        Some(value) => headers.push(KeyValue::new(parameter.name, value)),
                        None => optional_left_out = true,
                    }
                }
                "cookie" => cookie = true,
                "querystring" => querystring = true,
                _ => {}
            }
        }
        for (flag, tally) in [
            (optional_left_out, &mut self.notes.optional_left_out),
            (cookie, &mut self.notes.cookie_parameters),
            (querystring, &mut self.notes.querystring_parameters),
        ] {
            if flag {
                tally.add(&name);
            }
        }

        let mut url = format!("{prefix}{}", templated_path(operation.path));
        if !query.is_empty() {
            url.push('?');
            url.push_str(&query.join("&"));
        }

        let body = self.body(operation, &name);
        let request = HttpRequest {
            method: operation.method,
            url,
            headers,
            query_params: Vec::new(),
            body,
            auth,
            settings: RequestSettings::default(),
        };
        let examples = if self.options.include_examples {
            self.examples(operation, &name)
        } else {
            Vec::new()
        };
        PlannedRequest {
            name,
            folder,
            request,
            examples,
        }
    }

    /// A server the path or operation names for itself, with its variables
    /// filled in. Operation beats path, as the spec says.
    fn own_server(&self, operation: &OperationRef<'a>) -> Option<String> {
        [operation.operation, operation.path_item]
            .into_iter()
            .filter_map(|holder| holder.get("servers").and_then(Value::as_array))
            .find_map(|servers| servers.first())
            .map(concrete_server_url)
    }

    /// Path Item parameters, overridden by operation parameters with the same
    /// name and location.
    fn parameters(&self, operation: &OperationRef<'a>) -> Vec<Parameter<'a>> {
        let mut merged: Vec<Parameter<'a>> = Vec::new();
        for holder in [operation.path_item, operation.operation] {
            let Some(list) = holder.get("parameters").and_then(Value::as_array) else {
                continue;
            };
            for raw in list {
                let Some(object) = self.refs.resolve(raw) else {
                    continue;
                };
                let (Some(name), Some(location)) = (
                    object.get("name").and_then(Value::as_str),
                    object.get("in").and_then(Value::as_str),
                ) else {
                    continue;
                };
                let parameter = Parameter {
                    name,
                    location,
                    required: object
                        .get("required")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    object,
                };
                match merged.iter_mut().find(|existing| {
                    existing.location == location
                        && if location == "header" {
                            existing.name.eq_ignore_ascii_case(name)
                        } else {
                            existing.name == name
                        }
                }) {
                    Some(existing) => *existing = parameter,
                    None => merged.push(parameter),
                }
            }
        }
        merged
    }

    /// The text a query or header row gets, or None to leave it out.
    fn row_value(&mut self, parameter: &Parameter<'a>, is_header: bool) -> Option<String> {
        let lowered = parameter.name.to_ascii_lowercase();
        let credential = is_credential_param_name(parameter.name)
            || (is_header && CREDENTIAL_HEADERS.contains(&lowered.as_str()));
        if credential {
            self.environment.secret(parameter.name);
            return Some(placeholder(parameter.name));
        }
        match self.parameter_value(parameter.object) {
            Some(value) if is_header => Some(value),
            Some(value) => Some(encode_query_component(&value, false)),
            None if parameter.required => {
                self.environment
                    .add(EnvironmentVariable::plain(parameter.name, ""));
                Some(placeholder(parameter.name))
            }
            None => None,
        }
    }

    /// A parameter's example, from wherever the document put one: the
    /// parameter itself, its `examples`, its schema, or (for a `content`
    /// parameter) its first media type.
    fn parameter_value(&mut self, parameter: &'a Value) -> Option<String> {
        if let Some(value) = parameter.get("example") {
            return Some(value_text(value));
        }
        if let Some(value) = self.first_example(parameter.get("examples")) {
            return Some(match value {
                ExampleValue::Serialized(text) => text.to_string(),
                ExampleValue::Data(value) => value_text(value),
            });
        }
        let schema = parameter.get("schema").or_else(|| {
            parameter
                .get("content")
                .and_then(Value::as_object)
                .and_then(|content| content.values().next())
                .and_then(|media| {
                    if let Some(value) = media.get("example") {
                        return Some(value);
                    }
                    media.get("schema")
                })
        })?;
        let schema = self.refs.resolve(schema)?;
        declared_value(schema).map(value_text)
    }

    /// The first entry of an `examples` map that carries a value.
    fn first_example(&mut self, examples: Option<&'a Value>) -> Option<ExampleValue<'a>> {
        let examples = examples?.as_object()?;
        for raw in examples.values() {
            let Some(example) = self.refs.resolve(raw) else {
                continue;
            };
            if let Some(value) = example_object_value(example, self.notes) {
                return Some(value);
            }
        }
        None
    }

    fn auth(&mut self, operation: &OperationRef<'a>, request: &str) -> Auth {
        let requirements = operation
            .operation
            .get("security")
            .or_else(|| self.refs.root().get("security"))
            .and_then(Value::as_array);
        let Some(first) = requirements
            .and_then(|list| list.first())
            .and_then(Value::as_object)
        else {
            return Auth::None;
        };
        let Some(scheme_name) = first.keys().next() else {
            // `{}` is "no authentication" offered as an alternative.
            return Auth::None;
        };
        if first.len() > 1 {
            self.notes.combined_security.add(request);
        }

        let scheme = self
            .refs
            .root()
            .pointer(&format!(
                "/components/securitySchemes/{}",
                escape(scheme_name)
            ))
            .and_then(|scheme| self.refs.resolve(scheme));
        let Some(scheme) = scheme else {
            self.notes
                .unknown_schemes
                .entry(scheme_name.clone())
                .or_default()
                .add(request);
            return Auth::None;
        };

        let kind = scheme.get("type").and_then(Value::as_str).unwrap_or("");
        let http_scheme = scheme
            .get("scheme")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        let location = scheme.get("in").and_then(Value::as_str).unwrap_or("");
        let key_name = scheme.get("name").and_then(Value::as_str).unwrap_or("");

        match (kind, http_scheme.as_str(), location) {
            ("http", "basic", _) => {
                self.environment
                    .add(EnvironmentVariable::plain(USERNAME_VARIABLE, ""));
                self.environment.secret(PASSWORD_VARIABLE);
                Auth::Basic {
                    username: placeholder(USERNAME_VARIABLE),
                    password: placeholder(PASSWORD_VARIABLE),
                }
            }
            ("http", "bearer", _) => {
                self.environment.secret(TOKEN_VARIABLE);
                Auth::Bearer {
                    token: placeholder(TOKEN_VARIABLE),
                }
            }
            ("apiKey", _, "header" | "query") if !key_name.trim().is_empty() => {
                let variable = api_key_variable(scheme_name);
                self.environment.secret(&variable);
                Auth::ApiKey {
                    key: key_name.trim().to_string(),
                    value: placeholder(&variable),
                    location: if location == "query" {
                        ApiKeyLocation::Query
                    } else {
                        ApiKeyLocation::Header
                    },
                }
            }
            _ => {
                let described = match kind {
                    "http" => format!("http {http_scheme}"),
                    "apiKey" => format!("apiKey in {location}"),
                    other => other.to_string(),
                };
                self.notes
                    .unsupported_security
                    .entry((scheme_name.clone(), described))
                    .or_default()
                    .add(request);
                Auth::None
            }
        }
    }

    fn body(&mut self, operation: &OperationRef<'a>, request: &str) -> RequestBody {
        let Some(content) = operation
            .operation
            .get("requestBody")
            .and_then(|body| self.refs.resolve(body))
            .and_then(|body| body.get("content"))
            .and_then(Value::as_object)
        else {
            return RequestBody::None;
        };
        let chosen = content
            .iter()
            .filter_map(|(media_type, media)| {
                body_kind(media_type).map(|kind| (kind, media_type, media))
            })
            .min_by_key(|(kind, _, _)| *kind);
        let Some((kind, media_type, media)) = chosen else {
            if let Some(media_type) = content.keys().next() {
                self.notes
                    .unsupported_bodies
                    .entry(essence(media_type))
                    .or_default()
                    .add(request);
            }
            return RequestBody::None;
        };
        let Some(media) = self.refs.resolve(media) else {
            return RequestBody::None;
        };
        let schema = media.get("schema");

        match kind {
            BodyKind::Json | BodyKind::Xml | BodyKind::Text => {
                let example = self.media_example(media);
                let sampled;
                let example = match example {
                    Some(example) => Some(example),
                    None if kind == BodyKind::Json => {
                        sampled = schema.and_then(|schema| sample(self.refs, schema));
                        sampled.as_ref().map(ExampleValue::Data)
                    }
                    None => None,
                };
                RequestBody::Raw {
                    content_type: media_type.clone(),
                    text: body_text(example, kind == BodyKind::Json),
                }
            }
            BodyKind::Form => {
                RequestBody::FormUrlEncoded(self.form_fields(media, schema).unwrap_or_default())
            }
            BodyKind::Multipart => {
                let files = file_properties(self.refs, schema);
                if !files.is_empty() {
                    self.notes.file_parts.add(request);
                }
                let parts = self
                    .form_fields(media, schema)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|field| !files.contains(&field.name))
                    .map(|field| MultipartPart::Text {
                        name: field.name,
                        value: field.value,
                    })
                    .collect();
                RequestBody::Multipart(parts)
            }
        }
    }

    fn media_example(&mut self, media: &'a Value) -> Option<ExampleValue<'a>> {
        if let Some(value) = media.get("example") {
            return Some(ExampleValue::Data(value));
        }
        self.first_example(media.get("examples"))
    }

    /// Form fields from the example if there is one, the schema otherwise.
    fn form_fields(
        &mut self,
        media: &'a Value,
        schema: Option<&'a Value>,
    ) -> Option<Vec<KeyValue>> {
        let value = match self.media_example(media) {
            Some(ExampleValue::Serialized(text)) => return Some(parse_form(text)),
            Some(ExampleValue::Data(Value::String(text))) => return Some(parse_form(text)),
            Some(ExampleValue::Data(value)) => value.clone(),
            None => sample(self.refs, schema?)?,
        };
        let Value::Object(fields) = value else {
            return None;
        };
        Some(
            fields
                .iter()
                .map(|(name, value)| KeyValue::new(name, value_text(value)))
                .collect(),
        )
    }

    fn examples(&mut self, operation: &OperationRef<'a>, request: &str) -> Vec<PlannedExample> {
        let Some(responses) = operation
            .operation
            .get("responses")
            .and_then(Value::as_object)
        else {
            return Vec::new();
        };
        let mut planned = Vec::new();
        for (code, raw) in responses {
            let Some(response) = self.refs.resolve(raw) else {
                continue;
            };
            let Some((status, guessed)) = status_for(code) else {
                continue;
            };
            let Some(content) = response.get("content").and_then(Value::as_object) else {
                continue;
            };
            let description = response
                .get("description")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .unwrap_or(code);
            for (media_type, raw_media) in content {
                let Some(media) = self.refs.resolve(raw_media) else {
                    continue;
                };
                let json = is_json(media_type);
                let headers = vec![KeyValue::new("Content-Type", media_type)];
                let add =
                    |name: &str, value: ExampleValue<'_>, planned: &mut Vec<PlannedExample>| {
                        planned.push(PlannedExample {
                            name: name.to_string(),
                            status,
                            response_headers: headers.clone(),
                            response_body: body_text(Some(value), json),
                        });
                    };
                let before = planned.len();
                if let Some(examples) = media.get("examples").and_then(Value::as_object) {
                    for (key, raw_example) in examples {
                        let Some(example) = self.refs.resolve(raw_example) else {
                            continue;
                        };
                        let name = example
                            .get("summary")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|text| !text.is_empty())
                            .unwrap_or(key);
                        if let Some(value) = example_object_value(example, self.notes) {
                            add(name, value, &mut planned);
                        }
                    }
                } else if let Some(value) = media.get("example") {
                    add(description, ExampleValue::Data(value), &mut planned);
                }
                if guessed {
                    for _ in before..planned.len() {
                        self.notes.status_guessed.add(request);
                    }
                }
            }
        }
        planned
    }
}

fn request_name(operation: &OperationRef<'_>) -> String {
    let text = |key: &str| {
        operation
            .operation
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let mut name = text("summary")
        .or_else(|| text("operationId"))
        .unwrap_or_else(|| format!("{} {}", operation.method.as_str(), operation.path));
    if operation
        .operation
        .get("deprecated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        name.push_str(DEPRECATED_SUFFIX);
    }
    name
}

/// `200` → 200; `2XX` → 200, guessed; `default` → 500, guessed.
fn status_for(code: &str) -> Option<(u16, bool)> {
    if code == "default" {
        return Some((DEFAULT_RESPONSE_STATUS, true));
    }
    if let Ok(status) = code.parse::<u16>() {
        return (100..=599).contains(&status).then_some((status, false));
    }
    let bytes = code.as_bytes();
    match bytes {
        [digit @ b'1'..=b'5', b'X' | b'x', b'X' | b'x'] => {
            Some((u16::from(*digit - b'0') * 100, true))
        }
        _ => None,
    }
}

fn api_key_variable(scheme_name: &str) -> String {
    let trimmed = scheme_name.trim();
    if trimmed.is_empty() || trimmed.contains(['{', '}']) {
        API_KEY_VARIABLE.to_string()
    } else {
        trimmed.to_string()
    }
}

fn auth_uses(auth: &Auth, where_: ApiKeyLocation, name: &str) -> bool {
    matches!(auth, Auth::ApiKey { key, location, .. }
        if *location == where_ && key.eq_ignore_ascii_case(name))
}

/// `a=1&b=two` → rows, left as written.
fn parse_form(text: &str) -> Vec<KeyValue> {
    text.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((name, value)) => KeyValue::new(name, value),
            None => KeyValue::new(pair, ""),
        })
        .collect()
}

/// Multipart properties that are files. A file part needs a path on this
/// machine, which a document cannot supply.
fn file_properties(refs: Refs<'_>, schema: Option<&Value>) -> Vec<String> {
    let Some(properties) = schema
        .and_then(|schema| refs.resolve(schema))
        .and_then(|schema| schema.get("properties"))
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    properties
        .iter()
        .filter(|(_, property)| {
            refs.resolve(property).is_some_and(|property| {
                let format = property.get("format").and_then(Value::as_str);
                matches!(format, Some("binary" | "base64"))
                    || property.get("contentMediaType").is_some()
                    || property.get("contentEncoding").is_some()
                    || property
                        .get("items")
                        .and_then(|items| items.get("format"))
                        .and_then(Value::as_str)
                        == Some("binary")
            })
        })
        .map(|(name, _)| name.clone())
        .collect()
}

#[cfg(test)]
#[path = "to_collection_tests.rs"]
mod tests;
