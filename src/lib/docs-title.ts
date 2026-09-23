// http_client/src/lib/docs-title.ts
//
// What a Docs tab is called (PLAN.md Phase 12).
//
// A Docs tab holds only the item it documents, not the item's name, so that
// renaming a collection in the sidebar retitles its Docs tab without anything
// having to be kept in step. The price is that the name has to be looked up,
// and the lookup is here rather than in the component: it is a search across
// two stores with a fallback, which is a rule, not rendering (CLAUDE.md
// section 6).
import type { Collection, CollectionContents } from "@/types/collections";
import type { DocsTarget } from "@/types/docs";

/**
 * Shown until the item's name is known — while its collection's contents are
 * still loading, or if the item has since been deleted from under the tab.
 */
export const UNNAMED_DOCS_ITEM = "Untitled";

/**
 * The name of the item a Docs tab documents, or null if it cannot be found.
 *
 * Folders and requests are searched across every loaded collection because a
 * DocsTarget carries no collection id: it does not need one to fetch or save
 * (ids are unique), and adding one would mean keeping it correct when a
 * request is moved between collections.
 */
export function docsTargetName(
  target: DocsTarget,
  collections: Collection[],
  contentsById: Record<string, CollectionContents>,
): string | null {
  if (target.kind === "collection") {
    return collections.find((collection) => collection.id === target.id)?.name ?? null;
  }

  for (const contents of Object.values(contentsById)) {
    // A WebSocket request is a "request" target too: it shares the requests
    // table, and its docs are reached by the same id.
    const items: readonly { id: string; name: string }[] =
      target.kind === "folder" ? contents.folders : [...contents.requests, ...contents.webSockets];
    const found = items.find((item) => item.id === target.id);
    if (found !== undefined) {
      return found.name;
    }
  }
  return null;
}

/** The tab strip's label, as the spec writes it: `Docs: Auth Service`. */
export function docsTabLabel(name: string | null): string {
  return `Docs: ${name ?? UNNAMED_DOCS_ITEM}`;
}
