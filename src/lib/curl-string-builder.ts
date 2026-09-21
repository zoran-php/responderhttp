// http_client/src/lib/curl-string-builder.ts
//
// Builds the human-readable `curl ...` line behind "Copy as cURL". This is a
// display artifact only — the app never executes it, and libcurl is called
// through the Rust client (CLAUDE.md section 4).
import { encodeUrlForSend } from "@/lib/query-sync";
import type { Auth, KeyValue, SendRequestInput } from "@/types/http";

const LINE_CONTINUATION = " \\\n  ";

export function buildCurlCommand(input: SendRequestInput): string {
  const parts: string[] = [`curl -X ${input.method}`];

  // Encoded the way the backend encodes before sending, so the pasted
  // command reaches the server with the same bytes — and curl, like libcurl,
  // refuses a URL with a space in it.
  const url = input.settings.encodeUrl ? encodeUrlForSend(input.url) : input.url;
  parts.push(quote(urlWithQuery(url, [...input.queryParams, ...authQueryParams(input.auth)])));

  for (const header of nonEmpty(input.headers)) {
    parts.push(`-H ${quote(`${header.name}: ${header.value}`)}`);
  }

  parts.push(...authFlags(input.auth, input.headers));

  const body = input.body;
  switch (body.kind) {
    case "none":
      break;
    case "raw":
      if (!hasContentType(input.headers) && body.contentType) {
        parts.push(`-H ${quote(`Content-Type: ${body.contentType}`)}`);
      }
      parts.push(`--data-raw ${quote(body.text)}`);
      break;
    case "formUrlEncoded":
      for (const field of nonEmpty(body.fields)) {
        parts.push(`--data-urlencode ${quote(`${field.name}=${field.value}`)}`);
      }
      break;
    case "multipart":
      for (const part of body.parts) {
        if (part.name.trim() === "") {
          continue;
        }
        if (part.kind === "file") {
          // curl's own syntax for "read this part from a file", so the copied
          // command uploads the same file rather than the literal path as
          // text. type= is appended only when we have one, letting curl fall
          // back to its own guess otherwise.
          const spec = part.contentType
            ? `${part.name}=@${part.path};type=${part.contentType}`
            : `${part.name}=@${part.path}`;
          parts.push(`-F ${quote(spec)}`);
        } else {
          parts.push(`-F ${quote(`${part.name}=${part.value}`)}`);
        }
      }
      break;
  }

  const {
    followRedirects,
    maxRedirects,
    timeoutMs,
    verifyTls,
    proxy,
    httpVersion,
    keepMethodOnRedirect,
    keepAuthOnRedirect,
    allowHttp09,
    tlsMinimum,
  } = input.settings;
  if (followRedirects) {
    parts.push("-L", `--max-redirs ${maxRedirects}`);
  }
  // Only emitted when the method should survive: curl's own default already
  // downgrades to GET, so the flags would be noise otherwise.
  if (followRedirects && keepMethodOnRedirect) {
    parts.push("--post301", "--post302", "--post303");
  }
  // --location-trusted is -L plus "keep credentials across hosts", so it
  // implies -L and is only meaningful alongside it.
  if (followRedirects && keepAuthOnRedirect) {
    parts.push("--location-trusted");
  }
  if (timeoutMs > 0) {
    parts.push(`--max-time ${round(timeoutMs / 1000)}`);
  }
  if (!verifyTls) {
    parts.push("-k");
  }
  if (httpVersion === "http11") {
    parts.push("--http1.1");
  }
  if (httpVersion === "http2") {
    parts.push("--http2");
  }
  if (tlsMinimum === "tls12") {
    parts.push("--tlsv1.2");
  }
  if (tlsMinimum === "tls13") {
    parts.push("--tlsv1.3");
  }
  if (allowHttp09) {
    parts.push("--http0.9");
  }
  if (proxy && proxy.trim()) {
    parts.push(`-x ${quote(proxy.trim())}`);
  }

  return parts.join(LINE_CONTINUATION);
}

/** An API key bound to the query string is part of the URL, not a flag. */
function authQueryParams(auth: Auth): KeyValue[] {
  if (auth.kind !== "apiKey" || auth.location !== "query" || auth.key.trim() === "") {
    return [];
  }
  return [{ name: auth.key.trim(), value: auth.value }];
}

/**
 * Mirrors the precedence in http/curl_client.rs: a header typed into the
 * Headers tab beats the one the scheme would generate, so the copied command
 * shows what the app would actually send.
 */
function authFlags(auth: Auth, headers: KeyValue[]): string[] {
  const taken = (name: string): boolean =>
    headers.some((header) => header.name.trim().toLowerCase() === name.toLowerCase());

  switch (auth.kind) {
    case "none":
      return [];
    case "basic":
      if ((auth.username === "" && auth.password === "") || taken("Authorization")) {
        return [];
      }
      // curl's own -u, rather than a pre-encoded header: it is what a person
      // would type, and curl does the base64 itself.
      return [`-u ${quote(`${auth.username}:${auth.password}`)}`];
    case "bearer": {
      const token = auth.token.trim();
      return token === "" || taken("Authorization")
        ? []
        : [`-H ${quote(`Authorization: Bearer ${token}`)}`];
    }
    case "apiKey": {
      const key = auth.key.trim();
      return key === "" || auth.location === "query" || taken(key)
        ? []
        : [`-H ${quote(`${key}: ${auth.value}`)}`];
    }
    case "custom": {
      const name = auth.headerName.trim();
      return name === "" || taken(name) ? [] : [`-H ${quote(`${name}: ${auth.headerValue}`)}`];
    }
  }
}

function urlWithQuery(url: string, params: KeyValue[]): string {
  const usable = nonEmpty(params);
  if (usable.length === 0) {
    return url;
  }
  const query = usable
    .map((param) => `${encodeURIComponent(param.name)}=${encodeURIComponent(param.value)}`)
    .join("&");
  const separator = url.includes("?") ? "&" : "?";
  return url.endsWith("?") || url.endsWith("&") ? `${url}${query}` : `${url}${separator}${query}`;
}

function nonEmpty(pairs: KeyValue[]): KeyValue[] {
  return pairs.filter((pair) => pair.name.trim() !== "");
}

function hasContentType(headers: KeyValue[]): boolean {
  return headers.some((header) => header.name.trim().toLowerCase() === "content-type");
}

/**
 * Single quotes, POSIX style: everything inside is literal, so only the
 * quote character itself needs the `'\''` dance. That keeps bodies with
 * double quotes or `$` readable and safe to paste into a shell.
 */
function quote(value: string): string {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}

function round(seconds: number): number {
  return Math.round(seconds * 100) / 100;
}
