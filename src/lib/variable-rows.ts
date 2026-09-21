// http_client/src/lib/variable-rows.ts
//
// Row bookkeeping for the environment editor. A variable row is a key-value
// row plus the secret flag and whether its stored value could be read, so it
// shares the table (components/KeyValueTable.tsx) but not the row helpers in
// lib/key-values.ts, which know nothing about secrets.
import { newRowId, type KeyValueRow } from "@/lib/key-values";
import type { EnvironmentVariable, EnvironmentVariableInput } from "@/types/environments";
import type { SecretState } from "@/types/http";

export interface VariableRow extends KeyValueRow {
  secret: boolean;
  secretState: SecretState;
}

export function emptyVariableRow(): VariableRow {
  return { id: newRowId(), name: "", value: "", secret: false, secretState: "ok" };
}

/** The editor always ends with one blank row to type into. A blank row that
 * was already flagged secret still counts as blank. */
export function withTrailingBlankVariable(rows: VariableRow[]): VariableRow[] {
  const last = rows[rows.length - 1];
  if (last && last.name === "" && last.value === "") {
    return rows;
  }
  return [...rows, emptyVariableRow()];
}

export function rowsFromVariables(variables: EnvironmentVariable[]): VariableRow[] {
  return withTrailingBlankVariable(variables.map((variable) => ({ ...variable, id: newRowId() })));
}

/** What gets saved: rows without a name are scaffolding, not data. */
export function toVariableInputs(rows: VariableRow[]): EnvironmentVariableInput[] {
  return rows
    .filter((row) => row.name.trim() !== "")
    .map((row) => ({ name: row.name.trim(), value: row.value, secret: row.secret }));
}

/** Flipping the flag. The row's state is left alone: only typing a new
 * value answers "needs re-entering" (see `withEditedRows`). */
export function toggleSecret(rows: VariableRow[], id: string): VariableRow[] {
  return rows.map((row) => (row.id === id ? { ...row, secret: !row.secret } : row));
}

/**
 * Applies the table's edits and clears "needs re-entering" on any row whose
 * value changed. "unavailable" is left alone: saving is refused until the
 * credential store can be reached, so the warning still applies.
 */
export function withEditedRows(previous: VariableRow[], edited: KeyValueRow[]): VariableRow[] {
  const before = new Map(previous.map((row) => [row.id, row]));
  const next = edited.map((row): VariableRow => {
    const old = before.get(row.id);
    if (old === undefined) {
      return { ...row, secret: false, secretState: "ok" };
    }
    const secretState =
      old.secretState === "needsReentry" && row.value !== old.value ? "ok" : old.secretState;
    return { ...old, name: row.name, value: row.value, secretState };
  });
  return withTrailingBlankVariable(next);
}
