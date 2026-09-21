// http_client/src/services/docs.ts
//
// One function per command in src-tauri/src/commands/docs.rs. Like every
// other service here, the boundary crossing goes through services/invoke.ts
// so invoke() appears in one place (CLAUDE.md section 11, rule 3).
import { call } from "@/services/invoke";
import type { DocsTarget } from "@/types/docs";
import type { ApiError } from "@/types/http";
import type { Result } from "@/types/result";

/** An item with no documentation returns an empty string, not a failure. */
export function itemDocs(target: DocsTarget): Promise<Result<string, ApiError>> {
  return call<string>("item_docs", { kind: target.kind, id: target.id });
}

/**
 * Replaces an item's documentation. An empty string is how the user clears
 * it, so it is an ordinary save rather than a delete.
 *
 * Fails with `invalidRequest` when the text is past the backend's cap, which
 * the editor surfaces rather than swallowing (CLAUDE.md section 7).
 */
export function setItemDocs(target: DocsTarget, markdown: string): Promise<Result<void, ApiError>> {
  return call<void>("set_item_docs", { kind: target.kind, id: target.id, markdown });
}
