// http_client/src/features/environments/EnvironmentsPanel.tsx
//
// The Environments tab of the sidebar: create, rename, delete, and open one
// for editing. Clicking a name opens it as a tab; making it the active
// environment is a separate control (EnvironmentSelector), so editing one
// while sending against another is possible.
import { useState } from "react";
import { Check, Pencil, Plus, Trash2 } from "lucide-react";

import { ConfirmDialog } from "@/components/ConfirmDialog";
// Lives under features/collections for now; it belongs in components/ once
// there is a way to remove the old path.
import { InlineTextInput } from "@/features/collections/InlineTextInput";
import { useEnvironmentsStore } from "@/store/environments-store";
import type { Environment } from "@/types/environments";

interface EnvironmentsPanelProps {
  onOpenEnvironment: (environmentId: string) => void;
}

export function EnvironmentsPanel({ onOpenEnvironment }: EnvironmentsPanelProps) {
  const environments = useEnvironmentsStore((state) => state.environments);
  const activeEnvironmentId = useEnvironmentsStore((state) => state.activeEnvironmentId);
  const error = useEnvironmentsStore((state) => state.error);
  // Selected individually rather than off the whole store object, which
  // would be a new reference on every render.
  const createEnvironment = useEnvironmentsStore((state) => state.createEnvironment);
  const renameEnvironment = useEnvironmentsStore((state) => state.renameEnvironment);
  const deleteEnvironment = useEnvironmentsStore((state) => state.deleteEnvironment);
  const setActiveEnvironment = useEnvironmentsStore((state) => state.setActiveEnvironment);

  const [creating, setCreating] = useState(false);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<Environment | null>(null);

  async function handleCreate(name: string) {
    setCreating(false);
    const created = await createEnvironment(name);
    if (created) {
      onOpenEnvironment(created.id);
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center justify-end border-b border-border px-3 py-2">
        <button
          aria-label="New environment"
          className="rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
          onClick={() => setCreating(true)}
          type="button"
        >
          <Plus aria-hidden className="h-4 w-4" />
        </button>
      </div>

      {error && (
        <p className="border-b border-border px-3 py-2 text-xs text-destructive">{error.message}</p>
      )}

      <div className="min-h-0 flex-1 overflow-auto p-1">
        {environments.length === 0 && !creating && (
          <p className="px-2 py-3 text-sm text-muted-foreground">
            No environments yet. Create one to hold {"{{variables}}"}.
          </p>
        )}

        {environments.map((environment) => {
          const isActive = environment.id === activeEnvironmentId;
          return (
            <div
              className="group flex items-center gap-1 rounded px-1 py-1 text-sm hover:bg-accent"
              key={environment.id}
            >
              {renamingId === environment.id ? (
                <InlineTextInput
                  initialValue={environment.name}
                  onCancel={() => setRenamingId(null)}
                  onCommit={(name) => {
                    setRenamingId(null);
                    void renameEnvironment(environment.id, name);
                  }}
                />
              ) : (
                <>
                  <button
                    aria-label={
                      isActive
                        ? `${environment.name} is the active environment`
                        : `Make ${environment.name} the active environment`
                    }
                    className={`shrink-0 rounded p-0.5 ${
                      isActive ? "text-primary" : "text-muted-foreground/40 hover:text-foreground"
                    }`}
                    onClick={() => setActiveEnvironment(isActive ? null : environment.id)}
                    type="button"
                  >
                    <Check aria-hidden className="h-3.5 w-3.5" />
                  </button>
                  <button
                    className={`min-w-0 flex-1 truncate text-left ${isActive ? "font-medium" : ""}`}
                    onClick={() => onOpenEnvironment(environment.id)}
                    type="button"
                  >
                    {environment.name}
                  </button>
                  <button
                    aria-label={`Rename ${environment.name}`}
                    className="shrink-0 rounded p-0.5 text-muted-foreground opacity-0 hover:text-foreground group-hover:opacity-100"
                    onClick={() => setRenamingId(environment.id)}
                    type="button"
                  >
                    <Pencil aria-hidden className="h-3.5 w-3.5" />
                  </button>
                  <button
                    aria-label={`Delete ${environment.name}`}
                    className="shrink-0 rounded p-0.5 text-muted-foreground opacity-0 hover:text-destructive group-hover:opacity-100"
                    onClick={() => setDeleting(environment)}
                    type="button"
                  >
                    <Trash2 aria-hidden className="h-3.5 w-3.5" />
                  </button>
                </>
              )}
            </div>
          );
        })}

        {creating && (
          <div className="flex items-center gap-1 px-1 py-1">
            <InlineTextInput
              onCancel={() => setCreating(false)}
              onCommit={(name) => void handleCreate(name)}
              placeholder="Environment name"
            />
          </div>
        )}
      </div>

      {deleting && (
        <ConfirmDialog
          confirmLabel="Delete"
          message={`"${deleting.name}" and its variables will be deleted. This cannot be undone.`}
          onCancel={() => setDeleting(null)}
          onConfirm={() => {
            void deleteEnvironment(deleting.id);
            setDeleting(null);
          }}
          title="Delete environment"
        />
      )}
    </div>
  );
}
