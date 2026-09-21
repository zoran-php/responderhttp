// http_client/src/lib/multipart-rows.ts
//
// Row bookkeeping for the multipart table. Separate from key-values.ts
// because a multipart row is no longer a key-value pair: it is either text or
// a file, and a file row carries a path and a content type it did not choose.
//
// The shared KeyValueTable stays the one table for headers, params and
// form-urlencoded (CLAUDE.md section 6). Multipart earned its own because the
// row shape genuinely differs, not because a second copy was convenient.
import { newRowId } from "@/lib/key-values";
import type { MultipartPart } from "@/types/http";

export type MultipartRowKind = MultipartPart["kind"];

/**
 * Both kinds in one row so toggling text ⇄ file keeps what was already typed.
 * Switching to File and back should not silently discard a value.
 */
export interface MultipartRow {
  id: string;
  kind: MultipartRowKind;
  name: string;
  /** Used when kind is "text". */
  value: string;
  /** Used when kind is "file"; empty until a file is chosen. */
  path: string;
  fileName: string;
  contentType: string | null;
}

export function emptyMultipartRow(): MultipartRow {
  return {
    id: newRowId(),
    kind: "text",
    name: "",
    value: "",
    path: "",
    fileName: "",
    contentType: null,
  };
}

function isBlank(row: MultipartRow): boolean {
  return row.name === "" && row.value === "" && row.path === "";
}

/** A table always shows one blank row at the end to type into. */
export function withTrailingBlankPart(rows: MultipartRow[]): MultipartRow[] {
  const last = rows[rows.length - 1];
  if (last && isBlank(last)) {
    return rows;
  }
  return [...rows, emptyMultipartRow()];
}

export function updateMultipartRow(
  rows: MultipartRow[],
  id: string,
  patch: Partial<Omit<MultipartRow, "id">>,
): MultipartRow[] {
  return rows.map((row) => (row.id === id ? { ...row, ...patch } : row));
}

export function removeMultipartRow(rows: MultipartRow[], id: string): MultipartRow[] {
  return rows.filter((row) => row.id !== id);
}

/**
 * What actually gets sent. A row without a name is scaffolding, and so is a
 * file row with no file chosen yet — sending it would fail validation in the
 * backend for a row the user has not finished filling in.
 */
export function toMultipartParts(rows: MultipartRow[]): MultipartPart[] {
  const parts: MultipartPart[] = [];
  for (const row of rows) {
    const name = row.name.trim();
    if (name === "") {
      continue;
    }
    if (row.kind === "file") {
      if (row.path === "") {
        continue;
      }
      parts.push({ kind: "file", name, path: row.path, contentType: row.contentType });
    } else {
      parts.push({ kind: "text", name, value: row.value });
    }
  }
  return parts;
}

/** The inverse, for loading a saved request back into the builder. */
export function rowsFromMultipartParts(parts: MultipartPart[]): MultipartRow[] {
  return withTrailingBlankPart(
    parts.map((part) =>
      part.kind === "file"
        ? {
            ...emptyMultipartRow(),
            id: newRowId(),
            kind: "file" as const,
            name: part.name,
            path: part.path,
            fileName: basename(part.path),
            contentType: part.contentType,
          }
        : {
            ...emptyMultipartRow(),
            id: newRowId(),
            kind: "text" as const,
            name: part.name,
            value: part.value,
          },
    ),
  );
}

/**
 * Display only. Handles both separators because a collection saved on Windows
 * can be opened on macOS, where the path is a meaningless string but its last
 * segment is still the useful thing to show.
 */
export function basename(path: string): string {
  const segments = path.split(/[/\\]/);
  return segments[segments.length - 1] ?? path;
}
