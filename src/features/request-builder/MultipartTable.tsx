// http_client/src/features/request-builder/MultipartTable.tsx
//
// Multipart's own table. Headers, params and form-urlencoded still share
// KeyValueTable (CLAUDE.md section 6) — this exists because a multipart row
// genuinely is not a key-value pair any more: it is text or a file, and a
// file row carries a path and a content type it did not choose.
//
// Presentational. Row bookkeeping is in lib/multipart-rows.ts, and picking a
// file goes through the parent, which owns the service call.
import { File as FileIcon, Trash2 } from "lucide-react";

import {
  removeMultipartRow,
  updateMultipartRow,
  withTrailingBlankPart,
  type MultipartRow,
  type MultipartRowKind,
} from "@/lib/multipart-rows";

const KINDS: { value: MultipartRowKind; label: string }[] = [
  { value: "text", label: "Text" },
  { value: "file", label: "File" },
];

interface MultipartTableProps {
  rows: MultipartRow[];
  onChange: (rows: MultipartRow[]) => void;
  /** Opens the native chooser; resolves to null if dismissed. */
  onChooseFile: (rowId: string) => void;
}

export function MultipartTable({ rows, onChange, onChooseFile }: MultipartTableProps) {
  function patch(id: string, changes: Partial<Omit<MultipartRow, "id">>) {
    onChange(withTrailingBlankPart(updateMultipartRow(rows, id, changes)));
  }

  return (
    <table className="w-full text-left text-sm">
      <thead>
        <tr className="border-b border-border text-xs text-muted-foreground">
          <th className="w-28 px-3 py-1.5 font-medium">Type</th>
          <th className="w-1/3 px-3 py-1.5 font-medium">Part</th>
          <th className="px-3 py-1.5 font-medium">Value</th>
          <th className="w-10 px-3 py-1.5" />
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => (
          <tr className="border-b border-border/50" key={row.id}>
            <td className="px-3 py-1">
              <select
                aria-label={`Part type for ${row.name || "new part"}`}
                className="h-7 w-full rounded border border-input bg-background px-1 text-xs"
                onChange={(event) =>
                  patch(row.id, { kind: event.target.value as MultipartRowKind })
                }
                value={row.kind}
              >
                {KINDS.map((kind) => (
                  <option key={kind.value} value={kind.value}>
                    {kind.label}
                  </option>
                ))}
              </select>
            </td>
            <td className="px-3 py-1">
              <input
                aria-label="Part name"
                className="h-7 w-full rounded border border-input bg-background px-2 text-sm"
                onChange={(event) => patch(row.id, { name: event.target.value })}
                spellCheck={false}
                value={row.name}
              />
            </td>
            <td className="px-3 py-1">
              {row.kind === "file" ? (
                <div className="flex items-center gap-2">
                  <button
                    className="inline-flex h-7 shrink-0 items-center gap-1.5 rounded border border-input px-2 text-xs hover:bg-accent active:bg-accent/70"
                    onClick={() => onChooseFile(row.id)}
                    type="button"
                  >
                    <FileIcon aria-hidden className="h-3.5 w-3.5" />
                    {row.path === "" ? "Choose file…" : "Change"}
                  </button>
                  {row.path === "" ? (
                    <span className="text-xs text-muted-foreground">No file chosen</span>
                  ) : (
                    // The full path is the title: it is what actually gets
                    // saved with the request, so it has to be inspectable.
                    <span className="min-w-0 flex-1 truncate text-xs" title={row.path}>
                      {row.fileName}
                      {row.contentType && (
                        <span className="ml-1.5 text-muted-foreground">({row.contentType})</span>
                      )}
                    </span>
                  )}
                </div>
              ) : (
                <input
                  aria-label="Part value"
                  className="h-7 w-full rounded border border-input bg-background px-2 text-sm"
                  onChange={(event) => patch(row.id, { value: event.target.value })}
                  spellCheck={false}
                  value={row.value}
                />
              )}
            </td>
            <td className="px-3 py-1">
              <button
                aria-label={`Remove ${row.name || "row"}`}
                className="rounded p-1 text-muted-foreground hover:bg-accent hover:text-destructive"
                onClick={() => onChange(withTrailingBlankPart(removeMultipartRow(rows, row.id)))}
                type="button"
              >
                <Trash2 aria-hidden className="h-3.5 w-3.5" />
              </button>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
