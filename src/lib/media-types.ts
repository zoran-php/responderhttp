// http_client/src/lib/media-types.ts
//
// The media types offered anywhere the user picks one: the raw body editor's
// content-type box, and the Accept / Content-Type header value suggestions.
// One list rather than two, so the body editor and the Headers tab cannot
// drift (CLAUDE.md section 7, DRY).
//
// A suggestion list, never a restriction — every place that uses it accepts
// any typed value.

/** Ordered by how often an API client actually sends them, not alphabetically:
 * a datalist shows its options in order, so the useful ones belong first. */
export const MEDIA_TYPES: readonly string[] = [
  "application/json",
  "application/xml",
  "application/x-www-form-urlencoded",
  "multipart/form-data",
  "text/plain",
  "text/html",
  "text/csv",
  "application/octet-stream",
  "application/pdf",
  "application/graphql",
  "application/ld+json",
  "application/problem+json",
  "text/event-stream",
];
