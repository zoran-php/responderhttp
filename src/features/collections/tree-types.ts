// http_client/src/features/collections/tree-types.ts
//
// Shared shape between CollectionsSidebar (owns all the ephemeral state) and
// CollectionTreeNode (renders it), kept in its own file so neither component
// has to import the other's props.
import type { MouseEvent } from "react";

import type { ExampleSummary, SavedRequest } from "@/types/collections";

export type TreeTarget =
  | { kind: "collection"; id: string; name: string }
  | { kind: "folder"; id: string; collectionId: string; name: string }
  | { kind: "request"; id: string; collectionId: string; folderId: string | null; name: string }
  | { kind: "example"; id: string; collectionId: string; name: string };

export type TreeKind = TreeTarget["kind"];

export type RenamingTarget = { kind: TreeKind; id: string } | null;

export interface TreeContext {
  expandedFolderIds: Set<string>;
  onToggleFolder: (id: string) => void;
  onOpenRequest: (request: SavedRequest) => void;
  loadedRequestId: string | null;
  /** Keyed by request id; a request with no entry here renders as a leaf. */
  examplesByRequestId: Map<string, ExampleSummary[]>;
  expandedRequestIds: Set<string>;
  onToggleRequest: (id: string) => void;
  onOpenExample: (id: string) => void;
  renaming: RenamingTarget;
  onCommitRename: (kind: "folder" | "request" | "example", id: string, name: string) => void;
  onCancelRename: () => void;
  onContextMenu: (event: MouseEvent, target: TreeTarget) => void;
  /** Set when a "new folder" inline input should render as a child of the
   * folder (or collection root, when null) currently being rendered. */
  creatingFolderIn: { parentFolderId: string | null } | null;
  onCommitCreateFolder: (parentFolderId: string | null, name: string) => void;
  onCancelCreateFolder: () => void;
  /** Same shape as creatingFolderIn, for the "new request" inline input. */
  creatingRequestIn: { parentFolderId: string | null } | null;
  onCommitCreateRequest: (parentFolderId: string | null, name: string) => void;
  onCancelCreateRequest: () => void;
}
