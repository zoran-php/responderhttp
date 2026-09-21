// http_client/src/lib/request-defaults.ts
//
// The blank request every new tab starts from, plus the equality check that
// tells a tab whether it has unsaved changes. Kept in lib/ as pure, tested
// functions per CLAUDE.md section 6 rather than reimplemented in the store.
import {
  AUTH_NONE,
  DEFAULT_SETTINGS,
  type MultipartPart,
  type SendRequestInput,
} from "@/types/http";

export function emptyRequestInput(): SendRequestInput {
  return {
    method: "GET",
    url: "",
    headers: [],
    queryParams: [],
    body: { kind: "none" },
    auth: AUTH_NONE,
    settings: DEFAULT_SETTINGS,
  };
}

/**
 * Structural equality for SendRequestInput, used to decide whether a tab is
 * dirty. Every producer of this shape happens to build its keys in the same
 * order today, so a plain JSON.stringify comparison would also pass — but
 * that is an easy invariant to break by accident later, so this compares
 * fields directly instead of trusting key order.
 */
export function requestInputsEqual(a: SendRequestInput, b: SendRequestInput): boolean {
  return (
    a.method === b.method &&
    a.url === b.url &&
    keyValuesEqual(a.headers, b.headers) &&
    keyValuesEqual(a.queryParams, b.queryParams) &&
    bodiesEqual(a.body, b.body) &&
    authEqual(a.auth, b.auth) &&
    settingsEqual(a.settings, b.settings)
  );
}

function keyValuesEqual(a: SendRequestInput["headers"], b: SendRequestInput["headers"]): boolean {
  return (
    a.length === b.length &&
    a.every((pair, index) => {
      const other = b[index];
      return other !== undefined && pair.name === other.name && pair.value === other.value;
    })
  );
}

function bodiesEqual(a: SendRequestInput["body"], b: SendRequestInput["body"]): boolean {
  if (a.kind !== b.kind) {
    return false;
  }
  switch (a.kind) {
    case "none":
      return true;
    case "raw":
      return b.kind === "raw" && a.contentType === b.contentType && a.text === b.text;
    case "formUrlEncoded":
      return b.kind === "formUrlEncoded" && keyValuesEqual(a.fields, b.fields);
    case "multipart":
      return b.kind === "multipart" && multipartPartsEqual(a.parts, b.parts);
  }
}

/** Dirty-checking, so it has to notice a swapped file as well as a retyped
 * value — comparing names alone would call a changed upload "saved". */
function multipartPartsEqual(a: MultipartPart[], b: MultipartPart[]): boolean {
  if (a.length !== b.length) {
    return false;
  }
  return a.every((part, index) => {
    const other = b[index];
    if (!other || other.kind !== part.kind || other.name !== part.name) {
      return false;
    }
    return part.kind === "file"
      ? other.kind === "file" && other.path === part.path
      : other.kind === "text" && other.value === part.value;
  });
}

function authEqual(a: SendRequestInput["auth"], b: SendRequestInput["auth"]): boolean {
  if (a.kind !== b.kind) {
    return false;
  }
  switch (a.kind) {
    case "none":
      return true;
    case "basic":
      return b.kind === "basic" && a.username === b.username && a.password === b.password;
    case "bearer":
      return b.kind === "bearer" && a.token === b.token;
    case "apiKey":
      return (
        b.kind === "apiKey" &&
        a.key === b.key &&
        a.value === b.value &&
        a.location === b.location
      );
    case "custom":
      return (
        b.kind === "custom" && a.headerName === b.headerName && a.headerValue === b.headerValue
      );
  }
}

function settingsEqual(a: SendRequestInput["settings"], b: SendRequestInput["settings"]): boolean {
  return (
    a.followRedirects === b.followRedirects &&
    a.maxRedirects === b.maxRedirects &&
    a.timeoutMs === b.timeoutMs &&
    a.verifyTls === b.verifyTls &&
    a.proxy === b.proxy &&
    a.sendCookies === b.sendCookies &&
    a.httpVersion === b.httpVersion &&
    a.keepMethodOnRedirect === b.keepMethodOnRedirect &&
    a.keepAuthOnRedirect === b.keepAuthOnRedirect &&
    a.encodeUrl === b.encodeUrl &&
    a.allowHttp09 === b.allowHttp09 &&
    a.tlsMinimum === b.tlsMinimum
  );
}
