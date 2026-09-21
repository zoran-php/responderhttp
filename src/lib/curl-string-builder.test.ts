// http_client/src/lib/curl-string-builder.test.ts
import { describe, expect, it } from "vitest";

import { buildCurlCommand } from "@/lib/curl-string-builder";
import { DEFAULT_SETTINGS, type SendRequestInput } from "@/types/http";

function input(overrides: Partial<SendRequestInput> = {}): SendRequestInput {
  return {
    method: "GET",
    url: "https://example.com/things",
    headers: [],
    queryParams: [],
    body: { kind: "none" },
    auth: { kind: "none" },
    settings: DEFAULT_SETTINGS,
    ...overrides,
  };
}

describe("buildCurlCommand auth", () => {
  it("uses -u for basic rather than a pre-encoded header", () => {
    const command = buildCurlCommand(
      input({ auth: { kind: "basic", username: "user", password: "pa ss" } }),
    );
    expect(command).toContain("-u 'user:pa ss'");
  });

  it("sends a bearer token as an Authorization header", () => {
    const command = buildCurlCommand(input({ auth: { kind: "bearer", token: "abc123" } }));
    expect(command).toContain("-H 'Authorization: Bearer abc123'");
  });

  it("puts an API key bound to the query string into the URL", () => {
    const command = buildCurlCommand(
      input({ auth: { kind: "apiKey", key: "api_key", value: "s e c", location: "query" } }),
    );
    expect(command).toContain("'https://example.com/things?api_key=s%20e%20c'");
    expect(command).not.toContain("-H 'api_key");
  });

  it("omits the generated header when the Headers tab already sets that name", () => {
    const command = buildCurlCommand(
      input({
        headers: [{ name: "Authorization", value: "Bearer typed-by-hand" }],
        auth: { kind: "bearer", token: "from-the-auth-tab" },
      }),
    );
    expect(command).toContain("-H 'Authorization: Bearer typed-by-hand'");
    expect(command).not.toContain("from-the-auth-tab");
  });

  it("adds nothing for a scheme left blank", () => {
    const blank = buildCurlCommand(input({ auth: { kind: "bearer", token: "   " } }));
    expect(blank).toBe(buildCurlCommand(input()));
  });
});

describe("buildCurlCommand", () => {
  it("builds a minimal GET", () => {
    expect(buildCurlCommand(input())).toBe(
      "curl -X GET \\\n  'https://example.com/things' \\\n  -L \\\n  --max-redirs 10 \\\n  --max-time 30",
    );
  });

  it("appends query params to the URL", () => {
    const command = buildCurlCommand(
      input({ queryParams: [{ name: "q", value: "a b" }, { name: "", value: "skipped" }] }),
    );

    expect(command).toContain("'https://example.com/things?q=a%20b'");
    expect(command).not.toContain("skipped");
  });

  it("encodes a typed URL the way the request is sent", () => {
    const command = buildCurlCommand(input({ url: "https://example.com/s?q=a b&c=Köln&n=1+1" }));

    expect(command).toContain("'https://example.com/s?q=a%20b&c=K%C3%B6ln&n=1+1'");
  });

  it("copies the URL as typed when automatic encoding is off", () => {
    const command = buildCurlCommand(
      input({
        url: "https://example.com/s?q=a b",
        settings: { ...DEFAULT_SETTINGS, encodeUrl: false },
      }),
    );

    expect(command).toContain("'https://example.com/s?q=a b'");
  });

  it("includes headers and a raw body with its content type", () => {
    const command = buildCurlCommand(
      input({
        method: "POST",
        headers: [{ name: "X-Api-Key", value: "secret" }],
        body: { kind: "raw", contentType: "application/json", text: '{"a":1}' },
      }),
    );

    expect(command).toContain("-H 'X-Api-Key: secret'");
    expect(command).toContain("-H 'Content-Type: application/json'");
    expect(command).toContain(`--data-raw '{"a":1}'`);
  });

  it("does not add a content type the user already set", () => {
    const command = buildCurlCommand(
      input({
        method: "POST",
        headers: [{ name: "content-type", value: "application/vnd.custom+json" }],
        body: { kind: "raw", contentType: "application/json", text: "{}" },
      }),
    );

    expect(command).toContain("application/vnd.custom+json");
    expect(command).not.toContain("-H 'Content-Type: application/json'");
  });

  it("uses the right flag per body type", () => {
    const form = buildCurlCommand(
      input({
        method: "POST",
        body: { kind: "formUrlEncoded", fields: [{ name: "a", value: "1" }] },
      }),
    );
    const multipart = buildCurlCommand(
      input({
        method: "POST",
        body: { kind: "multipart", parts: [{ kind: "text", name: "a", value: "1" }] },
      }),
    );

    expect(form).toContain("--data-urlencode 'a=1'");
    expect(multipart).toContain("-F 'a=1'");
  });

  it("escapes single quotes so the line stays pasteable", () => {
    const command = buildCurlCommand(
      input({ method: "POST", body: { kind: "raw", contentType: "text/plain", text: "it's" } }),
    );

    expect(command).toContain(`--data-raw 'it'\\''s'`);
  });

  it("reflects settings that differ from curl's own defaults", () => {
    const command = buildCurlCommand(
      input({
        settings: {
          ...DEFAULT_SETTINGS,
          followRedirects: false,
          timeoutMs: 1500,
          verifyTls: false,
          proxy: "http://127.0.0.1:8080",
        },
      }),
    );

    expect(command).not.toContain("-L");
    expect(command).toContain("--max-time 1.5");
    expect(command).toContain("-k");
    expect(command).toContain("-x 'http://127.0.0.1:8080'");
  });

  /** A copied command has to upload the same file, not paste the path in as
   * text — that is what curl's @ syntax is for. */
  it("emits curl's @ syntax for a multipart file part", () => {
    const command = buildCurlCommand(
      input({
        method: "POST",
        body: {
          kind: "multipart",
          parts: [
            { kind: "file", name: "upload", path: "/tmp/a.png", contentType: "image/png" },
            { kind: "file", name: "raw", path: "/tmp/b.bin", contentType: null },
          ],
        },
      }),
    );

    expect(command).toContain("-F 'upload=@/tmp/a.png;type=image/png'");
    // No type= when we have no guess, so curl falls back to its own.
    expect(command).toContain("-F 'raw=@/tmp/b.bin'");
  });

  it("emits the transport settings curl has flags for", () => {
    const command = buildCurlCommand(
      input({
        settings: {
          ...DEFAULT_SETTINGS,
          httpVersion: "http11",
          tlsMinimum: "tls12",
          allowHttp09: true,
          keepMethodOnRedirect: true,
          keepAuthOnRedirect: true,
        },
      }),
    );

    expect(command).toContain("--http1.1");
    expect(command).toContain("--tlsv1.2");
    expect(command).toContain("--http0.9");
    expect(command).toContain("--post301");
    expect(command).toContain("--location-trusted");
  });

  /** curl already downgrades to GET and already drops credentials across
   * hosts, so emitting these when they are off would be noise that suggests
   * the opposite of what is configured. */
  it("says nothing about redirects when the defaults already match curl's", () => {
    const command = buildCurlCommand(input({ settings: { ...DEFAULT_SETTINGS } }));

    expect(command).not.toContain("--post301");
    expect(command).not.toContain("--location-trusted");
    expect(command).not.toContain("--http1.1");
    expect(command).not.toContain("--tlsv1");
    expect(command).not.toContain("--http0.9");
  });
});
