// http_client/src/features/collections/SaveRequestDialog.tsx
//
// Shown when the current request has never been saved. An already-saved
// request overwrites directly (see App.tsx) — this dialog only handles the
// "new" half of "Save (new + overwrite)". New-folder creation here is
// root-level only (a <select> cannot host an inline "create inside X" flow);
// nested folders can still be picked as a target, and can be created via the
// sidebar's own "New folder" context-menu item.
//
// It saves HTTP and WebSocket requests alike, into any collection and any
// folder: nothing here is filtered, hidden or refused by protocol
// (PLAN-WEBSOCKET.md spec section 3), because a collection is a place to put
// requests, not a statement about which kind they are.
import { useEffect, useState } from "react";

import { Modal } from "@/components/Modal";
import { buildFolderTree, flattenFolders } from "@/lib/collection-tree";
import { useCollectionsStore } from "@/store/collections-store";
import type { WebSocketShape } from "@/lib/ws-request";
import { EMPTY_COLLECTION_CONTENTS } from "@/types/collections";
import type { SendRequestInput } from "@/types/http";

/** What is being saved, as its tab holds it. */
export type SavePayload =
  { kind: "http"; request: SendRequestInput } | { kind: "websocket"; shape: WebSocketShape };

/** Where the saved request now lives, whichever kind it is. */
export interface SavedLocation {
  id: string;
  collectionId: string;
  folderId: string | null;
  name: string;
}

const NEW_COLLECTION_VALUE = "__new_collection__";
const NEW_FOLDER_VALUE = "__new_folder__";
const ROOT_FOLDER_VALUE = "";

interface SaveRequestDialogProps {
  payload: SavePayload;
  defaultName: string;
  onClose: () => void;
  onSaved: (saved: SavedLocation) => void;
}

export function SaveRequestDialog({
  payload,
  defaultName,
  onClose,
  onSaved,
}: SaveRequestDialogProps) {
  const store = useCollectionsStore();
  // See the matching comment in CollectionsSidebar.tsx: these two are read
  // via a selector, not off `store`, so they're stable across renders and
  // safe to put in an effect's dependency array.
  const loadCollections = useCollectionsStore((state) => state.loadCollections);
  const refreshContents = useCollectionsStore((state) => state.refreshContents);

  const [name, setName] = useState(defaultName);
  const [collectionId, setCollectionId] = useState("");
  const [newCollectionName, setNewCollectionName] = useState("");
  const [folderId, setFolderId] = useState(ROOT_FOLDER_VALUE);
  const [newFolderName, setNewFolderName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void loadCollections();
  }, [loadCollections]);

  useEffect(() => {
    if (collectionId && collectionId !== NEW_COLLECTION_VALUE) {
      void refreshContents(collectionId);
    }
  }, [collectionId, refreshContents]);

  const folders =
    collectionId && collectionId !== NEW_COLLECTION_VALUE
      ? flattenFolders(
          buildFolderTree(store.contentsById[collectionId] ?? EMPTY_COLLECTION_CONTENTS)
            .rootFolders,
        )
      : [];

  async function handleSubmit() {
    setError(null);
    const trimmedName = name.trim();
    if (trimmedName === "") {
      setError("Name cannot be empty.");
      return;
    }

    setBusy(true);
    try {
      let targetCollectionId = collectionId;
      if (collectionId === NEW_COLLECTION_VALUE) {
        const trimmedCollection = newCollectionName.trim();
        if (trimmedCollection === "") {
          setError("Collection name cannot be empty.");
          return;
        }
        const created = await store.createCollection(trimmedCollection);
        if (!created) {
          setError("Could not create the collection.");
          return;
        }
        targetCollectionId = created.id;
      }
      if (!targetCollectionId) {
        setError("Choose a collection.");
        return;
      }

      let targetFolderId: string | null = folderId === ROOT_FOLDER_VALUE ? null : folderId;
      if (folderId === NEW_FOLDER_VALUE) {
        const trimmedFolder = newFolderName.trim();
        if (trimmedFolder === "") {
          setError("Folder name cannot be empty.");
          return;
        }
        const created = await store.createFolder(targetCollectionId, null, trimmedFolder);
        if (!created) {
          setError("Could not create the folder.");
          return;
        }
        targetFolderId = created.id;
      }

      const location = {
        id: null,
        collectionId: targetCollectionId,
        folderId: targetFolderId,
        name: trimmedName,
      };
      const saved =
        payload.kind === "http"
          ? await store.saveRequest({ ...location, request: payload.request })
          : await store.saveWebSocket({
              ...location,
              request: payload.shape.request,
              draft: payload.shape.draft,
            });
      if (!saved) {
        setError("Could not save the request.");
        return;
      }
      onSaved(saved);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      onClose={onClose}
      title={payload.kind === "http" ? "Save request" : "Save WebSocket request"}
    >
      <div className="space-y-3 text-sm">
        <label className="block">
          <span className="mb-1 block text-muted-foreground">Name</span>
          <input
            autoFocus
            className="h-8 w-full rounded-md border border-input bg-background px-2"
            onChange={(event) => setName(event.target.value)}
            value={name}
          />
        </label>

        <label className="block">
          <span className="mb-1 block text-muted-foreground">Collection</span>
          <select
            className="h-8 w-full rounded-md border border-input bg-background px-2"
            onChange={(event) => {
              setCollectionId(event.target.value);
              setFolderId(ROOT_FOLDER_VALUE);
            }}
            value={collectionId}
          >
            <option disabled value="">
              Choose a collection…
            </option>
            {store.collections.map((collection) => (
              <option key={collection.id} value={collection.id}>
                {collection.name}
              </option>
            ))}
            <option value={NEW_COLLECTION_VALUE}>+ New collection…</option>
          </select>
        </label>

        {collectionId === NEW_COLLECTION_VALUE && (
          <input
            autoFocus
            className="h-8 w-full rounded-md border border-input bg-background px-2"
            onChange={(event) => setNewCollectionName(event.target.value)}
            placeholder="New collection name"
            value={newCollectionName}
          />
        )}

        {collectionId && collectionId !== NEW_COLLECTION_VALUE && (
          <label className="block">
            <span className="mb-1 block text-muted-foreground">Folder</span>
            <select
              className="h-8 w-full rounded-md border border-input bg-background px-2"
              onChange={(event) => setFolderId(event.target.value)}
              value={folderId}
            >
              <option value={ROOT_FOLDER_VALUE}>(No folder)</option>
              {folders.map(({ folder, depth }) => (
                <option key={folder.id} value={folder.id}>
                  {"— ".repeat(depth)}
                  {folder.name}
                </option>
              ))}
              <option value={NEW_FOLDER_VALUE}>+ New folder…</option>
            </select>
          </label>
        )}

        {folderId === NEW_FOLDER_VALUE && (
          <input
            autoFocus
            className="h-8 w-full rounded-md border border-input bg-background px-2"
            onChange={(event) => setNewFolderName(event.target.value)}
            placeholder="New folder name"
            value={newFolderName}
          />
        )}

        {error && <p className="text-xs text-destructive">{error}</p>}

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
            Save
          </button>
        </div>
      </div>
    </Modal>
  );
}
