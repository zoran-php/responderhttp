// http_client/src/types/collections.ts
//
// Mirrors the collection/folder/request DTOs in src-tauri/src/commands/dto.rs.
// SavedRequest reuses SendRequestInput from types/http.ts rather than
// duplicating the request shape a second time.
import type { KeyValue, SecretState, SendRequestInput } from "@/types/http";
import type { WebSocketRequest, WsDraft } from "@/types/websocket";

export interface Collection {
  id: string;
  name: string;
}

export interface Folder {
  id: string;
  collectionId: string;
  parentFolderId: string | null;
  name: string;
}

export interface SavedRequest {
  id: string;
  collectionId: string;
  folderId: string | null;
  name: string;
  request: SendRequestInput;
  /** Whether request.auth's secret could be read. When it could not, the
   * field is empty and the Auth tab asks for it again. */
  secretState: SecretState;
}

/**
 * A saved WebSocket request. Mirrors SavedWebSocketDto.
 *
 * It lives in the same collections and folders as SavedRequest, and shares
 * its id space: rename, move, delete and docs reach it through the request
 * commands by id.
 */
export interface SavedWebSocket {
  id: string;
  collectionId: string;
  folderId: string | null;
  name: string;
  request: WebSocketRequest;
  draft: WsDraft;
}

/**
 * A saved response kept under the request that produced it. Mirrors ExampleDto.
 *
 * `request` is the request **as sent**: already resolved, unlike SavedRequest
 * and HistoryEntry, which keep their {{placeholders}} so they can be re-run
 * against another environment. An example records one exchange that happened.
 */
export interface Example {
  id: string;
  requestId: string;
  name: string;
  createdAt: string;
  request: SendRequestInput;
  status: number;
  responseHeaders: KeyValue[];
  /** Text only — a binary response has nothing to store. */
  responseBody: string;
}

/** What the tree needs to draw an example, without pulling its body along. */
export interface ExampleSummary {
  id: string;
  requestId: string;
  name: string;
  status: number;
}

/** One collection's folders, requests, example summaries and WebSocket
 * requests — what collection_contents returns. */
export interface CollectionContents {
  folders: Folder[];
  requests: SavedRequest[];
  examples: ExampleSummary[];
  webSockets: SavedWebSocket[];
}

/**
 * What to render before a collection's contents have loaded. A constant
 * rather than an inline literal at each call site: those had to be corrected
 * by hand every time CollectionContents gained a field, and did not get
 * corrected when `examples` was added.
 */
export const EMPTY_COLLECTION_CONTENTS: CollectionContents = {
  folders: [],
  requests: [],
  examples: [],
  webSockets: [],
};
