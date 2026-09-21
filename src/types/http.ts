// http_client/src/types/http.ts
//
// Mirrors the DTOs in src-tauri/src/commands/dto.rs. Any change there has to
// land here in the same commit — these two files are one contract.

/** Mirrors HttpMethod in domain/models.rs. */
export const HTTP_METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] as const;

export type HttpMethod = (typeof HTTP_METHODS)[number];

/** Request headers, query params and form fields are all this shape. */
export interface KeyValue {
  name: string;
  value: string;
}

/**
 * One part of a multipart body. A file part carries the **path**, not the
 * bytes: libcurl opens and reads the file during the transfer, so an upload
 * never travels through IPC. The consequence is that a saved request stores a
 * path — move the file and it stops working, which the backend reports as an
 * invalid request naming the file rather than a transport failure.
 */
export type MultipartPart =
  | { kind: "text"; name: string; value: string }
  | { kind: "file"; name: string; path: string; contentType: string | null };

export type RequestBody =
  | { kind: "none" }
  | { kind: "raw"; contentType: string; text: string }
  | { kind: "formUrlEncoded"; fields: KeyValue[] }
  | { kind: "multipart"; parts: MultipartPart[] };

/** Mirrors ChosenFileDto in commands/files.rs. */
export interface ChosenFile {
  path: string;
  fileName: string;
  /** Null when the extension is not one the backend recognises. */
  contentType: string | null;
}

/** Mirrors ApiKeyLocation in domain/models.rs. */
export type ApiKeyLocation = "header" | "query";

/**
 * Mirrors Auth in domain/models.rs. "none" is a member rather than the type
 * being nullable, so the UI has something to select.
 */
export type Auth =
  | { kind: "none" }
  | { kind: "basic"; username: string; password: string }
  | { kind: "bearer"; token: string }
  | { kind: "apiKey"; key: string; value: string; location: ApiKeyLocation }
  | { kind: "custom"; headerName: string; headerValue: string };

export const AUTH_NONE: Auth = { kind: "none" };

/**
 * Mirrors HttpVersionPreference. "auto" is not "let libcurl decide" — it is
 * the behaviour this app has always had (attempt h2 over TLS, fall back to
 * 1.1), so the default changes nothing for a request that ignores it.
 */
export type HttpVersionPreference = "auto" | "http11" | "http2";

/**
 * Mirrors TlsMinimum. rustls supports 1.2 and 1.3 and nothing older, so —
 * unlike Postman's "protocols disabled during handshake" — there is no TLS
 * 1.0 or 1.1 here to switch off.
 */
export type TlsMinimum = "auto" | "tls12" | "tls13";

export interface RequestSettings {
  followRedirects: boolean;
  maxRedirects: number;
  timeoutMs: number;
  /** Off is an explicit per-request opt-in, never the default. */
  verifyTls: boolean;
  proxy: string | null;
  /** Off runs this one request without the cookie jar. */
  sendCookies: boolean;
  httpVersion: HttpVersionPreference;
  /** Keep the original method across a redirect; libcurl and browsers turn
   * POST into GET on 301/302/303. */
  keepMethodOnRedirect: boolean;
  /** Keep Authorization when a redirect crosses to another host. Off by
   * default because on is how credentials leak somewhere unintended. */
  keepAuthOnRedirect: boolean;
  /** Percent-encode the query parameters this app appends. The typed URL is
   * passed through untouched either way. */
  encodeUrl: boolean;
  /** Accept a bodies-only HTTP/0.9 response; off matches libcurl's default. */
  allowHttp09: boolean;
  tlsMinimum: TlsMinimum;
}

export const DEFAULT_SETTINGS: RequestSettings = {
  followRedirects: true,
  maxRedirects: 10,
  timeoutMs: 30_000,
  verifyTls: true,
  proxy: null,
  sendCookies: true,
  httpVersion: "auto",
  keepMethodOnRedirect: false,
  keepAuthOnRedirect: false,
  encodeUrl: true,
  allowHttp09: false,
  tlsMinimum: "auto",
};

export interface SendRequestInput {
  method: HttpMethod;
  url: string;
  headers: KeyValue[];
  queryParams: KeyValue[];
  body: RequestBody;
  auth: Auth;
  settings: RequestSettings;
}

export type ResponseBody =
  | { kind: "text"; text: string }
  | { kind: "binary"; byteLength: number };

export interface Timing {
  dnsMs: number;
  connectMs: number;
  tlsMs: number;
  timeToFirstByteMs: number;
  totalMs: number;
}

export interface HttpResponse {
  status: number;
  headers: KeyValue[];
  body: ResponseBody;
  timing: Timing;
}

/**
 * Mirrors DownloadResultDto in commands/dto.rs. Carries no body: the bytes
 * were written to disk in Rust rather than shipped through IPC.
 *
 * `savedTo` is null when the Save As dialog was dismissed — the request still
 * happened, and its status and timing are still worth showing.
 */
export interface DownloadResult {
  status: number;
  headers: KeyValue[];
  timing: Timing;
  byteLength: number;
  savedTo: string | null;
}

/**
 * Mirrors ApiError in commands/error.rs. notFound and storage are produced
 * by the collections commands (services/collections.ts); send_request /
 * cancel_request never emit them but every consumer of ApiError still has
 * to handle all six kinds.
 */
export type ApiError =
  | { kind: "invalidRequest"; message: string }
  | { kind: "transport"; message: string }
  | { kind: "cancelled"; message: string }
  | { kind: "notFound"; message: string }
  | { kind: "storage"; message: string }
  | { kind: "internal"; message: string }
  /** The OS credential store holding the data key could not be used, so a
   * secret cannot be saved (PLAN.md Phase 9). */
  | { kind: "secretStore"; message: string };

/**
 * Whether a stored secret could be read back. Mirrors SecretStateDto.
 * - needsReentry: gone for good (the key changed or the value was damaged);
 *   the field loads empty and has to be typed again.
 * - unavailable: the credential store could not be reached this session;
 *   the value may be fine, and saving over it is refused.
 */
export type SecretState = "ok" | "needsReentry" | "unavailable";
