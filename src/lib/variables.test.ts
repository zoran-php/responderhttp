// http_client/src/lib/variables.test.ts
import { describe, expect, it } from "vitest";

import {
  substitute,
  substituteMessage,
  substituteRequestInput,
  substituteWebSocketRequest,
  variableMap,
} from "@/lib/variables";
import { DEFAULT_WS_SETTINGS } from "@/types/websocket";
import { AUTH_NONE, DEFAULT_SETTINGS, type KeyValue, type SendRequestInput } from "@/types/http";

const vars: KeyValue[] = [
  { name: "base_url", value: "https://api.example.com" },
  { name: "token", value: "abc123" },
];

function input(overrides: Partial<SendRequestInput> = {}): SendRequestInput {
  return {
    method: "GET",
    url: "{{base_url}}/users",
    headers: [],
    queryParams: [],
    body: { kind: "none" },
    auth: AUTH_NONE,
    settings: DEFAULT_SETTINGS,
    ...overrides,
  };
}

describe("variableMap", () => {
  it("keeps the first definition when a name is repeated", () => {
    const map = variableMap([
      { name: "host", value: "first" },
      { name: "host", value: "second" },
    ]);
    expect(map.get("host")).toBe("first");
  });

  it("ignores rows with a blank name", () => {
    expect(variableMap([{ name: "   ", value: "x" }]).size).toBe(0);
  });
});

describe("substitute", () => {
  const map = variableMap(vars);

  it("replaces a known placeholder", () => {
    expect(substitute("{{base_url}}/users", map)).toBe("https://api.example.com/users");
  });

  it("leaves an unknown placeholder exactly as written", () => {
    expect(substitute("{{missing}}/users", map)).toBe("{{missing}}/users");
  });

  it("tolerates whitespace inside the braces", () => {
    expect(substitute("{{ token }}", map)).toBe("abc123");
  });

  it("replaces every occurrence, not just the first", () => {
    expect(substitute("{{token}}-{{token}}", map)).toBe("abc123-abc123");
  });

  it("leaves an unclosed placeholder alone", () => {
    expect(substitute("{{token", map)).toBe("{{token");
  });

  it("does not expand a placeholder that came from a value", () => {
    const nested = variableMap([
      { name: "outer", value: "{{inner}}" },
      { name: "inner", value: "resolved" },
    ]);
    // One pass only: a self-referencing variable cannot loop.
    expect(substitute("{{outer}}", nested)).toBe("{{inner}}");
  });
});

describe("substituteRequestInput", () => {
  it("returns the input untouched when there are no variables", () => {
    const original = input();
    expect(substituteRequestInput(original, [])).toBe(original);
  });

  it("resolves the url, headers, params, body and auth", () => {
    const resolved = substituteRequestInput(
      input({
        headers: [{ name: "X-{{token}}", value: "{{token}}" }],
        queryParams: [{ name: "q", value: "{{token}}" }],
        body: { kind: "raw", contentType: "application/json", text: '{"t":"{{token}}"}' },
        auth: { kind: "bearer", token: "{{token}}" },
      }),
      vars,
    );

    expect(resolved.url).toBe("https://api.example.com/users");
    expect(resolved.headers).toEqual([{ name: "X-abc123", value: "abc123" }]);
    expect(resolved.queryParams).toEqual([{ name: "q", value: "abc123" }]);
    expect(resolved.body).toEqual({
      kind: "raw",
      contentType: "application/json",
      text: '{"t":"abc123"}',
    });
    expect(resolved.auth).toEqual({ kind: "bearer", token: "abc123" });
  });

  it("does not mutate the input it was given", () => {
    const original = input({ auth: { kind: "bearer", token: "{{token}}" } });
    substituteRequestInput(original, vars);
    expect(original.url).toBe("{{base_url}}/users");
    expect(original.auth).toEqual({ kind: "bearer", token: "{{token}}" });
  });
});

describe("leaving secrets as placeholders", () => {
  const withSecret = [
    { name: "base_url", value: "https://api.example.com", secret: false },
    { name: "token", value: "live-token", secret: true },
  ];

  it("resolves everything by default, secrets included", () => {
    const resolved = substituteRequestInput(
      input({ auth: { kind: "bearer", token: "{{token}}" } }),
      withSecret,
    );

    expect(resolved.auth).toEqual({ kind: "bearer", token: "live-token" });
  });

  it("keeps a secret's placeholder and still resolves the rest", () => {
    const snapshot = substituteRequestInput(
      input({
        url: "{{base_url}}/users?key={{token}}",
        headers: [{ name: "Authorization", value: "Bearer {{token}}" }],
        auth: { kind: "bearer", token: "{{token}}" },
      }),
      withSecret,
      { leaveSecrets: true },
    );

    expect(snapshot.url).toBe("https://api.example.com/users?key={{token}}");
    expect(snapshot.headers).toEqual([{ name: "Authorization", value: "Bearer {{token}}" }]);
    expect(snapshot.auth).toEqual({ kind: "bearer", token: "{{token}}" });
    expect(JSON.stringify(snapshot)).not.toContain("live-token");
  });

  it("does not let a later plain duplicate stand in for a secret that won", () => {
    const map = variableMap(
      [
        { name: "token", value: "live-token", secret: true },
        { name: "token", value: "decoy", secret: false },
      ],
      { leaveSecrets: true },
    );

    expect(map.has("token")).toBe(false);
  });

  it("treats a plain key-value pair as not secret", () => {
    const map = variableMap([{ name: "host", value: "h" }], { leaveSecrets: true });

    expect(map.get("host")).toBe("h");
  });
});

describe("substituteWebSocketRequest and substituteMessage", () => {
  const variables = [
    { name: "host", value: "echo.test" },
    { name: "token", value: "s3cret", secret: true },
  ];
  const request = {
    url: "wss://{{host}}/live",
    headers: [{ name: "Authorization", value: "Bearer {{token}}" }],
    settings: DEFAULT_WS_SETTINGS,
  };

  it("resolves the URL and handshake headers, leaving unknown names as written", () => {
    const resolved = substituteWebSocketRequest(
      { ...request, url: "wss://{{host}}/{{missing}}" },
      variables,
    );

    expect(resolved.url).toBe("wss://echo.test/{{missing}}");
    expect(resolved.headers).toEqual([{ name: "Authorization", value: "Bearer s3cret" }]);
  });

  it("can leave secrets as placeholders, for the log", () => {
    const display = substituteWebSocketRequest(request, variables, { leaveSecrets: true });

    expect(display.url).toBe("wss://echo.test/live");
    expect(display.headers[0]?.value).toBe("Bearer {{token}}");
    expect(substituteMessage("{{token}}@{{host}}", variables, { leaveSecrets: true })).toBe(
      "{{token}}@echo.test",
    );
    expect(substituteMessage("{{token}}", variables)).toBe("s3cret");
  });
});
