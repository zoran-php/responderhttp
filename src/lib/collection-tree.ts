// http_client/src/lib/collection-tree.ts
//
// collection_contents returns flat folders[] (with parentFolderId) and
// requests[] (with folderId); the sidebar renders a tree. Turning one into
// the other is pure logic, so it lives here rather than in the component
// (CLAUDE.md section 6) and is tested rather than eyeballed.
import type {
  CollectionContents,
  ExampleSummary,
  Folder,
  SavedRequest,
} from "@/types/collections";

export interface FolderNode {
  folder: Folder;
  children: FolderNode[];
  requests: SavedRequest[];
}

export interface FolderTree {
  /** Requests saved directly in the collection, outside any folder. */
  rootRequests: SavedRequest[];
  rootFolders: FolderNode[];
  /**
   * Examples grouped by the request they hang under, in the order storage
   * returned them (creation order). Kept beside the tree rather than folded
   * into FolderNode: examples are not part of the folder hierarchy, they are
   * children of a request wherever that request happens to sit.
   */
  examplesByRequestId: Map<string, ExampleSummary[]>;
}

export function buildFolderTree(contents: CollectionContents): FolderTree {
  const nodesById = new Map<string, FolderNode>();
  for (const folder of contents.folders) {
    nodesById.set(folder.id, { folder, children: [], requests: [] });
  }

  const rootFolders: FolderNode[] = [];
  for (const node of nodesById.values()) {
    const parentId = node.folder.parentFolderId;
    const parent = parentId ? nodesById.get(parentId) : undefined;
    if (parent) {
      parent.children.push(node);
    } else {
      rootFolders.push(node);
    }
  }

  const rootRequests: SavedRequest[] = [];
  for (const request of contents.requests) {
    const node = request.folderId ? nodesById.get(request.folderId) : undefined;
    if (node) {
      node.requests.push(request);
    } else {
      // No folder, or the folder id does not resolve (should not happen given
      // the DB's foreign keys, but surfacing the request beats losing it).
      rootRequests.push(request);
    }
  }

  sortTree(rootFolders);
  rootRequests.sort(byName);

  return { rootRequests, rootFolders, examplesByRequestId: groupExamples(contents.examples) };
}

function groupExamples(examples: ExampleSummary[]): Map<string, ExampleSummary[]> {
  const byRequest = new Map<string, ExampleSummary[]>();
  for (const example of examples) {
    const existing = byRequest.get(example.requestId);
    if (existing) {
      existing.push(example);
    } else {
      byRequest.set(example.requestId, [example]);
    }
  }
  return byRequest;
}

export interface FlatFolder {
  folder: Folder;
  depth: number;
}

/** Depth-first listing for a picker (Save / Move dialogs) that needs to show
 * nesting with indentation rather than a real tree widget. */
export function flattenFolders(nodes: FolderNode[], depth = 0): FlatFolder[] {
  const result: FlatFolder[] = [];
  for (const node of nodes) {
    result.push({ folder: node.folder, depth });
    result.push(...flattenFolders(node.children, depth + 1));
  }
  return result;
}

function sortTree(nodes: FolderNode[]): void {
  nodes.sort((a, b) => byName(a.folder, b.folder));
  for (const node of nodes) {
    node.requests.sort(byName);
    sortTree(node.children);
  }
}

function byName(a: { name: string }, b: { name: string }): number {
  return a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
}
