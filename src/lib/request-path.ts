// http_client/src/lib/request-path.ts
//
// Where a request sits, as a list of names: collection, then each folder from
// the outside in, then the request itself. The toolbar renders it as a
// breadcrumb — a label, not navigation, so this returns plain strings and no
// ids (CLAUDE.md §6: the component gets data, not a tree to walk).
import type { Folder } from "@/types/collections";

export interface RequestPathInput {
  /** null for a request that has never been saved. */
  collectionName: string | null;
  /** Every folder in that collection; the walk picks out the ancestors. */
  folders: Folder[];
  /** null when the request sits at the collection root. */
  folderId: string | null;
  requestName: string;
}

/**
 * Always ends with the request name, so the last segment is never empty and
 * the caller can rely on it being the part worth keeping when space runs out.
 *
 * An unsaved request has no collection and no folders, so the result is one
 * segment — which renders as a plain name rather than a breadcrumb of one.
 */
export function requestPathSegments(input: RequestPathInput): string[] {
  const byId = new Map(input.folders.map((folder) => [folder.id, folder]));

  // Walked child-to-parent because that is the only direction the data points,
  // then reversed. `seen` guards against a parent chain that loops: nothing
  // should ever write one, but an infinite loop in a label is a hung window.
  const ancestors: string[] = [];
  const seen = new Set<string>();
  let current = input.folderId;
  while (current !== null && !seen.has(current)) {
    seen.add(current);
    const folder = byId.get(current);
    if (!folder) {
      break;
    }
    ancestors.push(folder.name);
    current = folder.parentFolderId;
  }
  ancestors.reverse();

  const segments = input.collectionName === null ? [] : [input.collectionName];
  segments.push(...ancestors, input.requestName);
  return segments;
}
