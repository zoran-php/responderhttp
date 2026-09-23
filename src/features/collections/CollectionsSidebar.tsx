// http_client/src/features/collections/CollectionsSidebar.tsx
//
// The Phase 3 sidebar: a tree of collections > folders > requests. All
// ephemeral UI state (which menu, dialog or inline editor is open) lives
// here as local state; persistence goes through store/collections-store.ts,
// the only thing that calls services/collections.ts for this feature.
import { useEffect, useState, type MouseEvent } from "react";
import {
  ChevronDown,
  ChevronRight,
  Copy,
  FileInput,
  FileOutput,
  FileText,
  FolderInput,
  FolderPlus,
  Pencil,
  Plus,
  Trash2,
  Zap,
} from "lucide-react";

import { ConfirmDialog } from "@/components/ConfirmDialog";
import { CollectionTreeNode } from "@/features/collections/CollectionTreeNode";
import { ContextMenu, type ContextMenuItem } from "@/features/collections/ContextMenu";
import { ExportOpenApiDialog } from "@/features/collections/ExportOpenApiDialog";
import { ImportOpenApiDialog } from "@/features/collections/ImportOpenApiDialog";
import { InlineTextInput } from "@/features/collections/InlineTextInput";
import { MoveRequestDialog } from "@/features/collections/MoveRequestDialog";
import type {
  NewRequestProtocol,
  RenamingTarget,
  TreeContext,
  TreeKind,
  TreeTarget,
} from "@/features/collections/tree-types";
import { buildFolderTree } from "@/lib/collection-tree";
import { useCollectionsStore } from "@/store/collections-store";
import { useEnvironmentsStore } from "@/store/environments-store";
import type { SavedRequest, SavedWebSocket } from "@/types/collections";
import type { DocsTarget } from "@/types/docs";
import type { OpenApiImportResult } from "@/types/openapi-import";

interface CollectionsSidebarProps {
  loadedRequestId: string | null;
  onOpenRequest: (request: SavedRequest) => void;
  onOpenWebSocket: (webSocket: SavedWebSocket) => void;
  onOpenExample: (exampleId: string) => void;
  onOpenDocs: (target: DocsTarget) => void;
}

interface MenuState {
  x: number;
  y: number;
  target: TreeTarget;
}

interface DeleteTarget {
  kind: TreeKind;
  id: string;
  collectionId: string;
  name: string;
}

interface CreatingFolder {
  collectionId: string;
  parentFolderId: string | null;
}

interface CreatingRequest {
  collectionId: string;
  parentFolderId: string | null;
  protocol: NewRequestProtocol;
}

interface ExportTarget {
  collectionId: string;
  collectionName: string;
}

interface MoveTarget {
  collectionId: string;
  requestId: string;
  requestName: string;
  currentFolderId: string | null;
}

