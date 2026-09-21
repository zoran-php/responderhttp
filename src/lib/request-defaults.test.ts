// http_client/src/lib/request-defaults.test.ts
import { describe, expect, it } from "vitest";

import { emptyRequestInput, requestInputsEqual } from "@/lib/request-defaults";
import { AUTH_NONE, DEFAULT_SETTINGS, type SendRequestInput } from "@/types/http";

describe("emptyRequestInput", () => {
  it("is a GET with no body and the default settings", () => {
    expect(emptyRequestInput()).toEqual({
      method: "GET",
      url: "",
      headers: [],
      queryParams: [],
      body: { kind: "none" },
      auth: AUTH_NONE,
      settings: DEFAULT_SETTINGS,
    });
  });
});

describe("requestInputsEqual auth", () => {
  it("is false when only the auth scheme differs", () => {
    const bearer: SendRequestInput = {
      ...emptyRequestInput(),
      auth: { kind: "bearer", token: "abc" },
    };
    expect(requestInputsEqual(emptyRequestInput(), bearer)).toBe(false);
  });

  it("is false when only a credential inside the same scheme differs", () => {
    const one: SendRequestInput = {
      ...emptyRequestInput(),
      auth: { kind: "basic", username: "a", password: "one" },
    };
    const two: SendRequestInput = {
      ...emptyRequestInput(),
      auth: { kind: "basic", username: "a", password: "two" },
    };
    expect(requestInputsEqual(one, two)).toBe(false);
  });

  it("is false when only an API key's location differs", () => {
    const asHeader: SendRequestInput = {
      ...emptyRequestInput(),
      auth: { kind: "apiKey", key: "k", value: "v", location: "header" },
    };
    const asQuery: SendRequestInput = {
      ...emptyRequestInput(),
      auth: { kind: "apiKey", key: "k", value: "v", location: "query" },
    };
    expect(requestInputsEqual(asHeader, asQuery)).toBe(false);
  });
});

describe("requestInputsEqual", () => {
  it("is true for two separately built but identical inputs", () => {
    expect(requestInputsEqual(emptyRequestInput(), emptyRequestInput())).toBe(true);
  });

  it("is false when the url differs", () => {
    const a = emptyRequestInput();
    const b: SendRequestInput = { ...a, url: "https://example.com" };

    expect(requestInputsEqual(a, b)).toBe(false);
  });

  it("is false when a header value differs", () => {
    const a: SendRequestInput = {
      ...emptyRequestInput(),
      headers: [{ name: "X-Api-Key", value: "1" }],
    };
    const b: SendRequestInput = {
      ...emptyRequestInput(),
      headers: [{ name: "X-Api-Key", value: "2" }],
    };

    expect(requestInputsEqual(a, b)).toBe(false);
  });

  it("is false when the body kind differs", () => {
    const a = emptyRequestInput();
    const b: SendRequestInput = {
      ...a,
      body: { kind: "raw", contentType: "application/json", text: "{}" },
    };

    expect(requestInputsEqual(a, b)).toBe(false);
  });

  it("compares raw body fields, not just the kind", () => {
    const a: SendRequestInput = {
      ...emptyRequestInput(),
      body: { kind: "raw", contentType: "application/json", text: "{}" },
    };
    const sameText: SendRequestInput = { ...a, body: { ...a.body } };
    const differentText: SendRequestInput = {
      ...a,
      body: { kind: "raw", contentType: "application/json", text: '{"a":1}' },
    };

    expect(requestInputsEqual(a, sameText)).toBe(true);
    expect(requestInputsEqual(a, differentText)).toBe(false);
  });

  it("is false when a setting differs", () => {
    const a = emptyRequestInput();
    const b: SendRequestInput = { ...a, settings: { ...a.settings, verifyTls: false } };

    expect(requestInputsEqual(a, b)).toBe(false);
  });
});
