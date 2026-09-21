// http_client/src/lib/query-sync.ts
//
// The Params tab as a view of the URL (decided
// 2026-09-17). The URL is the one source of truth: typing in the URL bar
// rewrites the table, and editing the table rewrites the URL's query string.
// Nothing else is stored — a request's `queryParams` is always empty once it
// has been through here.
//
// The table shows the query **as typed**, not decoded: `a%20b` stays
// `a%20b`. That is what makes the round trip lossless — there is no
// decode/encode step to disagree about `+`, a stray `%`, or an escape the
// user meant literally. Characters a URL cannot carry (a space, `ö`) are
// fixed when the request is sent, by "Encode URL automatically"
// (src-tauri/src/http/mapping.rs, encodeUrlForSend below).
import type { KeyValue } from "@/types/http";

export interface UrlParts {
  /** Everything before `?`: scheme, host and path. */
  base: string;
  /** The text after `?`, or null when the URL has no `?` at all. */
  query: string | null;
  /** From `#` on, including it, or "". It never reaches a server. */
  fragment: string;
}

export function splitUrl(url: string): UrlParts {
  const hashAt = url.indexOf("#");
  const main = hashAt === -1 ? url : url.slice(0, hashAt);
  const fragment = hashAt === -1 ? "" : url.slice(hashAt);
  const questionAt = main.indexOf("?");
  return questionAt === -1
    ? { base: main, query: null, fragment }
    : { base: main.slice(0, questionAt), query: main.slice(questionAt + 1), fragment };
}

/**
 * The query's pairs as typed. Split on `&` only; the first `=` separates a
 * name from its value, and a bare `flag` is a name with an empty value.
 * Empty segments (`a=1&&b=2`) are not pairs.
 */
export function queryPairs(url: string): KeyValue[] {
  const { query } = splitUrl(url);
  if (query === null || query === "") {
    return [];
  }
  return query
    .split("&")
    .filter((segment) => segment !== "")
    .map((segment) => {
      const equalsAt = segment.indexOf("=");
      return equalsAt === -1
        ? { name: segment, value: "" }
        : { name: segment.slice(0, equalsAt), value: segment.slice(equalsAt + 1) };
    });
}

/**
 * The URL with its query replaced by these pairs, as the table has them.
 *
 * Only the characters that would change the query's *structure* are escaped:
 * `&` and `#` anywhere, and `=` in a name. Everything else is written as
 * typed, so the URL bar shows what the user entered. A pair with an empty
 * value is written bare (`flag`). No pairs removes the `?` altogether.
 */
export function withQueryPairs(url: string, pairs: readonly KeyValue[]): string {
  const { base, fragment } = splitUrl(url);
  const query = pairs
    .filter((pair) => pair.name !== "" || pair.value !== "")
    .map((pair) => {
      const name = escapeStructural(pair.name, /[&#=]/g);
      return pair.value === "" ? name : `${name}=${escapeStructural(pair.value, /[&#]/g)}`;
    })
    .join("&");
  return query === "" ? `${base}${fragment}` : `${base}?${query}${fragment}`;
}

function escapeStructural(text: string, pattern: RegExp): string {
  return text.replace(
    pattern,
    (character) => `%${character.charCodeAt(0).toString(16).toUpperCase()}`,
  );
}

/**
 * A request saved before the Params tab became a view of the URL keeps its
 * params in a separate list. They are folded into the URL, after anything
 * already there — the order they were sent in — and encoded exactly as the
 * backend used to encode them, so the request still sends the same bytes.
 */
export function foldLegacyParams(
  url: string,
  legacy: readonly KeyValue[],
  encoded: boolean,
): string {
  const usable = legacy.filter((pair) => pair.name.trim() !== "");
  if (usable.length === 0) {
    return url;
  }
  const encode = encoded ? percentEncodeAll : (text: string) => text;
  return withQueryPairs(url, [
    ...queryPairs(url),
    ...usable.map((pair) => ({ name: encode(pair.name), value: encode(pair.value) })),
  ]);
}

/** Everything outside RFC 3986's unreserved set, as percent-escapes — the same
 * rule as `percent_encode` in src-tauri/src/http/mapping.rs. */
function percentEncodeAll(text: string): string {
  return Array.from(new TextEncoder().encode(text), (byte) => {
    const character = String.fromCharCode(byte);
    return /[A-Za-z0-9\-._~]/.test(character) ? character : `%${hex(byte)}`;
  }).join("");
}

const URL_SAFE = /[A-Za-z0-9\-._~!$&'()*+,;=:@/?#]/;

/**
 * What "Encode URL automatically" does to a URL on its way out. Mirrors
 * `encode_url_for_send` in src-tauri/src/http/mapping.rs, so Copy as cURL
 * shows what is actually sent: characters a URL cannot carry are
 * percent-encoded, valid `%XX` escapes and everything already legal are left
 * alone, a stray `%` becomes `%25`, and the scheme and host are not touched.
 */
export function encodeUrlForSend(url: string): string {
  const schemeEnd = url.indexOf("://");
  if (schemeEnd === -1) {
    return url;
  }
  const afterScheme = schemeEnd + 3;
  const authorityLength = url.slice(afterScheme).search(/[/?#]/);
  const authorityEnd = authorityLength === -1 ? url.length : afterScheme + authorityLength;
  const bytes = new TextEncoder().encode(url.slice(authorityEnd));
  let out = url.slice(0, authorityEnd);
  for (let index = 0; index < bytes.length; index += 1) {
    const byte = bytes[index] as number;
    const character = String.fromCharCode(byte);
    if (character === "%") {
      const escape = String.fromCharCode(bytes[index + 1] ?? 0, bytes[index + 2] ?? 0);
      out += /^[0-9A-Fa-f]{2}$/.test(escape) ? "%" : "%25";
    } else if (byte < 0x80 && URL_SAFE.test(character)) {
      out += character;
    } else {
      out += `%${hex(byte)}`;
    }
  }
  return out;
}

function hex(byte: number): string {
  return byte.toString(16).toUpperCase().padStart(2, "0");
}
