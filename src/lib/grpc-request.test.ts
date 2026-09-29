import { describe, expect, it } from "vitest";

import {
  clientStreams,
  emptyGrpcDraft,
  findMethod,
  grpcDraftsEqual,
  schemaRefFor,
  serverStreams,
} from "@/lib/grpc-request";
import type { GrpcRequestDraft, ProtoSchema } from "@/types/grpc";

const schema: ProtoSchema = {
  id: "proto_1",
  name: "Shop",
  origin: "import",
  files: ["shop.proto"],
  services: [
    {
      name: "shop.v1.Shop",
      methods: [
        {
          name: "GetOrder",
          path: "/shop.v1.Shop/GetOrder",
          kind: "unary",
          inputType: "shop.v1.GetOrderRequest",
          outputType: "shop.v1.Order",
        },
        {
          name: "Chat",
          path: "/shop.v1.Shop/Chat",
          kind: "bidirectional",
          inputType: "shop.v1.Line",
          outputType: "shop.v1.Line",
        },
      ],
    },
  ],
};

describe("grpcDraftsEqual", () => {
  const base = emptyGrpcDraft();
  const changes: Array<[string, Partial<GrpcRequestDraft>]> = [
    ["url", { url: "a:1" }],
    ["tls", { tls: false }],
    ["method", { methodPath: "/x.Y/Z" }],
    ["schema kind", { schema: { kind: "reflection" } }],
    ["metadata", { metadata: [{ name: "k", value: "v" }] }],
    ["auth", { auth: { kind: "bearer", token: "t" } }],
    ["message", { message: "{}" }],
    ["settings", { settings: { ...base.settings, deadlineMs: 1000 } }],
  ];

  it("is true for equal drafts", () => {
    expect(grpcDraftsEqual(emptyGrpcDraft(), emptyGrpcDraft())).toBe(true);
  });

  it.each(changes)("notices a change to the %s", (_, patch) => {
    expect(grpcDraftsEqual(base, { ...base, ...patch })).toBe(false);
  });

  it("compares a library reference by its id and auth by every field", () => {
    const a = { ...base, schema: { kind: "library" as const, schemaId: "proto_1" } };
    expect(grpcDraftsEqual(a, { ...a, schema: { kind: "library", schemaId: "proto_2" } })).toBe(
      false,
    );
    const bearer = { ...base, auth: { kind: "bearer" as const, token: "a" } };
    expect(grpcDraftsEqual(bearer, { ...bearer, auth: { kind: "bearer", token: "b" } })).toBe(
      false,
    );
  });
});

describe("schemaRefFor", () => {
  it("asks a server again for an unsaved reflected schema, and names anything else by id", () => {
    expect(schemaRefFor({ ...schema, origin: "reflection", name: null })).toEqual({
      kind: "reflection",
    });
    expect(schemaRefFor({ ...schema, origin: "reflection", name: "Saved" })).toEqual({
      kind: "library",
      schemaId: "proto_1",
    });
    expect(schemaRefFor({ ...schema, name: null })).toEqual({
      kind: "library",
      schemaId: "proto_1",
    });
  });
});

describe("findMethod and the streaming questions", () => {
  it("finds a method by path, and nothing for an unknown path or no schema", () => {
    expect(findMethod(schema, "/shop.v1.Shop/Chat")?.kind).toBe("bidirectional");
    expect(findMethod(schema, "/shop.v1.Shop/Nope")).toBeNull();
    expect(findMethod(null, "/shop.v1.Shop/Chat")).toBeNull();
    expect(findMethod(schema, "")).toBeNull();
  });

  it("says which side streams for each kind", () => {
    expect([clientStreams("unary"), serverStreams("unary")]).toEqual([false, false]);
    expect([clientStreams("serverStreaming"), serverStreams("serverStreaming")]).toEqual([
      false,
      true,
    ]);
    expect([clientStreams("clientStreaming"), serverStreams("clientStreaming")]).toEqual([
      true,
      false,
    ]);
    expect([clientStreams("bidirectional"), serverStreams("bidirectional")]).toEqual([true, true]);
  });
});
