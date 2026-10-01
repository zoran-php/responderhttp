// http_client/src-tauri/src/proto/catalog.rs
//
// What the method picker shows: every service, every method, its kind and
// its message types. Read from a descriptor pool, whichever way the pool was
// made (an import or server reflection).
use prost_reflect::{DescriptorPool, MethodDescriptor};

use super::ProtoError;

/// Decided by the two `stream` keywords of the `rpc` line. It sets what the
/// UI offers: Invoke alone, or Send and End Streaming as well.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    Unary,
    ServerStreaming,
    ClientStreaming,
    Bidirectional,
}

impl MethodKind {
    pub fn of(method: &MethodDescriptor) -> Self {
        match (method.is_client_streaming(), method.is_server_streaming()) {
            (false, false) => Self::Unary,
            (false, true) => Self::ServerStreaming,
            (true, false) => Self::ClientStreaming,
            (true, true) => Self::Bidirectional,
        }
    }

    /// Whether the user sends messages one by one and ends the stream
    /// themselves, rather than one message sent with Invoke.
    pub fn client_streams(self) -> bool {
        matches!(self, Self::ClientStreaming | Self::Bidirectional)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodInfo {
    pub name: String,
    /// The HTTP/2 `:path`: `/demo.v1.Shop/Get`. Also how a saved request
    /// remembers its method.
    pub path: String,
    pub kind: MethodKind,
    pub input_type: String,
    pub output_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceInfo {
    /// Fully qualified: `demo.v1.Shop`.
    pub name: String,
    pub methods: Vec<MethodInfo>,
}

pub fn method_path(method: &MethodDescriptor) -> String {
    format!("/{}/{}", method.parent_service().full_name(), method.name())
}

/// Services sorted by name, methods in the order the schema declares them,
/// which is the order their authors chose to present them in.
pub fn catalog(pool: &DescriptorPool) -> Vec<ServiceInfo> {
    let mut services: Vec<ServiceInfo> = pool
        .services()
        .map(|service| ServiceInfo {
            name: service.full_name().to_string(),
            methods: service
                .methods()
                .map(|method| MethodInfo {
                    name: method.name().to_string(),
                    path: method_path(&method),
                    kind: MethodKind::of(&method),
                    input_type: method.input().full_name().to_string(),
                    output_type: method.output().full_name().to_string(),
                })
                .collect(),
        })
        .collect();
    services.sort_by(|a, b| a.name.cmp(&b.name));
    services
}

/// The method a saved request or the picker refers to by its path.
pub fn find_method(pool: &DescriptorPool, path: &str) -> Result<MethodDescriptor, ProtoError> {
    let not_found = || ProtoError::NotInSchema(format!("the schema has no method {path}"));
    let (service, method) = path
        .strip_prefix('/')
        .and_then(|rest| rest.split_once('/'))
        .ok_or_else(|| {
            ProtoError::NotInSchema(format!(
                "{path:?} is not a method path like /package.Service/Method"
            ))
        })?;
    let service = pool.get_service_by_name(service).ok_or_else(not_found)?;
    let found = service
        .methods()
        .find(|candidate| candidate.name() == method);
    found.ok_or_else(not_found)
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;

    #[test]
    fn every_method_is_listed_with_its_kind_and_path() {
        let services = catalog(&fixture::shop().pool);

        assert_eq!(services.len(), 1);
        let shop = &services[0];
        assert_eq!(shop.name, "demo.v1.Shop");
        let summary: Vec<(&str, &str, MethodKind)> = shop
            .methods
            .iter()
            .map(|m| (m.name.as_str(), m.path.as_str(), m.kind))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("Get", "/demo.v1.Shop/Get", MethodKind::Unary),
                ("Watch", "/demo.v1.Shop/Watch", MethodKind::ServerStreaming),
                (
                    "Upload",
                    "/demo.v1.Shop/Upload",
                    MethodKind::ClientStreaming
                ),
                ("Chat", "/demo.v1.Shop/Chat", MethodKind::Bidirectional),
            ]
        );
        assert_eq!(shop.methods[0].input_type, "demo.v1.GetRequest");
        assert_eq!(shop.methods[0].output_type, "demo.v1.Item");
    }

    #[test]
    fn only_the_two_client_streaming_kinds_stream_from_the_client() {
        assert!(!MethodKind::Unary.client_streams());
        assert!(!MethodKind::ServerStreaming.client_streams());
        assert!(MethodKind::ClientStreaming.client_streams());
        assert!(MethodKind::Bidirectional.client_streams());
    }

    #[test]
    fn a_method_is_found_by_its_path() {
        let pool = fixture::shop().pool;

        let method = find_method(&pool, "/demo.v1.Shop/Chat").expect("found");

        assert_eq!(MethodKind::of(&method), MethodKind::Bidirectional);
    }

    #[test]
    fn unknown_or_malformed_paths_are_refused() {
        let pool = fixture::shop().pool;

        for path in [
            "/demo.v1.Shop/Nope",
            "/demo.v1.Nope/Get",
            "demo.v1.Shop/Get",
            "/demo.v1.Shop",
            "",
        ] {
            assert!(
                matches!(find_method(&pool, path), Err(ProtoError::NotInSchema(_))),
                "{path:?}"
            );
        }
    }
}
