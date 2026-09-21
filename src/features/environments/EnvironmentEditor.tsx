// http_client/src/features/environments/EnvironmentEditor.tsx
//
// The key/value editor for one environment, shown where the request builder
// normally sits. Variables save explicitly rather than on every keystroke,
// so the tab has no dirty dot — Save is the only write.
//
// A variable marked secret is encrypted on disk and masked here (PLAN.md
// Phase 9). It resolves like any other; only examples leave it as a
// placeholder.
import { useEffect, useState } from "react";
import { Save } from "lucide-react";

import { KeyValueTable, type SecretColumn } from "@/components/KeyValueTable";
import {
  emptyVariableRow,
  rowsFromVariables,
  toggleSecret,
  toVariableInputs,
  withEditedRows,
  type VariableRow,
} from "@/lib/variable-rows";
import { useEnvironmentsStore } from "@/store/environments-store";
import type { SecretState } from "@/types/http";

const SECRET_NOTICES: Record<SecretState, string | null> = {
  ok: null,
  needsReentry: "Could not be decrypted on this computer. Enter it again.",
  unavailable: "The credential store is unavailable, so this cannot be read or saved right now.",
};

interface EnvironmentEditorProps {
  environmentId: string;
}

export function EnvironmentEditor({ environmentId }: EnvironmentEditorProps) {
  const environment = useEnvironmentsStore((state) =>
    state.environments.find((candidate) => candidate.id === environmentId),
  );
  const variables = useEnvironmentsStore((state) => state.variablesById[environmentId]);
  const loadVariables = useEnvironmentsStore((state) => state.loadVariables);
  const saveVariables = useEnvironmentsStore((state) => state.saveVariables);
  // Shown here as well as in the sidebar panel, which may not be open: a
  // secret that cannot be saved must not fail out of sight.
  const error = useEnvironmentsStore((state) => state.error);

  const [rows, setRows] = useState<VariableRow[]>([emptyVariableRow()]);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    void loadVariables(environmentId);
  }, [environmentId, loadVariables]);

  useEffect(() => {
    if (variables !== undefined) {
      setRows(rowsFromVariables(variables));
    }
  }, [variables]);

  async function handleSave() {
    setSaving(true);
    await saveVariables(environmentId, toVariableInputs(rows));
    setSaving(false);
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-3 border-b border-border px-4 py-3">
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium">{environment?.name ?? "Environment"}</p>
          <p className="text-xs text-muted-foreground">
            Reference these as <span className="font-mono">{"{{name}}"}</span> in a request.
          </p>
        </div>
        <button
          className="inline-flex h-9 items-center gap-2 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90 active:bg-primary/80 disabled:opacity-60"
          disabled={saving}
          onClick={() => void handleSave()}
          type="button"
        >
          <Save aria-hidden className="h-4 w-4" />
          {saving ? "Saving…" : "Save"}
        </button>
      </div>

      {error && (
        <p className="border-b border-border px-4 py-2 text-xs text-destructive">{error.message}</p>
      )}

      <div className="min-h-0 flex-1 overflow-auto">
        <KeyValueTable
          nameLabel="Variable"
          onChange={(edited) => setRows((previous) => withEditedRows(previous, edited))}
          rows={rows}
          secretColumn={secretColumn(setRows)}
        />
      </div>
    </div>
  );
}

function secretColumn(
  setRows: (update: (rows: VariableRow[]) => VariableRow[]) => void,
): SecretColumn<VariableRow> {
  return {
    isSecret: (row) => row.secret,
    onToggle: (rowId) => setRows((rows) => toggleSecret(rows, rowId)),
    noticeFor: (row) => SECRET_NOTICES[row.secretState],
  };
}
