// Tests for to_collection.rs, kept in their own file because the mapping
// table they cover is long.
use serde_json::json;

use super::*;

fn options() -> ImportOptions {
    ImportOptions {
        grouping: Grouping::Tags,
        include_examples: true,
        create_environment: true,
    }
}

fn doc(paths: Value) -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {"title": "Pets", "version": "1"},
        "servers": [{"url": "https://api.example.com/v1/"}],
        "paths": paths
    })
}

fn map(document: &Value) -> Mapped {
    to_plan(document, &options())
}

fn only_request(mapped: &Mapped) -> &PlannedRequest {
    assert_eq!(mapped.plan.requests.len(), 1, "{:?}", mapped.plan.requests);
    &mapped.plan.requests[0]
}

fn variable<'a>(mapped: &'a Mapped, name: &str) -> Option<&'a EnvironmentVariable> {
    mapped
        .plan
        .environment
        .as_ref()?
        .variables
        .iter()
        .find(|variable| variable.name == name)
}

#[test]
fn operations_become_requests_named_by_summary_then_id_then_route() {
    let document = doc(json!({
        "/pets": {
            "get": {"summary": "List pets", "operationId": "listPets", "responses": {}},
            "post": {"operationId": "createPet", "responses": {}},
            "delete": {"summary": "  ", "responses": {}, "deprecated": true}
        }
    }));

    let mapped = map(&document);
    let names: Vec<(&str, HttpMethod)> = mapped
        .plan
        .requests
        .iter()
        .map(|r| (r.name.as_str(), r.request.method))
        .collect();

    assert_eq!(
        names,
        vec![
            ("List pets", HttpMethod::Get),
            ("createPet", HttpMethod::Post),
            ("DELETE /pets (deprecated)", HttpMethod::Delete),
        ]
    );
    assert_eq!(mapped.plan.collection_name, "Pets");
}

#[test]
fn unsupported_methods_are_reported_not_imported() {
    let document = json!({
        "openapi": "3.2.0",
        "info": {"title": "t", "version": "1"},
        "paths": {"/a": {
            "get": {"responses": {}},
            "trace": {"responses": {}},
            "query": {"responses": {}},
            "additionalOperations": {"COPY": {"responses": {}}}
        }}
    });

    let mapped = map(&document);

    assert_eq!(mapped.plan.requests.len(), 1);
    for method in ["TRACE", "QUERY", "COPY"] {
        assert!(
            mapped.notes.contains(&ImportNote::UnsupportedMethod {
                method: method.into(),
                path: "/a".into()
            }),
            "{method}: {:?}",
            mapped.notes
        );
    }
}

#[test]
fn the_first_server_becomes_base_url_in_a_new_environment() {
    let document = json!({
        "openapi": "3.1.0",
        "info": {"title": "t", "version": "1"},
        "servers": [
            {"url": "https://{region}.example.com/{version}", "variables": {
                "region": {"default": "eu"},
                "version": {"default": "v2"}
            }},
            {"url": "https://staging.example.com"}
        ],
        "paths": {"/users/{id}": {"get": {"responses": {}}}}
    });

    let mapped = map(&document);

    assert_eq!(
        only_request(&mapped).request.url,
        "{{baseUrl}}/users/{{id}}"
    );
    assert_eq!(
        variable(&mapped, "baseUrl").map(|v| v.value.as_str()),
        Some("https://eu.example.com/v2")
    );
    assert!(mapped.notes.contains(&ImportNote::OtherServersIgnored {
        urls: vec!["https://staging.example.com".into()]
    }));
    let environment = mapped.plan.environment.as_ref().unwrap();
    assert_eq!(environment.name, "t");
}

#[test]
fn without_an_environment_urls_are_written_out() {
    let document = doc(json!({"/pets/{id}": {"get": {"responses": {}}}}));

    let mapped = to_plan(
        &document,
        &ImportOptions {
            create_environment: false,
            ..options()
        },
    );

    assert_eq!(
        only_request(&mapped).request.url,
        "https://api.example.com/v1/pets/{{id}}"
    );
    assert!(mapped.plan.environment.is_none());
}

