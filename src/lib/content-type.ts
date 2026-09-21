// http_client/src/lib/content-type.ts
//
// Content-type sniffing lives here, not in the viewer: the same mapping
// decides Monaco's language, whether pretty-printing applies, and which body
// type the request builder pre-selects.

export type BodyLanguage = "json" | "xml" | "html" | "plaintext";

/** Strips parameters: `application/json; charset=utf-8` -> `application/json`. */
export function essenceOf(contentType: string): string {
  return contentType.split(";")[0]?.trim().toLowerCase() ?? "";
}

export function languageForContentType(contentType: string | undefined): BodyLanguage {
  const essence = essenceOf(contentType ?? "");
  if (essence === "application/json" || essence.endsWith("+json")) {
    return "json";
  }
  if (essence === "text/html" || essence === "application/xhtml+xml") {
    return "html";
  }
  if (essence === "application/xml" || essence === "text/xml" || essence.endsWith("+xml")) {
    return "xml";
  }
  return "plaintext";
}

export function findHeader(headers: KeyValueLike[], name: string): string | undefined {
  const wanted = name.toLowerCase();
  return headers.find((header) => header.name.toLowerCase() === wanted)?.value;
}

interface KeyValueLike {
  name: string;
  value: string;
}
