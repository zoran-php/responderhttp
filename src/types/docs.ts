// http_client/src/types/docs.ts
//
// Mirrors DocsTargetKind in src-tauri/src/commands/dto.rs.
//
// The three kinds are fixed by the schema: a collection, a folder and a
// saved request each carry a docs_md column. Examples are deliberately not
// documentable — an example is a record of one exchange that happened, not
// something to annotate.

/** Which kind of item some documentation belongs to. */
export type DocsTargetKind = "collection" | "folder" | "request";

/** One documentable item. Enough to fetch, save and title a Docs tab. */
export interface DocsTarget {
  kind: DocsTargetKind;
  id: string;
}

/**
 * Whether two targets point at the same item.
 *
 * Ids are unique across all three tables in practice — they carry a `col_`,
 * `fld_` or `req_` prefix — but the kind is compared anyway rather than
 * relying on that, since the prefixes are a convention of domain/ids.rs and
 * not something the schema enforces.
 */
export function sameDocsTarget(a: DocsTarget, b: DocsTarget): boolean {
  return a.kind === b.kind && a.id === b.id;
}
