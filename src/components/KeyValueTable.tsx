// http_client/src/components/KeyValueTable.tsx
//
// The one key-value table. Headers, query params and form fields all use it —
// a second copy of this would be a review failure (CLAUDE.md section 6).
//
// Multipart is the deliberate exception (features/request-builder/
// MultipartTable.tsx): once a part could be a file, its row stopped being a
// key-value pair. That is a different shape, not a second copy of this one.
//
// Environment variables use this table too, with the optional secret column
// (PLAN.md Phase 9): their row is a key-value pair with a flag, which the
// generic row type carries through every edit.
import { useId } from "react";
import { Lock, LockOpen, Trash2 } from "lucide-react";

import { SecretInput } from "@/components/SecretInput";
import { removeRow, updateRow, type KeyValueRow } from "@/lib/key-values";

/** Turns on the secret column. Only the environment editor uses it. */
export interface SecretColumn<R extends KeyValueRow> {
  isSecret: (row: R) => boolean;
  onToggle: (rowId: string) => void;
  /** Shown under the value, e.g. why a secret loaded empty. */
  noticeFor: (row: R) => string | null;
}

interface KeyValueTableProps<R extends KeyValueRow> {
  rows: R[];
  nameLabel?: string;
  valueLabel?: string;
  onChange: (rows: R[]) => void;
  /** Autocomplete for the name column. Omitted by query params and form
   * fields, whose names are whatever the API happens to call them. */
  nameSuggestions?: readonly string[];
  /** Autocomplete for the value column, keyed off the name in that row — so
   * Accept-Encoding suggests gzip while the row above it suggests nothing. */
  valueSuggestionsFor?: (name: string) => readonly string[];
  secretColumn?: SecretColumn<R>;
}

export function KeyValueTable<R extends KeyValueRow>({
  rows,
  nameLabel = "Name",
  valueLabel = "Value",
  onChange,
  nameSuggestions,
  valueSuggestionsFor,
  secretColumn,
}: KeyValueTableProps<R>) {
  // Datalists are addressed by id, and more than one table can be mounted at
  // once (Params and Headers both live in the builder), so the ids have to be
  // unique per instance rather than per file.
  const listPrefix = useId();

  return (
    <table className="w-full text-sm">
      <thead>
        <tr className="border-b border-border text-left text-xs uppercase text-muted-foreground">
          <th className="w-2/5 px-3 py-2 font-medium">{nameLabel}</th>
          <th className="px-3 py-2 font-medium">{valueLabel}</th>
          {secretColumn && <th className="w-16 px-3 py-2 font-medium">Secret</th>}
          <th className="w-10" />
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => (
          <tr key={row.id} className="border-b border-border/50">
            <td className="p-1">
              <input
                aria-label={`${nameLabel} ${row.id}`}
                autoComplete="off"
                className="h-8 w-full rounded border border-transparent bg-transparent px-2 focus:border-input focus:bg-background"
                list={nameSuggestions ? `${listPrefix}-names` : undefined}
                spellCheck={false}
                value={row.name}
                onChange={(event) =>
                  onChange(updateRow(rows, row.id, { name: event.target.value }))
                }
              />
            </td>
            <td className="p-1">
              {secretColumn?.isSecret(row) ? (
                <SecretInput
                  aria-label={`${valueLabel} ${row.id}`}
                  className={VALUE_INPUT_CLASS}
                  onChange={(value) => onChange(updateRow(rows, row.id, { value }))}
                  value={row.value}
                />
              ) : (
                <input
                  aria-label={`${valueLabel} ${row.id}`}
                  autoComplete="off"
                  className={VALUE_INPUT_CLASS}
                  list={
                    valueSuggestionsFor && valueSuggestionsFor(row.name).length > 0
                      ? `${listPrefix}-values-${row.id}`
                      : undefined
                  }
                  spellCheck={false}
                  value={row.value}
                  onChange={(event) =>
                    onChange(updateRow(rows, row.id, { value: event.target.value }))
                  }
                />
              )}
              {secretColumn?.noticeFor(row) && (
                <p className="px-2 pt-0.5 text-xs text-destructive">
                  {secretColumn.noticeFor(row)}
                </p>
              )}
              {/* Per row, because the options depend on that row's name. Only
                  rendered when there are any, so an unknown header adds no
                  empty datalist to the DOM. */}
              {valueSuggestionsFor && valueSuggestionsFor(row.name).length > 0 && (
                <datalist id={`${listPrefix}-values-${row.id}`}>
                  {valueSuggestionsFor(row.name).map((value) => (
                    <option key={value} value={value} />
                  ))}
                </datalist>
              )}
            </td>
            {secretColumn && (
              <td className="p-1 text-center">
                <button
                  aria-label={`${secretColumn.isSecret(row) ? "Unmark" : "Mark"} ${row.name || "row"} as secret`}
                  aria-pressed={secretColumn.isSecret(row)}
                  className="rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground aria-pressed:text-primary"
                  onClick={() => secretColumn.onToggle(row.id)}
                  title={
                    secretColumn.isSecret(row)
                      ? "Secret: encrypted on disk and masked here"
                      : "Not secret: stored as plain text"
                  }
                  type="button"
                >
                  {secretColumn.isSecret(row) ? (
                    <Lock aria-hidden className="h-4 w-4" />
                  ) : (
                    <LockOpen aria-hidden className="h-4 w-4" />
                  )}
                </button>
              </td>
            )}
            <td className="p-1 text-center">
              <button
                aria-label={`Remove ${row.name || "row"}`}
                className="rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
                onClick={() => onChange(removeRow(rows, row.id))}
                type="button"
              >
                <Trash2 className="h-4 w-4" aria-hidden />
              </button>
            </td>
          </tr>
        ))}
      </tbody>
      {nameSuggestions && (
        <tfoot>
          <tr>
            <td colSpan={secretColumn ? 4 : 3}>
              {/* One list shared by every row: the name column's options do
                  not depend on the row. */}
              <datalist id={`${listPrefix}-names`}>
                {nameSuggestions.map((name) => (
                  <option key={name} value={name} />
                ))}
              </datalist>
            </td>
          </tr>
        </tfoot>
      )}
    </table>
  );
}

const VALUE_INPUT_CLASS =
  "h-8 w-full rounded border border-transparent bg-transparent px-2 font-mono text-xs " +
  "focus:border-input focus:bg-background";
