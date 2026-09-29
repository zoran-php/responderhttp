// http_client/src-tauri/src/commands/grpc_dto.rs
//
// The gRPC wire format between Rust and TypeScript (PLAN-GRPC.md 16f),
// mirrored by src/types/grpc.ts, camelCase on both sides. Kept apart from
// dto.rs, which is long enough already; it reuses that file's KeyValueDto
// and AuthDto rather than defining them twice.
//
// Messages cross as JSON *text* (`json: String`), never as a parsed value,
// so a 64-bit integer is never touched by JavaScript's number type
// (PLAN-GRPC.md section 3, point 5). Durations cross as whole milliseconds.
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::commands::dto::{AuthDto, KeyValueDto, SecretStateDto};
use crate::domain::grpc_wire::GrpcStatus;
use crate::domain::models::{
    GrpcCallRequest, GrpcEndedBy, GrpcEvent, GrpcRequestDraft, GrpcSchemaRef, GrpcSettings,
    GrpcTarget, KeyValue, ProtoSchemaSummary, SavedGrpcRequest,
};
use crate::domain::services::grpc::GrpcCallOutcome;
use crate::domain::services::proto_schemas::{LoadedSchema, SchemaOrigin};
use crate::proto::catalog::{MethodInfo, MethodKind, ServiceInfo};

fn pairs_in(dtos: Vec<KeyValueDto>) -> Vec<KeyValue> {
    dtos.into_iter().map(KeyValue::from).collect()
}

fn pairs_out(pairs: Vec<KeyValue>) -> Vec<KeyValueDto> {
    pairs.into_iter().map(KeyValueDto::from).collect()
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcTargetDto {
    pub authority: String,
    pub tls: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcSettingsDto {
    pub verify_tls: bool,
    pub proxy: Option<String>,
    pub connect_timeout_ms: u64,
    /// None: no deadline.
    pub deadline_ms: Option<u64>,
    pub max_receive_bytes: u64,
    pub include_defaults: bool,
}

impl From<GrpcSettingsDto> for GrpcSettings {
    fn from(dto: GrpcSettingsDto) -> Self {
        Self {
            verify_tls: dto.verify_tls,
            proxy: dto.proxy,
            connect_timeout: Duration::from_millis(dto.connect_timeout_ms),
            deadline: dto.deadline_ms.map(Duration::from_millis),
            max_receive_bytes: usize::try_from(dto.max_receive_bytes).unwrap_or(usize::MAX),
            include_defaults: dto.include_defaults,
        }
    }
}

impl From<GrpcSettings> for GrpcSettingsDto {
    fn from(settings: GrpcSettings) -> Self {
        Self {
            verify_tls: settings.verify_tls,
            proxy: settings.proxy,
            connect_timeout_ms: millis(settings.connect_timeout),
            deadline_ms: settings.deadline.map(millis),
            max_receive_bytes: u64::try_from(settings.max_receive_bytes).unwrap_or(u64::MAX),
            include_defaults: settings.include_defaults,
        }
    }
}

/// Which schema a saved request uses. Tagged by `kind`, as AuthDto is.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GrpcSchemaRefDto {
    None,
    Reflection,
    Library {
        #[serde(rename = "schemaId")]
        schema_id: String,
    },
}

impl From<GrpcSchemaRefDto> for GrpcSchemaRef {
    fn from(dto: GrpcSchemaRefDto) -> Self {
        match dto {
            GrpcSchemaRefDto::None => Self::None,
            GrpcSchemaRefDto::Reflection => Self::Reflection,
            GrpcSchemaRefDto::Library { schema_id } => Self::Library(schema_id),
        }
    }
}

impl From<GrpcSchemaRef> for GrpcSchemaRefDto {
    fn from(schema: GrpcSchemaRef) -> Self {
        match schema {
            GrpcSchemaRef::None => Self::None,
            GrpcSchemaRef::Reflection => Self::Reflection,
            GrpcSchemaRef::Library(schema_id) => Self::Library { schema_id },
        }
    }
}

/// A gRPC tab's request, unresolved. Mirrored in src/types/grpc.ts.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcRequestDraftDto {
    pub url: String,
    pub tls: bool,
    pub method_path: String,
    pub schema: GrpcSchemaRefDto,
    pub metadata: Vec<KeyValueDto>,
    pub auth: AuthDto,
    pub message: String,
    pub settings: GrpcSettingsDto,
}