#[test]
fn a_server_that_is_one_variable_keeps_its_name() {
    let document = json!({
        "openapi": "3.1.0",
        "info": {"title": "t", "version": "1"},
        "servers": [{"url": "{base_url}", "variables": {"base_url": {"default": "<base_url>"}}}],
        "paths": {"/a": {"get": {"responses": {}}}}
    });

    let mapped = map(&document);

    assert_eq!(only_request(&mapped).request.url, "{{base_url}}/a");
    assert_eq!(
        variable(&mapped, "base_url").map(|v| v.value.as_str()),
        Some("")
    );
    assert!(variable(&mapped, "baseUrl").is_none());
}

#[test]
fn no_servers_and_relative_servers_are_reported() {
    let mut no_server = doc(json!({"/a": {"get": {"responses": {}}}}));
    no_server.as_object_mut().unwrap().remove("servers");
    let mut relative = no_server.clone();
    relative["servers"] = json!([{"url": "/api/v3"}]);

    let mapped = map(&no_server);
    assert!(mapped.notes.contains(&ImportNote::NoServer));
    assert_eq!(only_request(&mapped).request.url, "{{baseUrl}}/a");

    let mapped = map(&relative);
    assert!(mapped.notes.contains(&ImportNote::RelativeServer {
        url: "/api/v3".into()
    }));
    assert_eq!(
        variable(&mapped, "baseUrl").map(|v| v.value.as_str()),
        Some("/api/v3")
    );
}

#[test]
fn a_path_or_operation_server_is_written_out_and_tallied() {
    let document = doc(json!({
        "/a": {
            "servers": [{"url": "https://files.example.com/"}],
            "get": {"summary": "Path server", "responses": {}},
            "put": {"summary": "Operation server", "servers": [{"url": "https://upload.example.com"}], "responses": {}}
        }
    }));

    let mapped = map(&document);
    let urls: Vec<&str> = mapped
        .plan
        .requests
        .iter()
        .map(|r| r.request.url.as_str())
        .collect();

    assert_eq!(
        urls,
        vec![
            "https://files.example.com/a",
            "https://upload.example.com/a"
        ]
    );
    assert!(mapped
        .notes
        .iter()
        .any(|note| matches!(note, ImportNote::OwnServer(Tally { count: 2, .. }))));
}

#[test]
fn path_parameters_become_environment_variables_with_their_examples() {
    let document = doc(json!({
        "/users/{userId}/orders/{orderId}": {
            "parameters": [{"name": "userId", "in": "path", "required": true, "example": 42, "schema": {"type": "integer"}}],
            "get": {
                "parameters": [{"name": "orderId", "in": "path", "required": true, "schema": {"type": "string"}}],
                "responses": {}
            }
        }
    }));

    let mapped = map(&document);

    assert_eq!(
        only_request(&mapped).request.url,
        "{{baseUrl}}/users/{{userId}}/orders/{{orderId}}"
    );
    assert_eq!(
        variable(&mapped, "userId").map(|v| v.value.as_str()),
        Some("42")
    );
    assert_eq!(
        variable(&mapped, "orderId").map(|v| v.value.as_str()),
        Some("")
    );
    assert!(!variable(&mapped, "userId").unwrap().secret);
}

