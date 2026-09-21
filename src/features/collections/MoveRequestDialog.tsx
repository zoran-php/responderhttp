// http_client/src/features/collections/MoveRequestDialog.tsx
//
// "Move" without drag-and-drop (ruled out 2026-09-13, see PLAN.md): a picker
// listing every folder in the request's own collection. move_request only
// changes folder_id — it cannot move a request to a different collection.
import { useState } from "react";

import { Modal } from "@/components/Modal";
import { buildFolderTree, flattenFolders } from "@/lib/collection-tree";
import { useCollectionsStore } from "@/store/collections-store";
import { EMPTY_COLLECTION_CONTENTS } from "@/types/collections";

const ROOT_FOLDER_VALUE = "";

interface MoveRequestDialogProps {
  collectionId: string;
  requestId: string;
  requestName: string;
  currentFolderId: string | null;
  onClose: () => void;
  onMoved: () => void;
}

export function MoveRequestDialog({
  collectionId,
  requestId,
  requestName,
  currentFolderId,
  onClose,
  onMoved,
}: MoveRequestDialogProps) {
  const store = useCollectionsStore();
  const contents = store.contentsById[collectionId];
  const folders = flattenFolders(
    buildFolderTree(contents ?? EMPTY_COLLECTION_CONTENTS).rootFolders,
  );

  const [folderId, setFolderId] = useState(currentFolderId ?? ROOT_FOLDER_VALUE);
  const [busy, setBusy] = useState(false);

  async function handleSubmit() {
    setBusy(true);
    try {
      const target = folderId === ROOT_FOLDER_VALUE ? null : folderId;
      const moved = await store.moveRequest(collectionId, requestId, target);
      if (moved) {
        onMoved();
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal onClose={onClose} title={`Move "${requestName}"`}>
      <div className="space-y-3 text-sm">
        <label className="block">
          <span className="mb-1 block text-muted-foreground">Destination folder</span>
          <select
            className="h-8 w-full rounded-md border border-input bg-background px-2"
            onChange={(event) => setFolderId(event.target.value)}
            value={folderId}
          >
            <option value={ROOT_FOLDER_VALUE}>(Collection root)</option>
            {folders.map(({ folder, depth }) => (
              <option key={folder.id} value={folder.id}>
                {"— ".repeat(depth)}
                {folder.name}
              </option>
            ))}
          </select>
        </label>

        <div className="flex justify-end gap-2 pt-2">
          <button
            className="h-8 rounded-md border border-input px-3"
            disabled={busy}
            onClick={onClose}
            type="button"
          >
            Cancel
          </button>
          <button
            className="h-8 rounded-md bg-primary px-3 font-medium text-primary-foreground disabled:opacity-50"
            disabled={busy}
            onClick={() => void handleSubmit()}
            type="button"
          >
            Move
          </button>
        </div>
      </div>
    </Modal>
  );
}
