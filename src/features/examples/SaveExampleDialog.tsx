// http_client/src/features/examples/SaveExampleDialog.tsx
//
// Names a response before it is saved under its request. Deliberately only
// asks for a name: where it goes is already decided — under the request that
// produced it — unlike SaveRequestDialog, which has to ask for a collection.
import { useState } from "react";

import { Modal } from "@/components/Modal";
import { useCollectionsStore } from "@/store/collections-store";
import type { Example } from "@/types/collections";
import type { HttpResponse, SendRequestInput } from "@/types/http";

interface SaveExampleDialogProps {
  collectionId: string;
  requestId: string;
  /** The request as sent, already resolved. */
  request: SendRequestInput;
  response: HttpResponse;
  onClose: () => void;
  onSaved: (example: Example) => void;
}

export function SaveExampleDialog({
  collectionId,
  requestId,
  request,
  response,
  onClose,
  onSaved,
}: SaveExampleDialogProps) {
  const saveExample = useCollectionsStore((state) => state.saveExample);
  // Prefilled with the status, which is what distinguishes one example from
  // another far more often than anything the user would type.
  const [name, setName] = useState(`${response.status} response`);
  const [saving, setSaving] = useState(false);

  async function handleSave() {
    if (name.trim().length === 0 || saving) {
      return;
    }
    setSaving(true);
    const saved = await saveExample(collectionId, {
      requestId,
      name: name.trim(),
      request,
      status: response.status,
      responseHeaders: response.headers,
      responseBody: response.body.kind === "text" ? response.body.text : "",
    });
    setSaving(false);
    if (saved) {
      onSaved(saved);
    }
  }

  return (
    // Inside the shared shell rather than a backdrop of its own: Modal is what
    // marks the keyboard as owned, so without it Ctrl+S would save the request
    // behind this dialog while the example is still being named.
    <Modal onClose={onClose} title="Save response">
      <label className="mb-1 block text-xs text-muted-foreground" htmlFor="example-name">
        Name
      </label>
      <input
        autoFocus
        className="mb-4 w-full rounded border border-border bg-input px-2 py-1 text-sm"
        id="example-name"
        onChange={(event) => setName(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            void handleSave();
          }
        }}
        value={name}
      />
      <div className="flex justify-end gap-2">
        <button
          className="rounded px-3 py-1.5 text-sm text-muted-foreground hover:bg-accent hover:text-foreground"
          onClick={onClose}
          type="button"
        >
          Cancel
        </button>
        <button
          className="rounded bg-primary px-3 py-1.5 text-sm font-medium text-primary-foreground hover:bg-primary/90 active:bg-primary/80"
          disabled={name.trim().length === 0 || saving}
          onClick={() => void handleSave()}
          type="button"
        >
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
    </Modal>
  );
}