#[test]
fn query_parameters_follow_the_inclusion_rule() {
    let document = doc(json!({"/search": {"get": {
        "parameters": [
            {"name": "q", "in": "query", "required": true, "schema": {"type": "string"}},
            {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 20}},
            {"name": "sort", "in": "query", "schema": {"type": "string", "enum": ["asc", "desc"]}},
            {"name": "tags", "in": "query", "example": ["a b", "c&d"]},
            {"name": "verbose", "in": "query", "schema": {"type": "boolean"}},
            {"name": "api_key", "in": "query", "example": "leaked-in-the-docs"},
            {"name": "filter", "in": "query", "examples": {"one": {"value": "x=1"}}}
        ],
        "responses": {}
    }}}));

    let mapped = map(&document);
    let request = only_request(&mapped);

    assert_eq!(
        request.request.url,
        "{{baseUrl}}/search?q={{q}}&limit=20&sort=asc&tags=a%20b,c%26d&api_key={{api_key}}&filter=x=1"
    );
    assert!(request.request.query_params.is_empty());
    assert!(variable(&mapped, "api_key").unwrap().secret);
    assert_eq!(variable(&mapped, "api_key").unwrap().value, "");
    assert!(mapped.notes.iter().any(|note| matches!(
        note,
        ImportNote::OptionalParametersLeftOut(Tally { count: 1, .. })
    )));
}

#[test]
fn operation_parameters_override_path_item_ones() {
    let document = doc(json!({"/a": {
        "parameters": [{"name": "v", "in": "query", "example": "path"}],
        "get": {"parameters": [{"name": "v", "in": "query", "example": "operation"}], "responses": {}}
    }}));

    assert_eq!(
        only_request(&map(&document)).request.url,
        "{{baseUrl}}/a?v=operation"
    );
}

#[test]
fn referenced_parameters_are_resolved() {
    let mut document = doc(json!({"/a": {"get": {
        "parameters": [{"$ref": "#/components/parameters/Page"}],
        "responses": {}
    }}}));
    document["components"] =
        json!({"parameters": {"Page": {"name": "page", "in": "query", "example": 3}}});

    assert_eq!(
        only_request(&map(&document)).request.url,
        "{{baseUrl}}/a?page=3"
    );
}

#[test]
fn headers_follow_the_same_rule_and_skip_what_the_spec_ignores() {
    let document = doc(json!({"/a": {"get": {
        "parameters": [
            {"name": "X-Request-Id", "in": "header", "required": true},
            {"name": "X-Trace", "in": "header", "example": "abc"},
            {"name": "X-Optional", "in": "header"},
            {"name": "Accept", "in": "header", "example": "text/plain"},
            {"name": "Authorization", "in": "header", "example": "Bearer real"},
            {"name": "Cookie", "in": "header", "example": "session=real"},
            {"name": "session", "in": "cookie"}
        ],
        "responses": {}
    }}}));

    let mapped = map(&document);
    let headers: Vec<(&str, &str)> = only_request(&mapped)
        .request
        .headers
        .iter()
        .map(|h| (h.name.as_str(), h.value.as_str()))
        .collect();

    assert_eq!(
        headers,
        vec![
            ("X-Request-Id", "{{X-Request-Id}}"),
            ("X-Trace", "abc"),
            ("Cookie", "{{Cookie}}")
        ]
    );
    assert!(variable(&mapped, "Cookie").unwrap().secret);
    assert!(mapped
        .notes
        .iter()
        .any(|note| matches!(note, ImportNote::CookieParameters(_))));
}

#[test]
fn a_json_body_uses_the_example_then_the_schema() {
    let mut document = doc(json!({
        "/with-example": {"post": {"requestBody": {"content": {"application/json": {
            "example": {"name": "Rex"}
        }}}, "responses": {}}},
        "/with-schema": {"post": {"requestBody": {"$ref": "#/components/requestBodies/Pet"}, "responses": {}}},
        "/serialized": {"post": {"requestBody": {"content": {"application/json": {
            "examples": {"raw": {"serializedValue": "{\"a\": 1}"}}
        }}}, "responses": {}}}
    }));
    document["components"] = json!({
        "requestBodies": {"Pet": {"content": {"application/problem+json": {"schema": {
            "type": "object",
            "properties": {"name": {"type": "string"}, "age": {"type": "integer"}}
        }}}}}
    });

    let mapped = map(&document);
    let bodies: BTreeMap<&str, &RequestBody> = mapped
        .plan
        .requests
        .iter()
        .map(|r| (r.name.as_str(), &r.request.body))
        .collect();

    assert_eq!(
        bodies["POST /with-example"],
        &RequestBody::Raw {
            content_type: "application/json".into(),
            text: "{\n  \"name\": \"Rex\"\n}".into()
        }
    );
    assert_eq!(
        bodies["POST /with-schema"],
        &RequestBody::Raw {
            content_type: "application/problem+json".into(),
            text: "{\n  \"age\": 0,\n  \"name\": \"string\"\n}".into()
        }
    );
    assert_eq!(
        bodies["POST /serialized"],
        &RequestBody::Raw {
            content_type: "application/json".into(),
            text: "{\"a\": 1}".into()
        }
    );
}

