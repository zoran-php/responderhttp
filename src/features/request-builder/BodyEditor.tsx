// http_client/src/features/request-builder/BodyEditor.tsx
import { KeyValueTable } from "@/components/KeyValueTable";
import { LazyCodeEditor } from "@/components/LazyCodeEditor";
import { MultipartTable } from "@/features/request-builder/MultipartTable";
import { languageForContentType } from "@/lib/content-type";
import { MEDIA_TYPES } from "@/lib/media-types";
import type { KeyValueRow } from "@/lib/key-values";
import type { MultipartRow } from "@/lib/multipart-rows";
import type { BodyKind } from "@/store/request-store";

const BODY_KINDS: { kind: BodyKind; label: string }[] = [
  { kind: "none", label: "None" },
  { kind: "raw", label: "Raw" },
  { kind: "formUrlEncoded", label: "Form URL-encoded" },
  { kind: "multipart", label: "Multipart" },
];

interface BodyEditorProps {
  bodyKind: BodyKind;
  rawContentType: string;
  rawText: string;
  formRows: KeyValueRow[];
  multipartRows: MultipartRow[];
  onBodyKindChange: (kind: BodyKind) => void;
  onRawContentTypeChange: (contentType: string) => void;
  onRawTextChange: (text: string) => void;
  onFormRowsChange: (rows: KeyValueRow[]) => void;
  onMultipartRowsChange: (rows: MultipartRow[]) => void;
  onChooseMultipartFile: (rowId: string) => void;
}

export function BodyEditor({
  bodyKind,
  rawContentType,
  rawText,
  formRows,
  multipartRows,
  onBodyKindChange,
  onRawContentTypeChange,
  onRawTextChange,
  onFormRowsChange,
  onMultipartRowsChange,
  onChooseMultipartFile,
}: BodyEditorProps) {
  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-border px-3 py-2">
        <select
          aria-label="Body type"
          className="h-8 rounded-md border border-input bg-background px-2 text-sm"
          value={bodyKind}
          onChange={(event) => onBodyKindChange(event.target.value as BodyKind)}
        >
          {BODY_KINDS.map(({ kind, label }) => (
            <option key={kind} value={kind}>
              {label}
            </option>
          ))}
        </select>

        {bodyKind === "raw" && (
          <input
            aria-label="Content type"
            className="h-8 w-64 rounded-md border border-input bg-background px-2 font-mono text-xs"
            list="raw-content-types"
            spellCheck={false}
            value={rawContentType}
            onChange={(event) => onRawContentTypeChange(event.target.value)}
          />
        )}
        <datalist id="raw-content-types">
          {MEDIA_TYPES.map((type) => (
            <option key={type} value={type} />
          ))}
        </datalist>


      </div>

      <div className="flex-1 overflow-auto">
        {bodyKind === "none" && (
          <p className="p-3 text-sm text-muted-foreground">This request has no body.</p>
        )}
        {bodyKind === "raw" && (
          <LazyCodeEditor
            value={rawText}
            language={languageForContentType(rawContentType)}
            onChange={onRawTextChange}
          />
        )}
        {bodyKind === "formUrlEncoded" && (
          <KeyValueTable rows={formRows} nameLabel="Field" onChange={onFormRowsChange} />
        )}
        {bodyKind === "multipart" && (
          <MultipartTable
            onChange={onMultipartRowsChange}
            onChooseFile={onChooseMultipartFile}
            rows={multipartRows}
          />
        )}
      </div>
    </div>
  );
}
