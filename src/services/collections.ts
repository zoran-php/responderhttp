// http_client/src/services/collections.ts
//
// One function per command in src-tauri/src/commands/collections.rs. The
// boundary crossing itself lives in services/invoke.ts, which is the only
// place invoke() appears for these (CLAUDE.md section 11, rule 3) — see
// src/types/collections.ts for the DTOs and the note in types/http.ts about
// ApiError's notFound/storage kinds, which these commands are the ones that
// actually produce.
import { call } from "@/services/invoke";
import type {
  Collection,
  CollectionContents,
  Example,
  Folder,
  SavedRequest,
  SavedWebSocket,
} from "@/types/collections";
import type { ApiError, KeyValue, SendRequestInput } from "@/types/http";
import type { Result } from "@/types/result";
import type { WebSocketRequest, WsDraft } from "@/types/websocket";

export function listCollections(): Promise<Result<Collection[], ApiError>> {
  return call<Collection[]>("list_collections");
}

export function collectionContents(
  collectionId: string,
): Promise<Result<CollectionContents, ApiError>> {
  return call<CollectionContents>("collection_contents", { collectionId });
}

export function createCollection(name: string): Promise<Result<Collection, ApiError>> {
  return call<Collection>("create_collection", { name });
}

export function renameCollection(id: string, name: string): Promise<Result<void, ApiError>> {
  return call<void>("rename_collection", { id, name });
}

export function deleteCollection(id: string): Promise<Result<void, ApiError>> {
  return call<void>("delete_collection", { id });
}

export function createFolder(
  collectionId: string,
  parentFolderId: string | null,
  name: string,
): Promise<Result<Folder, ApiError>> {
  return call<Folder>("create_folder", { collectionId, parentFolderId, name });
}

export function renameFolder(id: string, name: string): Promise<Result<void, ApiError>> {
  return call<void>("rename_folder", { id, name });
}

export function deleteFolder(id: string): Promise<Result<void, ApiError>> {
  return call<void>("delete_folder", { id });
}

export interface SaveRequestArgs {
  /** null saves a new request; an existing id overwrites it. */
  id: string | null;
  collectionId: string;
  folderId: string | null;
  name: string;
  request: SendRequestInput;
}

export function saveRequest(args: SaveRequestArgs): Promise<Result<SavedRequest, ApiError>> {
  return call<SavedRequest>("save_request", {
    id: args.id,
    collectionId: args.collectionId,
    folderId: args.folderId,
    name: args.name,
    request: args.request,
  });
}

export function loadRequest(id: string): Promise<Result<SavedRequest, ApiError>> {
  return call<SavedRequest>("load_request", { id });
}

export function renameRequest(id: string, name: string): Promise<Result<void, ApiError>> {
  return call<void>("rename_request", { id, name });
}

export function moveRequest(id: string, folderId: string | null): Promise<Result<void, ApiError>> {
  return call<void>("move_request", { id, folderId });
}

export function deleteRequest(id: string): Promise<Result<void, ApiError>> {
  return call<void>("delete_request", { id });
}

export interface SaveWebSocketArgs {
  /** null saves a new WebSocket request; an existing id overwrites it. */
  id: string | null;
  collectionId: string;
  folderId: string | null;
  name: string;
  request: WebSocketRequest;
  draft: WsDraft;
}

/** Rename, move, delete and docs for a WebSocket use the request functions
 * above: the commands act on a row by id, whatever its kind. */
export function saveWebSocket(args: SaveWebSocketArgs): Promise<Result<SavedWebSocket, ApiError>> {
  return call<SavedWebSocket>("save_web_socket", { input: args });
}

export function loadWebSocket(id: string): Promise<Result<SavedWebSocket, ApiError>> {
  return call<SavedWebSocket>("load_web_socket", { id });
}

export interface SaveExampleArgs {
  requestId: string;
  name: string;
  /** The request as sent, already resolved. */
  request: SendRequestInput;
  status: number;
  responseHeaders: KeyValue[];
  responseBody: string;
}

export function saveExample(args: SaveExampleArgs): Promise<Result<Example, ApiError>> {
  return call<Example>("save_example", { example: args });
}

export function loadExample(id: string): Promise<Result<Example, ApiError>> {
  return call<Example>("load_example", { id });
}

export function renameExample(id: string, name: string): Promise<Result<void, ApiError>> {
  return call<void>("rename_example", { id, name });
}

export function deleteExample(id: string): Promise<Result<void, ApiError>> {
  return call<void>("delete_example", { id });
}