#[test]
fn json_is_preferred_over_other_media_types() {
    let document = doc(json!({"/a": {"post": {"requestBody": {"content": {
        "application/xml": {"example": "<a/>"},
        "application/json": {"example": {"a": 1}},
        "application/x-www-form-urlencoded": {"example": {"a": 1}}
    }}, "responses": {}}}}));

    let mapped = map(&document);
    let RequestBody::Raw { content_type, .. } = &only_request(&mapped).request.body else {
        panic!("expected a raw body");
    };
    assert_eq!(content_type, "application/json");
}

#[test]
fn xml_and_text_take_string_examples_only() {
    let document = doc(json!({
        "/xml": {"post": {"requestBody": {"content": {"application/xml": {"example": "<pet/>"}}}, "responses": {}}},
        "/text": {"post": {"requestBody": {"content": {"text/plain; charset=utf-8": {"example": {"no": "serialisation"}}}}, "responses": {}}}
    }));

    let mapped = map(&document);

    assert_eq!(
        mapped.plan.requests[0].request.body,
        RequestBody::Raw {
            content_type: "text/plain; charset=utf-8".into(),
            text: String::new()
        }
    );
    assert_eq!(
        mapped.plan.requests[1].request.body,
        RequestBody::Raw {
            content_type: "application/xml".into(),
            text: "<pet/>".into()
        }
    );
}

#[test]
fn form_and_multipart_bodies_become_fields_without_file_parts() {
    let document = doc(json!({
        "/form": {"post": {"requestBody": {"content": {"application/x-www-form-urlencoded": {
            "schema": {"type": "object", "properties": {"user": {"type": "string", "example": "ann"}, "remember": {"type": "boolean"}}}
        }}}, "responses": {}}},
        "/form-string": {"post": {"requestBody": {"content": {"application/x-www-form-urlencoded": {
            "example": "a=1&b"
        }}}, "responses": {}}},
        "/upload": {"post": {"requestBody": {"content": {"multipart/form-data": {
            "schema": {"type": "object", "properties": {
                "title": {"type": "string"},
                "file": {"type": "string", "format": "binary"}
            }}
        }}}, "responses": {}}}
    }));

    let mapped = map(&document);
    let bodies: BTreeMap<&str, &RequestBody> = mapped
        .plan
        .requests
        .iter()
        .map(|r| (r.name.as_str(), &r.request.body))
        .collect();

    assert_eq!(
        bodies["POST /form"],
        &RequestBody::FormUrlEncoded(vec![
            KeyValue::new("remember", "true"),
            KeyValue::new("user", "ann")
        ])
    );
    assert_eq!(
        bodies["POST /form-string"],
        &RequestBody::FormUrlEncoded(vec![KeyValue::new("a", "1"), KeyValue::new("b", "")])
    );
    assert_eq!(
        bodies["POST /upload"],
        &RequestBody::Multipart(vec![MultipartPart::Text {
            name: "title".into(),
            value: "string".into()
        }])
    );
    assert!(mapped
        .notes
        .iter()
        .any(|note| matches!(note, ImportNote::FilePartsLeftOut(Tally { count: 1, .. }))));
}

