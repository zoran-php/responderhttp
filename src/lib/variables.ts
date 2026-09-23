// http_client/src/lib/variables.ts
//
// The {{variable}} substitution engine. Pure and tested here rather than
// inline in the store (CLAUDE.md section 6), and applied only on the way out
// — a saved request keeps its placeholders, so the same request can be sent
// against a different environment tomorrow.
import type { Auth, KeyValue, RequestBody, SendRequestInput } from "@/types/http";
import type { WebSocketRequest } from "@/types/websocket";

/** A binding as substitution sees it. Environment variables carry `secret`;
 * a plain KeyValue is treated as not secret. */
export interface VariableBinding extends KeyValue {
  secret?: boolean;
}

export interface SubstituteOptions {
  /**
   * Leave `{{name}}` in place wherever the variable that would apply is a
   * secret. Used for the request snapshot an example keeps, which must not
   * carry a secret (PLAN.md Phase 9) — the send itself still resolves
   * everything.
   */
  leaveSecrets?: boolean;
}

/** `[^{}]*` on purpose: a stray `{{` with no closer matches nothing. */
const PLACEHOLDER = /\{\{([^{}]*)\}\}/g;

/**
 * First definition wins. Duplicates are possible — the editor is an ordered
 * list, not a map — and the row nearest the top is the one a reader's eye
 * lands on, so that is the one that applies.
 */
export function variableMap(
  variables: readonly VariableBinding[],
  options: SubstituteOptions = {},
): Map<string, string> {
  const map = new Map<string, string>();
  const seen = new Set<string>();
  for (const variable of variables) {
    const name = variable.name.trim();
    if (name === "" || seen.has(name)) {
      continue;
    }
    // Recorded before the secret check, so a later non-secret duplicate
    // cannot stand in for a secret that won: the placeholder stays instead.
    seen.add(name);
    if (options.leaveSecrets === true && variable.secret === true) {
      continue;
    }
    map.set(name, variable.value);
  }
  return map;
}

/**
 * An unknown name is left exactly as written rather than replaced with an
 * empty string: a request that visibly asks for `{{token}}` is far easier to
 * diagnose than one that silently sends `Bearer `.
 *
 * Substitution is a single pass, so a value that itself contains `{{x}}` is
 * not expanded again. That is a deliberate limit, not an oversight — it
 * makes a self-referencing variable impossible to hang the app on.
 */
export function substitute(text: string, variables: Map<string, string>): string {
  if (!text.includes("{{")) {
    return text;
  }
  return text.replace(PLACEHOLDER, (match: string, rawName: string): string => {
    const value = variables.get(rawName.trim());
    return value === undefined ? match : value;
  });
}

/** Every free-text field in the request, so there is no field the user has
 * to remember is exempt. */
export function substituteRequestInput(
  input: SendRequestInput,
  variables: readonly VariableBinding[],
  options: SubstituteOptions = {},
): SendRequestInput {
  const map = variableMap(variables, options);
  if (map.size === 0) {
    return input;
  }
  return {
    ...input,
    url: substitute(input.url, map),
    headers: substitutePairs(input.headers, map),
    queryParams: substitutePairs(input.queryParams, map),
    body: substituteBody(input.body, map),
    auth: substituteAuth(input.auth, map),
  };
}

/**
 * The WebSocket counterpart of substituteRequestInput: the URL and every
 * handshake header, by the same rules. Settings hold nothing typed as text
 * except the proxy, which the HTTP path does not substitute either.
 */
export function substituteWebSocketRequest(
  request: WebSocketRequest,
  variables: readonly VariableBinding[],
  options: SubstituteOptions = {},
): WebSocketRequest {
  const map = variableMap(variables, options);
  if (map.size === 0) {
    return request;
  }
  return {
    ...request,
    url: substitute(request.url, map),
    headers: substitutePairs(request.headers, map),
  };
}

/** A message from the composer, substituted when it is sent rather than
 * when it is typed, like every other field. */
export function substituteMessage(
  text: string,
  variables: readonly VariableBinding[],
  options: SubstituteOptions = {},
): string {
  return substitute(text, variableMap(variables, options));
}

function substitutePairs(pairs: KeyValue[], map: Map<string, string>): KeyValue[] {
  return pairs.map((pair) => ({
    name: substitute(pair.name, map),
    value: substitute(pair.value, map),
  }));
}

function substituteBody(body: RequestBody, map: Map<string, string>): RequestBody {
  switch (body.kind) {
    case "none":
      return body;
    case "raw":
      return {
        kind: "raw",
        contentType: substitute(body.contentType, map),
        text: substitute(body.text, map),
      };
    case "formUrlEncoded":
      return { kind: "formUrlEncoded", fields: substitutePairs(body.fields, map) };
    case "multipart":
      return {
        kind: "multipart",
        // A path is substituted too: {{fixtures}}/logo.png is as reasonable
        // a thing to want as {{base_url}}. contentType is left alone — it is
        // derived from the chosen file, not something the user typed.
        parts: body.parts.map((part) =>
          part.kind === "file"
            ? { ...part, name: substitute(part.name, map), path: substitute(part.path, map) }
            : {
                ...part,
                name: substitute(part.name, map),
                value: substitute(part.value, map),
              },
        ),
      };
  }
}

function substituteAuth(auth: Auth, map: Map<string, string>): Auth {
  switch (auth.kind) {
    case "none":
      return auth;
    case "basic":
      return {
        kind: "basic",
        username: substitute(auth.username, map),
        password: substitute(auth.password, map),
      };
    case "bearer":
      return { kind: "bearer", token: substitute(auth.token, map) };
    case "apiKey":
      return {
        kind: "apiKey",
        key: substitute(auth.key, map),
        value: substitute(auth.value, map),
        location: auth.location,
      };
    case "custom":
      return {
        kind: "custom",
        headerName: substitute(auth.headerName, map),
        headerValue: substitute(auth.headerValue, map),
      };
  }
}