export function CollectionsSidebar({
  loadedRequestId,
  onOpenRequest,
  onOpenWebSocket,
  onOpenExample,
  onOpenDocs,
}: CollectionsSidebarProps) {
  const store = useCollectionsStore();
  // Selected separately from `store` above: useCollectionsStore() with no
  // selector returns a new object on every state change, so putting
  // store.loadCollections straight in an effect's deps would fire this
  // effect (and re-fetch) on every unrelated store update. The action
  // itself is a stable reference for the store's lifetime, which a
  // selector — but not a property read off the whole-store object — gives
  // eslint (and this effect) a safe dependency to key off.
  const loadCollections = useCollectionsStore((state) => state.loadCollections);
  const loadEnvironments = useEnvironmentsStore((state) => state.loadEnvironments);

  const [menu, setMenu] = useState<MenuState | null>(null);
  const [renaming, setRenaming] = useState<RenamingTarget>(null);
  const [creatingFolder, setCreatingFolder] = useState<CreatingFolder | null>(null);
  const [creatingRequestIn, setCreatingRequestIn] = useState<CreatingRequest | null>(null);
  const [creatingCollection, setCreatingCollection] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<DeleteTarget | null>(null);
  const [moveTarget, setMoveTarget] = useState<MoveTarget | null>(null);
  const [exportTarget, setExportTarget] = useState<ExportTarget | null>(null);
  const [importing, setImporting] = useState(false);

  useEffect(() => {
    void loadCollections();
  }, [loadCollections]);

  function openMenu(event: MouseEvent, target: TreeTarget) {
    setMenu({ x: event.clientX, y: event.clientY, target });
  }

  async function handleImported(result: OpenApiImportResult) {
    await loadCollections();
    store.expandCollection(result.collectionId);
    if (result.environmentId !== null) {
      await loadEnvironments();
    }
  }

  async function handleOpenRequest(request: SavedRequest) {
    // Re-fetches by id rather than trusting the tree's cached copy, so a
    // stale collection_contents snapshot can't hand the builder stale data.
    const fresh = await store.loadRequest(request.id);
    if (fresh) {
      onOpenRequest(fresh);
    }
  }

  async function handleOpenWebSocket(webSocket: SavedWebSocket) {
    // Re-fetched by id for the same reason as handleOpenRequest.
    const fresh = await store.loadWebSocket(webSocket.id);
    if (fresh) {
      onOpenWebSocket(fresh);
    }
  }

  async function handleCommitRename(
    kind: TreeKind,
    id: string,
    name: string,
    collectionId: string,
  ) {
    setRenaming(null);
    if (kind === "collection") {
      await store.renameCollection(id, name);
    } else if (kind === "folder") {
      await store.renameFolder(collectionId, id, name);
    } else if (kind === "example") {
      await store.renameExample(collectionId, id, name);
    } else {
      await store.renameRequest(collectionId, id, name);
    }
  }

  async function handleCommitCreateFolder(
    collectionId: string,
    parentFolderId: string | null,
    name: string,
  ) {
    setCreatingFolder(null);
    await store.createFolder(collectionId, parentFolderId, name);
  }

  async function handleCommitCreateCollection(name: string) {
    setCreatingCollection(false);
    await store.createCollection(name);
  }

  async function handleCommitCreateRequest(
    collectionId: string,
    parentFolderId: string | null,
    name: string,
    protocol: NewRequestProtocol,
  ) {
    setCreatingRequestIn(null);
    if (protocol === "websocket") {
      const created = await store.createWebSocket(collectionId, parentFolderId, name);
      if (created) {
        onOpenWebSocket(created);
      }
      return;
    }
    // saveRequest's response is already the fresh row, so this opens it
    // directly rather than going through handleOpenRequest's re-fetch,
    // which exists for stale cached data — not a concern for a row we just
    // created ourselves.
    const saved = await store.createRequest(collectionId, parentFolderId, name);
    if (saved) {
      onOpenRequest(saved);
    }
  }

  async function handleDuplicateRequest(id: string, collectionId: string, name: string) {
    const saved = await store.duplicateRequest(collectionId, id, `${name} copy`);
    if (saved) {
      onOpenRequest(saved);
    }
  }

  async function handleConfirmDelete() {
    if (!deleteTarget) {
      return;
    }
    const { kind, id, collectionId } = deleteTarget;
    setDeleteTarget(null);
    if (kind === "collection") {
      await store.deleteCollection(id);
    } else if (kind === "folder") {
      await store.deleteFolder(collectionId, id);
    } else if (kind === "example") {
      await store.deleteExample(collectionId, id);
    } else {
      await store.deleteRequest(collectionId, id);
    }
  }

  function menuItems(target: TreeTarget): ContextMenuItem[] {
    if (target.kind === "collection") {
      return [
        {
          label: "New HTTP request",
          icon: Plus,
          // The inline input renders among the collection's children, so a
          // collapsed collection would swallow it.
          onSelect: () => {
            store.expandCollection(target.id);
            setCreatingRequestIn({
              collectionId: target.id,
              parentFolderId: null,
              protocol: "http",
            });
          },
        },
        {
          label: "New WebSocket request",
          icon: Zap,
          onSelect: () => {
            store.expandCollection(target.id);
            setCreatingRequestIn({
              collectionId: target.id,
              parentFolderId: null,
              protocol: "websocket",
            });
          },
        },
        {
          label: "New folder",
          icon: FolderPlus,
          onSelect: () => {
            store.expandCollection(target.id);
            setCreatingFolder({ collectionId: target.id, parentFolderId: null });
          },
        },
        {
          label: "Rename",
          icon: Pencil,
          onSelect: () => setRenaming({ kind: "collection", id: target.id }),
        },
        {
          label: "Docs",
          icon: FileText,
          onSelect: () => onOpenDocs({ kind: "collection", id: target.id }),
        },
        {
          label: "Export as OpenAPI",
          icon: FileOutput,
          onSelect: () => setExportTarget({ collectionId: target.id, collectionName: target.name }),
        },
        {
          label: "Delete",
          icon: Trash2,
          destructive: true,
          onSelect: () =>
            setDeleteTarget({
              kind: "collection",
              id: target.id,
              collectionId: target.id,
              name: target.name,
            }),
        },
      ];
    }
    if (target.kind === "folder") {
      return [
        {
          label: "New HTTP request",
          icon: Plus,
          onSelect: () => {
            store.expandCollection(target.collectionId);
            store.expandFolder(target.id);
            setCreatingRequestIn({
              collectionId: target.collectionId,
              parentFolderId: target.id,
              protocol: "http",
            });
          },
        },
        {
          label: "New WebSocket request",
          icon: Zap,
          onSelect: () => {
            store.expandCollection(target.collectionId);
            store.expandFolder(target.id);
            setCreatingRequestIn({
              collectionId: target.collectionId,
              parentFolderId: target.id,
              protocol: "websocket",
            });
          },
        },
        {
          label: "New folder",
          icon: FolderPlus,
          onSelect: () => {
            store.expandCollection(target.collectionId);
            store.expandFolder(target.id);
            setCreatingFolder({ collectionId: target.collectionId, parentFolderId: target.id });
          },
        },
        {
          label: "Rename",
          icon: Pencil,
          onSelect: () => setRenaming({ kind: "folder", id: target.id }),
        },
        {
          label: "Docs",
          icon: FileText,
          onSelect: () => onOpenDocs({ kind: "folder", id: target.id }),
        },
        {
          label: "Delete",
          icon: Trash2,
          destructive: true,
          onSelect: () =>
            setDeleteTarget({
              kind: "folder",
              id: target.id,
              collectionId: target.collectionId,
              name: target.name,
            }),
        },
      ];
    }
    if (target.kind === "websocket") {
      // Rename, Docs, Move and Delete act on the row by id, whatever its
      // kind. No Duplicate yet: it would need its own load-and-save path,
      // and nothing asked for it.
      return [
        {
          label: "Rename",
          icon: Pencil,
          onSelect: () => setRenaming({ kind: "websocket", id: target.id }),
        },
        {
          label: "Docs",
          icon: FileText,
          onSelect: () => onOpenDocs({ kind: "request", id: target.id }),
        },
        {
          label: "Move",
          icon: FolderInput,
          onSelect: () =>
            setMoveTarget({
              collectionId: target.collectionId,
              requestId: target.id,
              requestName: target.name,
              currentFolderId: target.folderId,
            }),
        },
        {
          label: "Delete",
          icon: Trash2,
          destructive: true,
          onSelect: () =>
            setDeleteTarget({
              kind: "websocket",
              id: target.id,
              collectionId: target.collectionId,
              name: target.name,
            }),
        },
      ];
    }
    if (target.kind === "example") {
      return [
        {
          label: "Rename",
          icon: Pencil,
          onSelect: () => setRenaming({ kind: "example", id: target.id }),
        },
        {
          label: "Delete",
          icon: Trash2,
          destructive: true,
          onSelect: () =>
            setDeleteTarget({
              kind: "example",
              id: target.id,
              collectionId: target.collectionId,
              name: target.name,
            }),
        },
      ];
    }
    return [
      {
        label: "Rename",
        icon: Pencil,
        onSelect: () => setRenaming({ kind: "request", id: target.id }),
      },
      {
        label: "Docs",
        icon: FileText,
        onSelect: () => onOpenDocs({ kind: "request", id: target.id }),
      },
      {
        label: "Duplicate",
        icon: Copy,
        onSelect: () => void handleDuplicateRequest(target.id, target.collectionId, target.name),
      },
      {
        label: "Move",
        icon: FolderInput,
        onSelect: () =>
          setMoveTarget({
            collectionId: target.collectionId,
            requestId: target.id,
            requestName: target.name,
            currentFolderId: target.folderId,
          }),
      },
      {
        label: "Delete",
        icon: Trash2,
        destructive: true,
        onSelect: () =>
          setDeleteTarget({
            kind: "request",
            id: target.id,
            collectionId: target.collectionId,
            name: target.name,
          }),
      },
    ];
  }

  function buildCtx(
    collectionId: string,
    examplesByRequestId: TreeContext["examplesByRequestId"],
  ): TreeContext {
    return {
      expandedFolderIds: store.expandedFolderIds,
      onToggleFolder: store.toggleFolder,
      onOpenRequest: (request) => void handleOpenRequest(request),
      onOpenWebSocket: (webSocket) => void handleOpenWebSocket(webSocket),
      loadedRequestId,
      examplesByRequestId,
      expandedRequestIds: store.expandedRequestIds,
      onToggleRequest: store.toggleRequest,
      onOpenExample,
      renaming,
      onCommitRename: (kind, id, name) => void handleCommitRename(kind, id, name, collectionId),
      onCancelRename: () => setRenaming(null),
      onContextMenu: openMenu,
      creatingFolderIn:
        creatingFolder && creatingFolder.collectionId === collectionId
          ? { parentFolderId: creatingFolder.parentFolderId }
          : null,
      onCommitCreateFolder: (parentFolderId, name) =>
        void handleCommitCreateFolder(collectionId, parentFolderId, name),
      onCancelCreateFolder: () => setCreatingFolder(null),
      creatingRequestIn:
        creatingRequestIn && creatingRequestIn.collectionId === collectionId
          ? {
              parentFolderId: creatingRequestIn.parentFolderId,
              protocol: creatingRequestIn.protocol,
            }
          : null,
      onCommitCreateRequest: (parentFolderId, name) =>
        void handleCommitCreateRequest(
          collectionId,
          parentFolderId,
          name,
          creatingRequestIn?.protocol ?? "http",
        ),
      onCancelCreateRequest: () => setCreatingRequestIn(null),
    };
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Two equal halves. Sized to fit the 256px minimum sidebar width at
          11px — the size the panel tabs above already use; the label
          truncates rather than wraps if a font runs wider, and the title
          still says it in full. */}
      <div className="grid grid-cols-2 gap-1 border-b border-border px-1 py-2">
        <button
          className="flex min-w-0 items-center justify-center gap-1 rounded px-1.5 py-1 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground"
          onClick={() => setImporting(true)}
          title="Import Collection from an OpenAPI file (JSON or YAML)"
          type="button"
        >
          <FileInput aria-hidden className="h-3.5 w-3.5 shrink-0" />
          <span className="truncate">Import Collection</span>
        </button>
        <button
          className="flex min-w-0 items-center justify-center gap-1 rounded px-1.5 py-1 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground"
          onClick={() => setCreatingCollection(true)}
          title="New Collection"
          type="button"
        >
          <Plus aria-hidden className="h-3.5 w-3.5 shrink-0" />
          <span className="truncate">New Collection</span>
        </button>
      </div>

      {store.error && (
        <div className="flex items-start justify-between gap-2 border-b border-border bg-destructive/10 px-3 py-2 text-xs text-destructive">
          <span>{store.error.message}</span>
          <button onClick={store.clearError} type="button">
            Dismiss
          </button>
        </div>
      )}

      <div className="flex-1 overflow-auto p-1">
        {creatingCollection && (
          <div className="flex items-center gap-1 px-1 py-1">
            <FolderPlus aria-hidden className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
            <InlineTextInput
              onCancel={() => setCreatingCollection(false)}
              onCommit={(name) => void handleCommitCreateCollection(name)}
              placeholder="Collection name"
            />
          </div>
        )}

        {store.collections.map((collection) => {
          const isExpanded = store.expandedCollectionIds.has(collection.id);
          const isRenaming = renaming?.kind === "collection" && renaming.id === collection.id;
          const contents = store.contentsById[collection.id];
          const tree = contents ? buildFolderTree(contents) : null;

          return (
            <div key={collection.id}>
              <div
                className="flex items-center gap-1 rounded px-1 py-1 text-sm font-medium hover:bg-accent"
                onContextMenu={(event) => {
                  event.preventDefault();
                  openMenu(event, { kind: "collection", id: collection.id, name: collection.name });
                }}
              >
                <button
                  aria-label={
                    isExpanded ? `Collapse ${collection.name}` : `Expand ${collection.name}`
                  }
                  className="shrink-0 text-muted-foreground"
                  onClick={() => store.toggleCollection(collection.id)}
                  type="button"
                >
                  {isExpanded ? (
                    <ChevronDown aria-hidden className="h-3.5 w-3.5" />
                  ) : (
                    <ChevronRight aria-hidden className="h-3.5 w-3.5" />
                  )}
                </button>
                {isRenaming ? (
                  <InlineTextInput
                    initialValue={collection.name}
                    onCancel={() => setRenaming(null)}
                    onCommit={(name) =>
                      void handleCommitRename("collection", collection.id, name, collection.id)
                    }
                  />
                ) : (
                  <button
                    className="min-w-0 flex-1 truncate text-left"
                    onClick={() => store.toggleCollection(collection.id)}
                    type="button"
                  >
                    {collection.name}
                  </button>
                )}
              </div>

              {isExpanded &&
                (tree ? (
                  <CollectionTreeNode
                    collectionId={collection.id}
                    ctx={buildCtx(collection.id, tree.examplesByRequestId)}
                    depth={1}
                    folders={tree.rootFolders}
                    parentFolderId={null}
                    requests={tree.rootRequests}
                    webSockets={tree.rootWebSockets}
                  />
                ) : (
                  <p className="px-6 py-1 text-xs text-muted-foreground">Loading…</p>
                ))}
            </div>
          );
        })}

        {store.collections.length === 0 && !creatingCollection && (
          <p className="px-2 py-4 text-center text-xs text-muted-foreground">
            No collections yet. Create or import one above.
          </p>
        )}
      </div>

      {menu && (
        <ContextMenu
          items={menuItems(menu.target)}
          onClose={() => setMenu(null)}
          x={menu.x}
          y={menu.y}
        />
      )}

      {deleteTarget && (
        <ConfirmDialog
          message={deleteMessage(deleteTarget)}
          onCancel={() => setDeleteTarget(null)}
          onConfirm={() => void handleConfirmDelete()}
          title={`Delete ${DELETE_TITLE_NOUN[deleteTarget.kind]}`}
        />
      )}

      {exportTarget && (
        <ExportOpenApiDialog
          collectionId={exportTarget.collectionId}
          collectionName={exportTarget.collectionName}
          onClose={() => setExportTarget(null)}
        />
      )}

      {importing && (
        <ImportOpenApiDialog
          onClose={() => setImporting(false)}
          onImported={(result) => void handleImported(result)}
        />
      )}

      {moveTarget && (
        <MoveRequestDialog
          collectionId={moveTarget.collectionId}
          currentFolderId={moveTarget.currentFolderId}
          onClose={() => setMoveTarget(null)}
          onMoved={() => setMoveTarget(null)}
          requestId={moveTarget.requestId}
          requestName={moveTarget.requestName}
        />
      )}
    </div>
  );
}

/** A request now carries its saved responses down with it, which the prompt
 * has to say — "cannot be undone" reads very differently once there is
 * something underneath. */
const DELETE_TITLE_NOUN: Record<TreeKind, string> = {
  collection: "collection",
  folder: "folder",
  request: "request",
  example: "example",
  websocket: "WebSocket request",
};

function deleteMessage(target: DeleteTarget): string {
  // A WebSocket request has no saved responses under it, so the plain
  // prompt is the truthful one.
  if (target.kind === "example" || target.kind === "websocket") {
    return `Delete "${target.name}"? This cannot be undone.`;
  }
  if (target.kind === "request") {
    return `Delete "${target.name}"? Any responses saved under it go too. This cannot be undone.`;
  }
  return `Delete "${target.name}"? This also deletes everything inside it.`;
}