#[test]
fn an_unrepresentable_body_is_reported_per_media_type() {
    let document = doc(json!({
        "/a": {"put": {"requestBody": {"content": {"application/octet-stream": {}}}, "responses": {}}},
        "/b": {"put": {"requestBody": {"content": {"application/octet-stream": {}}}, "responses": {}}}
    }));

    let mapped = map(&document);

    assert!(mapped
        .plan
        .requests
        .iter()
        .all(|r| r.request.body == RequestBody::None));
    assert!(mapped.notes.contains(&ImportNote::UnsupportedBody {
        media_type: "application/octet-stream".into(),
        requests: Tally {
            count: 2,
            sample: vec!["PUT /a".into(), "PUT /b".into()]
        }
    }));
}

fn with_security(scheme: Value, security: Value) -> Value {
    let mut document = doc(json!({"/a": {"get": {"responses": {}}}}));
    document["components"] = json!({"securitySchemes": {"main": scheme}});
    document["security"] = security;
    document
}

#[test]
fn supported_schemes_become_auth_with_secret_placeholders() {
    let cases = [
        (
            json!({"type": "http", "scheme": "Bearer"}),
            Auth::Bearer {
                token: "{{token}}".into(),
            },
            vec!["token"],
        ),
        (
            json!({"type": "http", "scheme": "basic"}),
            Auth::Basic {
                username: "{{username}}".into(),
                password: "{{password}}".into(),
            },
            vec!["password"],
        ),
        (
            json!({"type": "apiKey", "in": "header", "name": "X-API-Key"}),
            Auth::ApiKey {
                key: "X-API-Key".into(),
                value: "{{main}}".into(),
                location: ApiKeyLocation::Header,
            },
            vec!["main"],
        ),
        (
            json!({"type": "apiKey", "in": "query", "name": "key"}),
            Auth::ApiKey {
                key: "key".into(),
                value: "{{main}}".into(),
                location: ApiKeyLocation::Query,
            },
            vec!["main"],
        ),
    ];

    for (scheme, expected, secrets) in cases {
        let mapped = map(&with_security(scheme, json!([{"main": []}])));

        assert_eq!(only_request(&mapped).request.auth, expected);
        for name in secrets {
            let variable = variable(&mapped, name).expect("variable");
            assert!(variable.secret, "{name}");
            assert_eq!(variable.value, "");
        }
    }
}

#[test]
fn operation_security_overrides_the_document_and_empty_means_none() {
    let mut document = with_security(
        json!({"type": "http", "scheme": "bearer"}),
        json!([{"main": []}]),
    );
    document["paths"]["/a"]["get"]["security"] = json!([]);
    assert_eq!(only_request(&map(&document)).request.auth, Auth::None);

    document["paths"]["/a"]["get"]["security"] = json!([{}, {"main": []}]);
    assert_eq!(only_request(&map(&document)).request.auth, Auth::None);
}

#[test]
fn unsupported_and_unknown_schemes_are_reported() {
    let oauth = map(&with_security(
        json!({"type": "oauth2", "flows": {}}),
        json!([{"main": ["read"]}]),
    ));
    assert_eq!(only_request(&oauth).request.auth, Auth::None);
    assert!(oauth.notes.iter().any(|note| matches!(
        note,
        ImportNote::UnsupportedSecurity { scheme, kind, .. } if scheme == "main" && kind == "oauth2"
    )));

    let cookie = map(&with_security(
        json!({"type": "apiKey", "in": "cookie", "name": "sid"}),
        json!([{"main": []}]),
    ));
    assert!(cookie.notes.iter().any(|note| matches!(
        note,
        ImportNote::UnsupportedSecurity { kind, .. } if kind == "apiKey in cookie"
    )));

    let unknown = map(&with_security(
        json!({"type": "http", "scheme": "bearer"}),
        json!([{"other": []}]),
    ));
    assert!(unknown.notes.iter().any(|note| matches!(
        note,
        ImportNote::UnknownSecurityScheme { scheme, .. } if scheme == "other"
    )));
}

