-- 0011_grpc.sql
--
-- gRPC requests (PLAN-GRPC.md 16g). Never edit this file once shipped; add a
-- new numbered one (CLAUDE.md section 11, rule 5).
--
-- `proto_schemas` is the schema library (decision D3): the schemas gRPC
-- requests are made against, whether imported from .proto files or asked of
-- a server by reflection. Requests refer to one by id, so a schema shared by
-- every method of a service is stored once.
--
-- - `origin` is 'import' or 'reflection', written only from the closed Rust
--   enum SchemaOrigin (persistence/repositories/proto_schemas.rs).
-- - `descriptor_set` is a serialized google.protobuf.FileDescriptorSet with
--   every import included: enough to load the schema again with no source
--   file and no disk.
-- - `sources_json` keeps an import's source text by import name, for viewing
--   and re-importing: {"common/money.proto": "syntax = ..."}. '{}' for a
--   reflected schema, which arrives as descriptors.
--
-- A schema holds no secrets.
CREATE TABLE proto_schemas (
    id             TEXT PRIMARY KEY,
    name           TEXT NOT NULL,
    origin         TEXT NOT NULL,
    descriptor_set BLOB NOT NULL,
    sources_json   TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);

-- gRPC requests live in `requests`, as WebSocket ones do (0010), so
-- collections, folders, rename, move, delete, docs and the cascades work on
-- them unchanged. `kind` gains the value 'grpc'. The target stays in `url`,
-- metadata in `headers_json` and auth in `auth_json`, so secrets are sealed
-- by the existing path (CLAUDE.md section 5). What only a gRPC request has
-- (the method path, the schema reference, the draft message and the
-- settings) goes in `grpc_json`. Empty for every other kind.
ALTER TABLE requests ADD COLUMN grpc_json TEXT NOT NULL DEFAULT '';
