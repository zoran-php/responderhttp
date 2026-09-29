// http_client/src/features/grpc/GrpcSchemaPanel.tsx
//
// The Service definition tab: where the schema comes from and what it
// holds. Server reflection (with Refresh), or an import of `.proto` files
// with any import paths their imports need. The choosers are native
// dialogs opened through the parent, which owns the service calls.
import { useState } from "react";
import { FileUp, FolderPlus, RefreshCw, X } from "lucide-react";

import { methodKindLabel } from "@/lib/grpc-status";
import type { GrpcSchemaStatus } from "@/store/request-store";
import type { ProtoSchema } from "@/types/grpc";

interface GrpcSchemaPanelProps {
  schema: ProtoSchema | null;
  status: GrpcSchemaStatus;
  error: string | null;
  onReflect: () => void;
  /** Opens the file chooser and imports what was chosen. */
  onImport: (importPaths: string[]) => void;
  /** Opens the folder chooser; null when cancelled. */
  onChooseImportFolder: () => Promise<string | null>;
}

const BUTTON =
  "inline-flex h-8 items-center gap-1.5 rounded-md border border-input px-3 text-xs text-muted-foreground hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50";

export function GrpcSchemaPanel(props: GrpcSchemaPanelProps) {
  const { schema, status } = props;
  // Only what the next import needs; a loaded schema already holds its files.
  const [importPaths, setImportPaths] = useState<string[]>([]);
  const loading = status === "loading";

  async function addImportPath() {
    const chosen = await props.onChooseImportFolder();
    if (chosen !== null && !importPaths.includes(chosen)) {
      setImportPaths([...importPaths, chosen]);
    }
  }

  return (
    <div className="space-y-4 p-4 text-sm">
      <div className="flex flex-wrap items-center gap-2">
        <button className={BUTTON} disabled={loading} onClick={props.onReflect} type="button">
          <RefreshCw aria-hidden className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} />
          Use server reflection
        </button>
        <button
          className={BUTTON}
          disabled={loading}
          onClick={() => props.onImport(importPaths)}
          type="button"
        >
          <FileUp aria-hidden className="h-3.5 w-3.5" />
          Import .proto files…
        </button>
        <button
          className={BUTTON}
          disabled={loading}
          onClick={() => void addImportPath()}
          type="button"
        >
          <FolderPlus aria-hidden className="h-3.5 w-3.5" />
          Add import path…
        </button>
      </div>

      {importPaths.length > 0 && (
        <div>
          <p className="mb-1 text-xs text-muted-foreground">
            Import paths, searched before each file&apos;s own folder:
          </p>
          <ul className="space-y-1">
            {importPaths.map((path) => (
              <li className="flex items-center gap-2 font-mono text-xs" key={path}>
                <span className="min-w-0 flex-1 truncate">{path}</span>
                <button
                  aria-label={`Remove ${path}`}
                  className="rounded p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground"
                  onClick={() => setImportPaths(importPaths.filter((other) => other !== path))}
                  type="button"
                >
                  <X aria-hidden className="h-3 w-3" />
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="border-t border-border pt-4" role="status">
        {status === "loading" && <p className="text-muted-foreground">Loading the schema…</p>}
        {status === "error" && <p className="text-destructive">{props.error}</p>}
        {status === "none" && (
          <p className="text-muted-foreground">
            No schema loaded. Use server reflection, or import the service&apos;s .proto files.
          </p>
        )}
        {status === "loaded" && schema !== null && <SchemaSummary schema={schema} />}
      </div>
    </div>
  );
}

function SchemaSummary({ schema }: { schema: ProtoSchema }) {
  const source =
    schema.origin === "reflection" ? "From server reflection" : "Imported from .proto files";
  return (
    <div className="space-y-3">
      <p className="text-xs text-muted-foreground">
        {source}
        {schema.name !== null && ` · saved in the library as "${schema.name}"`}
      </p>
      {schema.services.length === 0 && (
        <p className="text-muted-foreground">The schema defines no services.</p>
      )}
      {schema.services.map((service) => (
        <div key={service.name}>
          <p className="font-mono text-xs font-medium">{service.name}</p>
          <ul className="mt-1 space-y-0.5 pl-4">
            {service.methods.map((method) => (
              <li className="flex gap-2 font-mono text-xs" key={method.path}>
                <span>{method.name}</span>
                <span className="text-muted-foreground">
                  ({method.inputType}) → {method.outputType} · {methodKindLabel(method.kind)}
                </span>
              </li>
            ))}
          </ul>
        </div>
      ))}
      {schema.files.length > 0 && (
        <div>
          <p className="text-xs text-muted-foreground">Files</p>
          <ul className="mt-1 space-y-0.5 pl-4 font-mono text-xs">
            {schema.files.map((file) => (
              <li key={file}>{file}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