#[test]
fn a_combined_requirement_uses_the_first_scheme_and_says_so() {
    let mut document = with_security(
        json!({"type": "http", "scheme": "bearer"}),
        json!([{"main": [], "second": []}]),
    );
    document["components"]["securitySchemes"]["second"] =
        json!({"type": "apiKey", "in": "header", "name": "X-Key"});

    let mapped = map(&document);

    assert!(matches!(
        only_request(&mapped).request.auth,
        Auth::Bearer { .. }
    ));
    assert!(mapped
        .notes
        .iter()
        .any(|note| matches!(note, ImportNote::CombinedSecurity(_))));
}

#[test]
fn a_parameter_that_duplicates_the_api_key_is_not_repeated() {
    let mut document = with_security(
        json!({"type": "apiKey", "in": "header", "name": "X-API-Key"}),
        json!([{"main": []}]),
    );
    document["paths"]["/a"]["get"]["parameters"] =
        json!([{"name": "x-api-key", "in": "header", "required": true}]);

    assert!(only_request(&map(&document)).request.headers.is_empty());
}

#[test]
fn response_examples_become_saved_examples() {
    let mut document = doc(json!({"/pets": {"get": {"responses": {
        "200": {"description": "A list", "content": {"application/json": {"examples": {
            "two": {"summary": "Two pets", "value": [{"id": 1}, {"id": 2}]},
            "empty": {"value": []},
            "remote": {"externalValue": "https://example.com/pets.json"}
        }}}},
        "404": {"$ref": "#/components/responses/NotFound"},
        "4XX": {"description": "Client error", "content": {"text/plain": {"example": "bad"}}},
        "default": {"description": "No saved response"}
    }}}}));
    document["components"] = json!({"responses": {"NotFound": {
        "description": "Missing",
        "content": {"application/json": {"example": {"error": "not found"}}}
    }}});

    let mapped = map(&document);
    let examples: Vec<(&str, u16, &str)> = only_request(&mapped)
        .examples
        .iter()
        .map(|e| (e.name.as_str(), e.status, e.response_body.as_str()))
        .collect();

    assert_eq!(
        examples,
        vec![
            ("empty", 200, "[]"),
            (
                "Two pets",
                200,
                "[\n  {\n    \"id\": 1\n  },\n  {\n    \"id\": 2\n  }\n]"
            ),
            ("Missing", 404, "{\n  \"error\": \"not found\"\n}"),
            ("Client error", 400, "bad"),
        ]
    );
    assert_eq!(
        only_request(&mapped).examples[0].response_headers,
        vec![KeyValue::new("Content-Type", "application/json")]
    );
    assert!(mapped.notes.contains(&ImportNote::Ignored {
        feature: IgnoredFeature::ExternalExamples,
        count: 1
    }));
    assert!(mapped.notes.iter().any(|note| matches!(
        note,
        ImportNote::ExampleStatusGuessed(Tally { count: 1, .. })
    )));
}

#[test]
fn examples_can_be_left_out() {
    let document = doc(json!({"/a": {"get": {"responses": {
        "200": {"description": "ok", "content": {"application/json": {"example": {}}}}
    }}}}));

    let mapped = to_plan(
        &document,
        &ImportOptions {
            include_examples: false,
            ..options()
        },
    );

    assert!(only_request(&mapped).examples.is_empty());
}

#[test]
fn status_codes_map_exactly_by_range_or_by_default() {
    assert_eq!(status_for("201"), Some((201, false)));
    assert_eq!(status_for("5XX"), Some((500, true)));
    assert_eq!(status_for("2xx"), Some((200, true)));
    assert_eq!(status_for("default"), Some((500, true)));
    assert_eq!(status_for("600"), None);
    assert_eq!(status_for("6XX"), None);
    assert_eq!(status_for("abc"), None);
}

