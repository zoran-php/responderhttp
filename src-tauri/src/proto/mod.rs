// http_client/src-tauri/src/proto/mod.rs
//
// Protobuf schemas at runtime, for gRPC requests (PLAN-GRPC.md 16b). The
// user's `.proto` files are unknown until they are imported or fetched by
// server reflection, so nothing here is generated at build time: protox
// compiles sources in memory, prost-reflect describes and converts messages.
//
// A mapping layer over two libraries, beside `openapi/` for the same reason:
// pure, no I/O, no port. A trait in front of prost-reflect would have one
// implementation and no test seam (CLAUDE.md section 7).
//
// One rule runs through the whole module: a message crosses into and out of
// it as JSON *text*. The frontend never parses a message, because
// `JSON.parse` would round a 64-bit integer before Rust ever saw it.
pub mod catalog;
pub mod codec;
pub mod compile;
pub mod example;
pub mod reflection;

use crate::domain::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    /// The schema itself: a syntax error, a missing import, a type that
    /// does not resolve.
    #[error("{0}")]
    Schema(String),
    /// A message that does not fit its type, in either direction.
    #[error("{0}")]
    Message(String),
    /// A service, method or message the schema does not contain.
    #[error("{0}")]
    NotInSchema(String),
}

/// Every one of these is something the user can fix in what they supplied.
impl From<ProtoError> for AppError {
    fn from(error: ProtoError) -> Self {
        AppError::InvalidRequest(error.to_string())
    }
}

/// A small schema used by the tests of every submodule: two files, an
/// import, a well-known type, all four method kinds, and every field shape
/// the example generator has to handle.
#[cfg(test)]
pub(crate) mod fixture {
    use super::compile::{compile, CompiledSchema, SourceMap};

    pub const SHOP: &str = r#"
syntax = "proto3";
package demo.v1;

import "google/protobuf/timestamp.proto";
import "common/money.proto";

service Shop {
  rpc Get(GetRequest) returns (Item);
  rpc Watch(GetRequest) returns (stream Item);
  rpc Upload(stream Item) returns (Summary);
  rpc Chat(stream Item) returns (stream Item);
}

enum Kind {
  KIND_UNSPECIFIED = 0;
  KIND_BOOK = 1;
}

message GetRequest {
  string id = 1;
  int64 big = 2;
}

message Item {
  string name = 1;
  common.Money price = 2;
  repeated string tags = 3;
  map<string, int32> stock = 4;
  Kind kind = 5;
  oneof choice {
    string note = 6;
    int32 code = 7;
  }
  google.protobuf.Timestamp at = 8;
  bytes blob = 9;
  uint64 count = 10;
  Item child = 11;
  optional bool flag = 12;
  repeated common.Money history = 13;
}

message Summary {
  int32 received = 1;
}
"#;

    pub const MONEY: &str = r#"
syntax = "proto3";
package common;

message Money {
  string currency = 1;
  int64 units = 2;
}
"#;

    pub fn sources() -> SourceMap {
        let mut sources = SourceMap::default();
        sources.insert("shop/shop.proto", SHOP);
        sources.insert("common/money.proto", MONEY);
        sources
    }

    pub fn shop() -> CompiledSchema {
        compile(&sources(), &["shop/shop.proto".to_string()]).expect("the fixture compiles")
    }
}
