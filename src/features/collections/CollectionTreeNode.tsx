// http_client/src/features/collections/CollectionTreeNode.tsx
//
// One level of the folder/request tree, recursive on itself for subfolders.
// Purely presentational: all state (expansion, renaming, menus, inline
// create) is owned by CollectionsSidebar and passed down through ctx.
import {
  ChevronDown,
  ChevronRight,
  FileText,
  Folder as FolderIcon,
  MessageSquare,
  Zap,
} from "lucide-react";

import { InlineTextInput } from "@/features/collections/InlineTextInput";
import type { TreeContext } from "@/features/collections/tree-types";
import type { FolderNode } from "@/lib/collection-tree";
import { methodTextColor } from "@/lib/http-method-colors";
import { statusTextColor } from "@/lib/status-colors";
import type { SavedRequest, SavedWebSocket } from "@/types/collections";

interface CollectionTreeNodeProps {
  collectionId: string;
  folders: FolderNode[];
  requests: SavedRequest[];
  webSockets: SavedWebSocket[];
  depth: number;
  parentFolderId: string | null;
  ctx: TreeContext;
}

export function CollectionTreeNode({
  collectionId,
  folders,
  requests,
  webSockets,
  depth,
  parentFolderId,
  ctx,
}: CollectionTreeNodeProps) {
  const indent = { paddingLeft: `${depth * 16 + 8}px` };
  const showCreateHere =
    ctx.creatingFolderIn !== null && ctx.creatingFolderIn.parentFolderId === parentFolderId;
  const showCreateRequestHere =
    ctx.creatingRequestIn !== null && ctx.creatingRequestIn.parentFolderId === parentFolderId;

  return (
    <>
      {folders.map((node) => {
        const isExpanded = ctx.expandedFolderIds.has(node.folder.id);
        const isRenaming = ctx.renaming?.kind === "folder" && ctx.renaming.id === node.folder.id;

        return (
          <div key={node.folder.id}>
            <div
              className="flex items-center gap-1 rounded px-1 py-1 text-sm hover:bg-accent"
              onContextMenu={(event) => {
                event.preventDefault();
                ctx.onContextMenu(event, {
                  kind: "folder",
                  id: node.folder.id,
                  collectionId,
                  name: node.folder.name,
                });
              }}
              style={indent}
            >
              <button
                aria-label={
                  isExpanded ? `Collapse ${node.folder.name}` : `Expand ${node.folder.name}`
                }
                className="shrink-0 text-muted-foreground"
                onClick={() => ctx.onToggleFolder(node.folder.id)}
                type="button"
              >
                {isExpanded ? (
                  <ChevronDown aria-hidden className="h-3.5 w-3.5" />
                ) : (
                  <ChevronRight aria-hidden className="h-3.5 w-3.5" />
                )}
              </button>
              <FolderIcon aria-hidden className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
              {isRenaming ? (
                <InlineTextInput
                  initialValue={node.folder.name}
                  onCancel={ctx.onCancelRename}
                  onCommit={(name) => ctx.onCommitRename("folder", node.folder.id, name)}
                />
              ) : (
                <button
                  className="min-w-0 flex-1 truncate text-left"
                  onClick={() => ctx.onToggleFolder(node.folder.id)}
                  type="button"
                >
                  {node.folder.name}
                </button>
              )}
            </div>

            {isExpanded && (
              <CollectionTreeNode
                collectionId={collectionId}
                ctx={ctx}
                depth={depth + 1}
                folders={node.children}
                parentFolderId={node.folder.id}
                requests={node.requests}
                webSockets={node.webSockets}
              />
            )}
          </div>
        );
      })}

      {requests.map((request) => {
        const isRenaming = ctx.renaming?.kind === "request" && ctx.renaming.id === request.id;
        const isLoaded = ctx.loadedRequestId === request.id;
        // A request is a leaf until it has a saved response under it, which
        // is the only thing that makes it expandable.
        const examples = ctx.examplesByRequestId.get(request.id) ?? [];
        const isExpanded = ctx.expandedRequestIds.has(request.id);

        return (
          <div key={request.id}>
            <div
              className={`flex items-center gap-1 rounded px-1 py-1 text-sm hover:bg-accent ${
                isLoaded ? "bg-accent/60 font-medium" : ""
              }`}
              onContextMenu={(event) => {
                event.preventDefault();
                ctx.onContextMenu(event, {
                  kind: "request",
                  id: request.id,
                  collectionId,
                  folderId: request.folderId,
                  name: request.name,
                });
              }}
              style={indent}
            >
              {examples.length > 0 ? (
                <button
                  aria-label={
                    isExpanded
                      ? `Collapse saved responses for ${request.name}`
                      : `Expand saved responses for ${request.name}`
                  }
                  className="shrink-0 text-muted-foreground"
                  onClick={() => ctx.onToggleRequest(request.id)}
                  type="button"
                >
                  {isExpanded ? (
                    <ChevronDown aria-hidden className="h-3.5 w-3.5" />
                  ) : (
                    <ChevronRight aria-hidden className="h-3.5 w-3.5" />
                  )}
                </button>
              ) : (
                <span aria-hidden className="w-3.5 shrink-0" />
              )}
              <FileText aria-hidden className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
              {isRenaming ? (
                <InlineTextInput
                  initialValue={request.name}
                  onCancel={ctx.onCancelRename}
                  onCommit={(name) => ctx.onCommitRename("request", request.id, name)}
                />
              ) : (
                <button
                  className="min-w-0 flex-1 truncate text-left"
                  onClick={() => ctx.onOpenRequest(request)}
                  type="button"
                >
                  <span
                    className={`mr-1.5 font-mono text-xs font-semibold ${methodTextColor(request.request.method)}`}
                  >
                    {request.request.method}
                  </span>
                  {request.name}
                </button>
              )}
            </div>

            {isExpanded &&
              examples.map((example) => {
                const isRenamingExample =
                  ctx.renaming?.kind === "example" && ctx.renaming.id === example.id;
                return (
                  <div
                    className="flex items-center gap-1 rounded px-1 py-1 text-sm hover:bg-accent"
                    key={example.id}
                    onContextMenu={(event) => {
                      event.preventDefault();
                      ctx.onContextMenu(event, {
                        kind: "example",
                        id: example.id,
                        collectionId,
                        name: example.name,
                      });
                    }}
                    style={{ paddingLeft: `${(depth + 1) * 16 + 8}px` }}
                  >
                    <span aria-hidden className="w-3.5 shrink-0" />
                    <MessageSquare
                      aria-hidden
                      className="h-3.5 w-3.5 shrink-0 text-muted-foreground"
                    />
                    {isRenamingExample ? (
                      <InlineTextInput
                        initialValue={example.name}
                        onCancel={ctx.onCancelRename}
                        onCommit={(name) => ctx.onCommitRename("example", example.id, name)}
                      />
                    ) : (
                      <button
                        className="min-w-0 flex-1 truncate text-left"
                        onClick={() => ctx.onOpenExample(example.id)}
                        type="button"
                      >
                        <span
                          className={`mr-1.5 font-mono text-xs font-semibold ${statusTextColor(example.status)}`}
                        >
                          {example.status}
                        </span>
                        {example.name}
                      </button>
                    )}
                  </div>
                );
              })}
          </div>
        );
      })}

      {webSockets.map((webSocket) => {
        const isRenaming = ctx.renaming?.kind === "websocket" && ctx.renaming.id === webSocket.id;
        const isLoaded = ctx.loadedRequestId === webSocket.id;

        return (
          <div
            className={`flex items-center gap-1 rounded px-1 py-1 text-sm hover:bg-accent ${
              isLoaded ? "bg-accent/60 font-medium" : ""
            }`}
            key={webSocket.id}
            onContextMenu={(event) => {
              event.preventDefault();
              ctx.onContextMenu(event, {
                kind: "websocket",
                id: webSocket.id,
                collectionId,
                folderId: webSocket.folderId,
                name: webSocket.name,
              });
            }}
            style={indent}
          >
            {/* A WebSocket has no saved responses, so never a chevron. */}
            <span aria-hidden className="w-3.5 shrink-0" />
            <Zap aria-label="WebSocket" className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
            {isRenaming ? (
              <InlineTextInput
                initialValue={webSocket.name}
                onCancel={ctx.onCancelRename}
                onCommit={(name) => ctx.onCommitRename("websocket", webSocket.id, name)}
              />
            ) : (
              <button
                className="min-w-0 flex-1 truncate text-left"
                onClick={() => ctx.onOpenWebSocket(webSocket)}
                type="button"
              >
                {webSocket.name}
              </button>
            )}
          </div>
        );
      })}

      {showCreateHere && (
        <div className="flex items-center gap-1 py-1" style={indent}>
          <span aria-hidden className="w-3.5 shrink-0" />
          <FolderIcon aria-hidden className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          <InlineTextInput
            onCancel={ctx.onCancelCreateFolder}
            onCommit={(name) => ctx.onCommitCreateFolder(parentFolderId, name)}
            placeholder="Folder name"
          />
        </div>
      )}

      {showCreateRequestHere && (
        <div className="flex items-center gap-1 py-1" style={indent}>
          <span aria-hidden className="w-3.5 shrink-0" />
          {ctx.creatingRequestIn?.protocol === "websocket" ? (
            <Zap aria-hidden className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          ) : (
            <FileText aria-hidden className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          )}
          <InlineTextInput
            onCancel={ctx.onCancelCreateRequest}
            onCommit={(name) => ctx.onCommitCreateRequest(parentFolderId, name)}
            placeholder={
              ctx.creatingRequestIn?.protocol === "websocket"
                ? "WebSocket request name"
                : "Request name"
            }
          />
        </div>
      )}
    </>
  );
}