#[test]
fn folders_follow_the_chosen_grouping() {
    let document = json!({
        "openapi": "3.2.0",
        "info": {"title": "t", "version": "1"},
        "tags": [{"name": "store"}, {"name": "orders", "parent": "store"}],
        "paths": {
            "/store/orders": {"get": {"tags": ["orders", "store"], "responses": {}}},
            "/health": {"get": {"responses": {}}}
        }
    });
    let refs = Refs::new(&document);

    let by_tags = map(&document);
    assert_eq!(by_tags.plan.folders.len(), 2);
    assert_eq!(by_tags.plan.folders[1].parent, Some(0));
    let folders: Vec<Option<usize>> = by_tags.plan.requests.iter().map(|r| r.folder).collect();
    assert_eq!(folders, vec![None, Some(1)]);

    let by_paths = to_plan(
        &document,
        &ImportOptions {
            grouping: Grouping::Paths,
            ..options()
        },
    );
    assert_eq!(
        by_paths
            .plan
            .folders
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        vec!["health", "store/orders"]
    );

    assert_eq!(default_grouping(refs), Grouping::Tags);
    assert_eq!(
        arrangement_for(refs, Grouping::Tags).top_level_names(),
        vec!["store"]
    );
}

#[test]
fn ignored_features_are_counted() {
    let mut document = doc(json!({"/a": {"post": {
        "callbacks": {"onEvent": {}},
        "responses": {"201": {"description": "ok", "links": {"self": {"operationId": "x"}}}}
    }}}));
    document["webhooks"] = json!({"newPet": {}, "oldPet": {}});

    let mapped = map(&document);

    for (feature, count) in [
        (IgnoredFeature::Webhooks, 2),
        (IgnoredFeature::Callbacks, 1),
        (IgnoredFeature::Links, 1),
    ] {
        assert!(
            mapped
                .notes
                .contains(&ImportNote::Ignored { feature, count }),
            "{feature:?}: {:?}",
            mapped.notes
        );
    }
}

#[test]
fn a_path_item_reference_is_followed() {
    let document = json!({
        "openapi": "3.1.0",
        "info": {"title": "t", "version": "1"},
        "paths": {"/a": {"$ref": "#/components/pathItems/A"}},
        "components": {"pathItems": {"A": {"get": {"summary": "From a component", "responses": {}}}}}
    });

    assert_eq!(only_request(&map(&document)).name, "From a component");
}

#[test]
fn extensions_beside_paths_and_responses_are_not_operations() {
    let document = doc(json!({
        "x-internal": {"get": {"responses": {}}},
        "/a": {"get": {"responses": {
            "x-note": {"description": "not a status"},
            "204": {"description": "ok"}
        }}}
    }));

    let mapped = map(&document);

    assert_eq!(only_request(&mapped).request.url, "{{baseUrl}}/a");
    assert!(only_request(&mapped).examples.is_empty());
}

#[test]
fn a_missing_title_gets_a_fallback() {
    let mut document = doc(json!({}));
    document["info"]["title"] = json!("   ");

    assert_eq!(map(&document).plan.collection_name, FALLBACK_TITLE);
}

#[test]
fn tallies_keep_a_short_sample_of_names() {
    let mut tally = Tally::default();
    for name in ["a", "a", "b", "c", "d"] {
        tally.add(name);
    }

    assert_eq!(tally.count, 5);
    assert_eq!(tally.sample, vec!["a", "b", "c"]);
}

#[test]
fn templating_and_encoding_helpers() {
    assert_eq!(templated_path("/a/{b}/c{d}.json"), "/a/{{b}}/c{{d}}.json");
    assert_eq!(templated_path("a/{}"), "/a/{}");
    assert_eq!(templated_path("/unclosed/{x"), "/unclosed/{x");
    assert_eq!(
        encode_query_component("a b&c#d+e%f=g", false),
        "a%20b%26c%23d%2Be%25f=g"
    );
    assert_eq!(encode_query_component("n=1", true), "n%3D1");
    assert_eq!(value_text(&json!([1, "x", true])), "1,x,true");
    assert_eq!(value_text(&json!({"a": 1})), "{\"a\":1}");
    assert_eq!(value_text(&json!(null)), "");
}

