// http_client/src-tauri/src/domain/import_plan.rs
//
// Everything an import will create, built in memory first and written in one
// transaction (PLAN.md Phase 8d). The plan carries no ids: storage assigns
// them, so a plan can be previewed, discarded or rebuilt with other options
// without anything existing yet.
use crate::domain::models::{EnvironmentVariable, HttpRequest, KeyValue};

#[derive(Debug, Clone)]
pub struct ImportPlan {
    pub collection_name: String,
    /// Parents always come before their children, so the folders can be
    /// written in order.
    pub folders: Vec<PlannedFolder>,
    pub requests: Vec<PlannedRequest>,
    pub environment: Option<PlannedEnvironment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFolder {
    pub name: String,
    /// Index into `ImportPlan::folders`, always lower than this folder's own.
    pub parent: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct PlannedRequest {
    pub name: String,
    /// Index into `ImportPlan::folders`; None is the collection root.
    pub folder: Option<usize>,
    pub request: HttpRequest,
    pub examples: Vec<PlannedExample>,
}

/// A saved response taken from the document. Its request snapshot is the
/// planned request itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedExample {
    pub name: String,
    pub status: u16,
    pub response_headers: Vec<KeyValue>,
    pub response_body: String,
}

#[derive(Debug, Clone)]
pub struct PlannedEnvironment {
    pub name: String,
    pub variables: Vec<EnvironmentVariable>,
}

/// What storage created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedIds {
    pub collection_id: String,
    pub environment_id: Option<String>,
}

impl ImportPlan {
    pub fn example_count(&self) -> usize {
        self.requests
            .iter()
            .map(|request| request.examples.len())
            .sum()
    }
}
