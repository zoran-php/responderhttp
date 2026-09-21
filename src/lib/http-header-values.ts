// http_client/src/lib/http-header-values.ts
//
// Suggested values per header, for the Headers tab's value autocomplete.
//
// **These do not come from IANA.** The registry records field *names* only —
// there is no values column and no companion registry for them. Every list
// below is hand-written from the RFC that defines the field, which is why
// each group cites one. Treat that as the maintenance burden it is: a wrong
// suggestion here is a bug that no upstream refresh will fix.
//
// Only headers that carry a small closed set of tokens are listed. Dates,
// URLs, credentials, digests and quoted ETags are all free text, and a
// suggestion for them would be noise — so Host, Origin, Referer,
// If-Modified-Since and friends are deliberately absent.
import { HTTP_METHODS } from "@/types/http";
import { MEDIA_TYPES } from "@/lib/media-types";

/**
 * Keys are lowercase; lookup lowercases too, because header names are
 * case-insensitive (RFC 9110 §5.1) and the user may type any casing.
 *
 * Every key here must also appear in IANA_HEADER_NAMES — a value suggestion
 * for a name we never suggest would be a quiet inconsistency, and there is a
 * test that fails if the two drift.
 */
const VALUES: Record<string, readonly string[]> = {
  // RFC 9110 §12.5 — content negotiation.
  accept: ["*/*", ...MEDIA_TYPES],
  "accept-encoding": ["gzip", "deflate", "br", "zstd", "identity", "*"],
  "accept-language": ["*", "en", "en-US", "en-GB"],

  // RFC 9110 §8.
  "content-type": MEDIA_TYPES,
  "content-encoding": ["gzip", "deflate", "br", "zstd", "identity"],
  "content-language": ["en", "en-US"],

  // RFC 9111 §5.2 — request directives only; no-transform and max-age are
  // also response directives, which is why they read the same either way.
  "cache-control": [
    "no-cache",
    "no-store",
    "no-transform",
    "only-if-cached",
    "max-age=0",
    "max-stale",
    "min-fresh=0",
  ],

  // RFC 9110 §7.6.1 and §7.8.
  connection: ["keep-alive", "close", "upgrade"],
  upgrade: ["websocket", "h2c"],
  te: ["trailers", "gzip", "deflate", "compress"],
  "transfer-encoding": ["chunked", "gzip", "deflate", "compress"],

  // RFC 9110 §10.1.1 — the only defined expectation.
  expect: ["100-continue"],

  // RFC 9110 §11.6.2 / §11.7.1 — scheme prefixes, left with the trailing
  // space so the credential can be typed straight after.
  authorization: ["Bearer ", "Basic ", "Digest ", "Negotiate "],
  "proxy-authorization": ["Bearer ", "Basic ", "Digest "],

  // RFC 9110 §13.1.1 / §13.1.2 — the wildcard; an ETag is free text.
  "if-match": ["*"],
  "if-none-match": ["*"],

  // RFC 9110 §14.2 — bytes is the only range unit HTTP defines.
  range: ["bytes=0-", "bytes=0-1023", "bytes=-1024"],

  // RFC 9110 §7.6.2.
  "max-forwards": ["0", "1", "10"],

  // RFC 7240.
  prefer: [
    "respond-async",
    "return=minimal",
    "return=representation",
    "wait=10",
    "handling=lenient",
    "handling=strict",
  ],

  // RFC 9218 — urgency 0 (highest) to 7, and the incremental flag.
  priority: ["u=0", "u=1", "u=2", "u=3", "u=4", "u=5", "u=6", "u=7", "i"],

  // Fetch Metadata, W3C.
  "sec-fetch-mode": ["cors", "navigate", "no-cors", "same-origin", "websocket"],
  "sec-fetch-site": ["cross-site", "same-origin", "same-site", "none"],
  "sec-fetch-dest": [
    "empty",
    "document",
    "iframe",
    "image",
    "script",
    "style",
    "font",
    "audio",
    "video",
    "worker",
    "object",
    "embed",
    "manifest",
    "report",
  ],
  "sec-gpc": ["1"],

  // CORS preflight: the method list is the one in types/http.ts rather than a
  // second copy of it.
  "access-control-request-method": HTTP_METHODS,

  // Response headers, but worth suggesting: an API client is often used to
  // reproduce what a server should send back.
  "x-content-type-options": ["nosniff"],
  vary: ["*", "Accept", "Accept-Encoding", "Origin"],
};

/** Empty when the header has no closed set of values worth suggesting. */
export function headerValueSuggestions(headerName: string): readonly string[] {
  return VALUES[headerName.trim().toLowerCase()] ?? [];
}

/** Exposed for the drift test; not meant for rendering. */
export function suggestedValueHeaderNames(): string[] {
  return Object.keys(VALUES);
}