// --- Item documentation (PLAN.md Phase 12) ---------------------------------

#[test]
fn an_operations_description_becomes_the_requests_documentation() {
    let document = doc(json!({
        "/pets": {
            "get": {
                "summary": "List pets",
                "description": "Returns every pet.\n\nPaginated with `?page`.",
                "responses": {"200": {"description": "ok"}}
            }
        }
    }));

    let mapped = map(&document);

    assert_eq!(
        only_request(&mapped).docs,
        "Returns every pet.\n\nPaginated with `?page`."
    );
    // The summary still names the request; the description is what sits under it.
    assert_eq!(only_request(&mapped).name, "List pets");
}

#[test]
fn an_operation_without_a_description_imports_undocumented() {
    let document = doc(json!({
        "/pets": {"get": {"summary": "List pets", "responses": {"200": {"description": "ok"}}}}
    }));

    assert_eq!(only_request(&map(&document)).docs, "");
}

#[test]
fn a_documents_description_becomes_the_collections_documentation() {
    let mut document = doc(json!({
        "/pets": {"get": {"responses": {"200": {"description": "ok"}}}}
    }));
    document["info"]["description"] = json!("# Pets API\n\nEverything about pets.");

    assert_eq!(
        map(&document).plan.collection_docs,
        "# Pets API\n\nEverything about pets."
    );
}

/// Exporting an undocumented collection writes a provenance sentence into
/// `info.description`. Importing that back must not leave it sitting in the
/// user's documentation as though they had written it.
#[test]
fn this_apps_own_provenance_sentence_is_not_imported_as_documentation() {
    let mut document = doc(json!({
        "/pets": {"get": {"responses": {"200": {"description": "ok"}}}}
    }));
    document["info"]["description"] = json!(
        crate::openapi::from_collection::generated_description("Pets")
    );

    assert_eq!(map(&document).plan.collection_docs, "");
}

#[test]
fn a_declared_tags_description_becomes_its_folders_documentation() {
    let mut document = doc(json!({
        "/pets": {
            "get": {"tags": ["pets"], "responses": {"200": {"description": "ok"}}}
        }
    }));
    document["tags"] = json!([{"name": "pets", "description": "Everything pet-shaped."}]);

    let mapped = map(&document);

    assert_eq!(mapped.plan.folders.len(), 1);
    assert_eq!(mapped.plan.folders[0].docs, "Everything pet-shaped.");
}

/// A tag used on an operation but never declared has a name and nothing else,
/// and a folder invented from a path segment has less than that.
#[test]
fn a_folder_with_nothing_to_describe_it_imports_undocumented() {
    let undeclared = doc(json!({
        "/pets": {"get": {"tags": ["pets"], "responses": {"200": {"description": "ok"}}}}
    }));
    let by_path = doc(json!({
        "/pets": {"get": {"responses": {"200": {"description": "ok"}}}}
    }));

    let tagged = map(&undeclared);
    assert_eq!(tagged.plan.folders[0].docs, "");

    let paths = to_plan(
        &by_path,
        &ImportOptions {
            grouping: Grouping::Paths,
            ..options()
        },
    );
    assert!(paths
        .plan
        .folders
        .iter()
        .all(|folder| folder.docs.is_empty()));
}

/// Markdown is whitespace sensitive, and the spec says a description may be
/// Markdown. Trimming it on the way in would edit what the author wrote.
#[test]
fn an_imported_description_keeps_its_whitespace() {
    let document = doc(json!({
        "/pets": {
            "get": {
                "description": "    indented code\n\nand a break  \n",
                "responses": {"200": {"description": "ok"}}
            }
        }
    }));

    assert_eq!(
        only_request(&map(&document)).docs,
        "    indented code\n\nand a break  \n"
    );
}
