// http_client/src/lib/key-values.ts
//
// Row bookkeeping for the key-value tables. Rows carry their own id so React
// keys stay stable while a user edits or reorders them — array indices would
// remount the wrong input (CLAUDE.md section 6).
import type { KeyValue } from "@/types/http";

export interface KeyValueRow extends KeyValue {
  id: string;
}

let nextRowId = 0;

/** Monotonic and unique per session, which is all a React key needs. */
export function newRowId(): string {
  nextRowId += 1;
  return `row-${nextRowId}`;
}

export function emptyRow(): KeyValueRow {
  return { id: newRowId(), name: "", value: "" };
}

/** A table always shows one blank row at the end to type into. */
export function withTrailingBlank(rows: KeyValueRow[]): KeyValueRow[] {
  const last = rows[rows.length - 1];
  if (last && last.name === "" && last.value === "") {
    return rows;
  }
  return [...rows, emptyRow()];
}

/** Generic so a row carrying more than a name and a value (a variable's
 * secret flag, say) keeps the rest through an edit. */
export function updateRow<R extends KeyValueRow>(
  rows: R[],
  id: string,
  patch: Partial<Omit<KeyValueRow, "id">>,
): R[] {
  return rows.map((row) => (row.id === id ? { ...row, ...patch } : row));
}

export function removeRow<R extends KeyValueRow>(rows: R[], id: string): R[] {
  return rows.filter((row) => row.id !== id);
}

/** What actually gets sent: rows without a name are scaffolding, not data. */
export function toKeyValues(rows: KeyValueRow[]): KeyValue[] {
  return rows
    .filter((row) => row.name.trim() !== "")
    .map((row) => ({ name: row.name.trim(), value: row.value }));
}

/**
 * Rows for these pairs, reusing the ids of the rows already on screen by
 * position. Used when the table is rebuilt from the URL on every keystroke in
 * the URL bar: stable ids keep React from remounting the inputs. Always ends
 * with a blank row to type into.
 */
export function rowsKeepingIds(previous: KeyValueRow[], values: KeyValue[]): KeyValueRow[] {
  return withTrailingBlank(
    values.map((value, index) => ({ ...value, id: previous[index]?.id ?? newRowId() })),
  );
}

/**
 * The inverse of toKeyValues, used when a saved request is loaded back into
 * the builder: give each stored pair a fresh row id and leave a blank row to
 * type into, same as a table starts out.
 */
export function rowsFromKeyValues(values: KeyValue[]): KeyValueRow[] {
  return withTrailingBlank(values.map((value) => ({ ...value, id: newRowId() })));
}