impl From<GrpcRequestDraftDto> for GrpcRequestDraft {
    fn from(dto: GrpcRequestDraftDto) -> Self {
        Self {
            url: dto.url,
            tls: dto.tls,
            method_path: dto.method_path,
            schema: dto.schema.into(),
            metadata: pairs_in(dto.metadata),
            auth: dto.auth.into(),
            message: dto.message,
            settings: dto.settings.into(),
        }
    }
}

impl From<GrpcRequestDraft> for GrpcRequestDraftDto {
    fn from(request: GrpcRequestDraft) -> Self {
        Self {
            url: request.url,
            tls: request.tls,
            method_path: request.method_path,
            schema: request.schema.into(),
            metadata: pairs_out(request.metadata),
            auth: request.auth.into(),
            message: request.message,
            settings: request.settings.into(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedGrpcRequestDto {
    pub id: String,
    pub collection_id: String,
    pub folder_id: Option<String>,
    pub name: String,
    pub request: GrpcRequestDraftDto,
    /// As for SavedRequestDto: when the secret did not open, the auth field
    /// is empty and the UI asks for it again.
    pub secret_state: SecretStateDto,
}

impl From<SavedGrpcRequest> for SavedGrpcRequestDto {
    fn from(saved: SavedGrpcRequest) -> Self {
        Self {
            id: saved.id,
            collection_id: saved.collection_id,
            folder_id: saved.folder_id,
            name: saved.name,
            request: saved.request.into(),
            secret_state: saved.secret_state.into(),
        }
    }
}

/// Grouped, as SaveWebSocketInput is.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveGrpcRequestInput {
    /// None saves a new gRPC request; an existing id overwrites it.
    pub id: Option<String>,
    pub collection_id: String,
    pub folder_id: Option<String>,
    pub name: String,
    pub request: GrpcRequestDraftDto,
}

/// What a call, or a reflection, is made with. `method_path` is ignored by
/// reflection, which asks its own questions.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcRequestInput {
    pub target: GrpcTargetDto,
    pub method_path: String,
    pub metadata: Vec<KeyValueDto>,
    pub auth: AuthDto,
    pub settings: GrpcSettingsDto,
}

impl From<GrpcRequestInput> for GrpcCallRequest {
    fn from(input: GrpcRequestInput) -> Self {
        Self {
            target: GrpcTarget {
                authority: input.target.authority,
                tls: input.target.tls,
            },
            path: input.method_path,
            metadata: pairs_in(input.metadata),
            auth: input.auth.into(),
            settings: input.settings.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MethodKindDto {
    Unary,
    ServerStreaming,
    ClientStreaming,
    Bidirectional,
}

impl From<MethodKind> for MethodKindDto {
    fn from(kind: MethodKind) -> Self {
        match kind {
            MethodKind::Unary => Self::Unary,
            MethodKind::ServerStreaming => Self::ServerStreaming,
            MethodKind::ClientStreaming => Self::ClientStreaming,
            MethodKind::Bidirectional => Self::Bidirectional,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcMethodDto {
    pub name: String,
    pub path: String,
    pub kind: MethodKindDto,
    pub input_type: String,
    pub output_type: String,
}

impl From<&MethodInfo> for GrpcMethodDto {
    fn from(method: &MethodInfo) -> Self {
        Self {
            name: method.name.clone(),
            path: method.path.clone(),
            kind: method.kind.into(),
            input_type: method.input_type.clone(),
            output_type: method.output_type.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcServiceDto {
    pub name: String,
    pub methods: Vec<GrpcMethodDto>,
}

impl From<&ServiceInfo> for GrpcServiceDto {
    fn from(service: &ServiceInfo) -> Self {
        Self {
            name: service.name.clone(),
            methods: service.methods.iter().map(GrpcMethodDto::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SchemaOriginDto {
    Import,
    Reflection,
}

impl From<SchemaOrigin> for SchemaOriginDto {
    fn from(origin: SchemaOrigin) -> Self {
        match origin {
            SchemaOrigin::Import => Self::Import,
            SchemaOrigin::Reflection => Self::Reflection,
        }
    }
}

/// One entry of the schema library, without its descriptors.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoSchemaSummaryDto {
    pub id: String,
    pub name: String,
    pub origin: SchemaOriginDto,
    pub updated_at: String,
}

impl From<ProtoSchemaSummary> for ProtoSchemaSummaryDto {
    fn from(summary: ProtoSchemaSummary) -> Self {
        Self {
            id: summary.id,
            name: summary.name,
            origin: summary.origin.into(),
            updated_at: summary.updated_at,
        }
    }
}

/// A loaded schema as the method picker needs it. The descriptors stay in
/// Rust; the UI refers to them by `id`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoSchemaDto {
    pub id: String,
    /// Its name in the library; None until saved.
    pub name: Option<String>,
    pub origin: SchemaOriginDto,
    pub services: Vec<GrpcServiceDto>,
    /// Import names of the files an import brought in, for the Service
    /// definition tab. Empty for a reflected schema.
    pub files: Vec<String>,
}

impl From<&LoadedSchema> for ProtoSchemaDto {
    fn from(schema: &LoadedSchema) -> Self {
        Self {
            id: schema.id.clone(),
            name: schema.name.clone(),
            origin: schema.origin.into(),
            services: schema.services.iter().map(GrpcServiceDto::from).collect(),
            files: schema.sources.names().map(str::to_string).collect(),
        }
    }
}

/// The name travels with the number, so the table of codes exists only in
/// Rust (CLAUDE.md section 7).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcStatusDto {
    pub code: u32,
    pub name: String,
    pub message: String,
}

impl From<GrpcStatus> for GrpcStatusDto {
    fn from(status: GrpcStatus) -> Self {
        Self {
            code: status.code.number(),
            name: status.code.name().to_string(),
            message: status.message,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GrpcEndedByDto {
    Server,
    Client,
}

impl From<GrpcEndedBy> for GrpcEndedByDto {
    fn from(by: GrpcEndedBy) -> Self {
        match by {
            GrpcEndedBy::Server => Self::Server,
            GrpcEndedBy::Client => Self::Client,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GrpcEventDto {
    #[serde(rename_all = "camelCase")]
    ResponseMetadata {
        at_ms: u64,
        metadata: Vec<KeyValueDto>,
    },
    #[serde(rename_all = "camelCase")]
    Sent {
        at_ms: u64,
        json: String,
        bytes: u64,
    },
    #[serde(rename_all = "camelCase")]
    Received {
        at_ms: u64,
        json: String,
        bytes: u64,
    },
    #[serde(rename_all = "camelCase")]
    StreamEnded { at_ms: u64 },
    #[serde(rename_all = "camelCase")]
    Ended {
        at_ms: u64,
        status: GrpcStatusDto,
        trailers: Vec<KeyValueDto>,
        by: GrpcEndedByDto,
        total_ms: u64,
    },
}

fn byte_count(bytes: usize) -> u64 {
    u64::try_from(bytes).unwrap_or(u64::MAX)
}

impl From<GrpcEvent> for GrpcEventDto {
    fn from(event: GrpcEvent) -> Self {
        match event {
            GrpcEvent::ResponseMetadata { at_ms, metadata } => Self::ResponseMetadata {
                at_ms,
                metadata: pairs_out(metadata),
            },
            GrpcEvent::Sent { at_ms, json, bytes } => Self::Sent {
                at_ms,
                json,
                bytes: byte_count(bytes),
            },
            GrpcEvent::Received { at_ms, json, bytes } => Self::Received {
                at_ms,
                json,
                bytes: byte_count(bytes),
            },
            GrpcEvent::StreamEnded { at_ms } => Self::StreamEnded { at_ms },
            GrpcEvent::Ended {
                at_ms,
                status,
                trailers,
                by,
                total_ms,
            } => Self::Ended {
                at_ms,
                status: status.into(),
                trailers: pairs_out(trailers),
                by: by.into(),
                total_ms,
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcCallOutcomeDto {
    pub status: GrpcStatusDto,
    pub by: GrpcEndedByDto,
    pub trailers: Vec<KeyValueDto>,
    pub total_ms: u64,
}

impl From<GrpcCallOutcome> for GrpcCallOutcomeDto {
    fn from(outcome: GrpcCallOutcome) -> Self {
        Self {
            status: outcome.status.into(),
            by: outcome.by.into(),
            trailers: pairs_out(outcome.trailers),
            total_ms: outcome.total_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::domain::grpc_wire::Code;

    #[test]
    fn events_cross_as_tagged_camel_case_with_messages_as_text() {
        let sent = serde_json::to_value(GrpcEventDto::from(GrpcEvent::Sent {
            at_ms: 5,
            json: "{\"big\": \"9007199254740993\"}".to_string(),
            bytes: 10,
        }))
        .expect("serializes");

        assert_eq!(
            sent,
            json!({ "type": "sent", "atMs": 5, "json": "{\"big\": \"9007199254740993\"}", "bytes": 10 })
        );
    }

    #[test]
    fn the_status_carries_its_name() {
        let ended = serde_json::to_value(GrpcEventDto::from(GrpcEvent::Ended {
            at_ms: 1,
            status: GrpcStatus::new(Code::DeadlineExceeded, "the 30 ms deadline passed"),
            trailers: Vec::new(),
            by: GrpcEndedBy::Client,
            total_ms: 31,
        }))
        .expect("serializes");

        assert_eq!(ended["type"], "ended");
        assert_eq!(
            ended["status"],
            json!({ "code": 4, "name": "DEADLINE_EXCEEDED", "message": "the 30 ms deadline passed" })
        );
        assert_eq!(ended["by"], "client");
        assert_eq!(ended["totalMs"], 31);
    }

    #[test]
    fn a_request_input_maps_to_the_domain_request() {
        let input: GrpcRequestInput = serde_json::from_value(json!({
            "target": { "authority": "localhost:50051", "tls": false },
            "methodPath": "/demo.v1.Shop/Get",
            "metadata": [{ "name": "x-id", "value": "1" }],
            "auth": { "kind": "bearer", "token": "t" },
            "settings": {
                "verifyTls": true, "proxy": null, "connectTimeoutMs": 30000,
                "deadlineMs": 5000, "maxReceiveBytes": 4194304, "includeDefaults": false
            }
        }))
        .expect("deserializes");

        let request = GrpcCallRequest::from(input);

        assert_eq!(request.path, "/demo.v1.Shop/Get");
        assert_eq!(request.target.authority, "localhost:50051");
        assert_eq!(request.settings.deadline, Some(Duration::from_millis(5000)));
        assert_eq!(request.metadata, vec![KeyValue::new("x-id", "1")]);
    }

    #[test]
    fn settings_survive_a_round_trip() {
        let settings = GrpcSettings {
            deadline: Some(Duration::from_millis(1500)),
            ..GrpcSettings::default()
        };

        let back = GrpcSettings::from(GrpcSettingsDto::from(settings.clone()));

        assert_eq!(back, settings);
    }

    #[test]
    fn method_kinds_are_camel_case() {
        assert_eq!(
            serde_json::to_value(MethodKindDto::ServerStreaming).expect("serializes"),
            "serverStreaming"
        );
    }

    /// The shape src/types/grpc.ts mirrors, both ways.
    #[test]
    fn a_schema_reference_crosses_tagged_by_kind() {
        let library = GrpcSchemaRefDto::from(GrpcSchemaRef::Library("proto_1".into()));
        assert_eq!(
            serde_json::to_value(&library).expect("serializes"),
            json!({ "kind": "library", "schemaId": "proto_1" })
        );
        let parsed: GrpcSchemaRefDto =
            serde_json::from_value(json!({ "kind": "reflection" })).expect("parses");
        assert_eq!(GrpcSchemaRef::from(parsed), GrpcSchemaRef::Reflection);
        let parsed: GrpcSchemaRefDto =
            serde_json::from_value(json!({ "kind": "none" })).expect("parses");
        assert_eq!(GrpcSchemaRef::from(parsed), GrpcSchemaRef::None);
    }
}
