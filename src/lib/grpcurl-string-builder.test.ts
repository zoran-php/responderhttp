import { describe, expect, it } from "vitest";

import { authMetadata, buildGrpcurlCommand, type GrpcurlInput } from "@/lib/grpcurl-string-builder";
import { DEFAULT_GRPC_SETTINGS, type GrpcRequestInput } from "@/types/grpc";

const request: GrpcRequestInput = {
  target: { authority: "api.test:443", tls: true },
  methodPath: "/shop.v1.Shop/GetOrder",
  metadata: [],
  auth: { kind: "none" },
  settings: DEFAULT_GRPC_SETTINGS,
};

function build(overrides: Partial<GrpcurlInput> = {}): string {
  return buildGrpcurlCommand({ request, message: '{"id":"1"}', protoFiles: [], ...overrides });
}

function lines(command: string): string[] {
  return command.split(" \\\n  ");
}

describe("buildGrpcurlCommand", () => {
  it("ends with the target and the method, without the leading slash", () => {
    expect(lines(build())).toEqual([
      "grpcurl",
      `-d '{"id":"1"}'`,
      "-connect-timeout 30",
      "-max-msg-sz 4194304",
      "'api.test:443'",
      "'shop.v1.Shop/GetOrder'",
    ]);
  });

  it("follows the lock with -plaintext, and verification with -insecure", () => {
    const plain = build({
      request: { ...request, target: { authority: "localhost:50051", tls: false } },
    });
    expect(lines(plain)[1]).toBe("-plaintext");

    const insecure = build({
      request: { ...request, settings: { ...DEFAULT_GRPC_SETTINGS, verifyTls: false } },
    });
    expect(lines(insecure)[1]).toBe("-insecure");
  });

  it("does not add -insecure to a plaintext call, where there is no certificate", () => {
    const command = build({
      request: {
        ...request,
        target: { authority: "localhost:50051", tls: false },
        settings: { ...DEFAULT_GRPC_SETTINGS, verifyTls: false },
      },
    });
    expect(command).not.toContain("-insecure");
  });

  it("writes metadata and then auth as -H, skipping unnamed rows", () => {
    const command = build({
      request: {
        ...request,
        metadata: [
          { name: " x-tenant ", value: " acme " },
          { name: "", value: "ignored" },
        ],
        auth: { kind: "bearer", token: "tok" },
      },
    });
    expect(lines(command).slice(1, 3)).toEqual([
      "-H 'x-tenant: acme'",
      "-H 'authorization: Bearer tok'",
    ]);
    expect(command).not.toContain("ignored");
  });

  it("quotes a message with single quotes in it so it pastes into a shell", () => {
    const command = build({ message: `{"note":"it's"}` });
    expect(command).toContain(`-d '{"note":"it'\\''s"}'`);
  });

  it("sends {} for an empty message", () => {
    expect(build({ message: "  " })).toContain("-d '{}'");
  });

  it("adds a deadline, the size limit and emit-defaults from the settings", () => {
    const command = build({
      request: {
        ...request,
        settings: {
          ...DEFAULT_GRPC_SETTINGS,
          connectTimeoutMs: 1500,
          deadlineMs: 2500,
          maxReceiveBytes: 1024,
          includeDefaults: true,
        },
      },
    });
    expect(lines(command)).toEqual(
      expect.arrayContaining([
        "-connect-timeout 1.5",
        "-max-time 2.5",
        "-max-msg-sz 1024",
        "-emit-defaults",
      ]),
    );
  });

  it("names the schema's files with -proto when the schema was imported", () => {
    const command = build({ protoFiles: ["shop/v1/shop.proto", "money.proto"] });
    expect(lines(command).slice(1, 3)).toEqual([
      "-proto 'shop/v1/shop.proto'",
      "-proto 'money.proto'",
    ]);
  });
});

describe("authMetadata", () => {
  it("builds the same headers as the call does", () => {
    expect(authMetadata({ kind: "basic", username: "ana", password: "pässword" })).toEqual([
      { name: "authorization", value: "Basic YW5hOnDDpHNzd29yZA==" },
    ]);
    expect(
      authMetadata({ kind: "apiKey", key: " x-api-key ", value: "k", location: "header" }),
    ).toEqual([{ name: "x-api-key", value: "k" }]);
    expect(authMetadata({ kind: "custom", headerName: "x-auth", headerValue: "v" })).toEqual([
      { name: "x-auth", value: "v" },
    ]);
  });

  it("adds nothing for an untouched form or a query-string API key", () => {
    expect(authMetadata({ kind: "none" })).toEqual([]);
    expect(authMetadata({ kind: "basic", username: "", password: "" })).toEqual([]);
    expect(authMetadata({ kind: "bearer", token: "  " })).toEqual([]);
    expect(authMetadata({ kind: "apiKey", key: "k", value: "v", location: "query" })).toEqual([]);
    expect(authMetadata({ kind: "custom", headerName: " ", headerValue: "v" })).toEqual([]);
  });
});
