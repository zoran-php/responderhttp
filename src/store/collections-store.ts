// http_client/src/store/collections-store.ts
//
// Data and persistence actions for the collections sidebar. Ephemeral UI
// state (which context menu, dialog, or inline editor is open) stays as
// local component state in features/collections/ — this store only holds
// what more than one component needs to agree on (CLAUDE.md section 6).
import { create } from "zustand";

import { emptyRequestInput } from "@/lib/request-defaults";
import * as collectionsService from "@/services/collections";
import type { SaveExampleArgs, SaveRequestArgs } from "@/services/collections";
import type {
  Collection,
  CollectionContents,
  Example,
  Folder,
  SavedRequest,
} from "@/types/collections";
import type { ApiError } from "@/types/http";

interface CollectionsState {
  collections: Collection[];
  contentsById: Record<string, CollectionContents>;
  expandedCollectionIds: Set<string>;
  expandedFolderIds: Set<string>;
  /** A request node only expands when it has examples under it. */
  expandedRequestIds: Set<string>;
  /**
   * Full examples, fetched one at a time when opened. Cached here rather than
   * held on the tab so a rename or a re-open does not refetch, and so the tab
   * strip can read an open example's name the way it reads an environment's.
   */
  examplesById: Record<string, Example>;
  error: ApiError | null;

  loadCollections: () => Promise<void>;
  toggleCollection: (id: string) => void;
  toggleFolder: (id: string) => void;
  toggleRequest: (id: string) => void;
  /** Idempotent, unlike toggle: creating inside a node has to open it,
   * never close it. */
  expandCollection: (id: string) => void;
  expandFolder: (id: string) => void;
  expandRequest: (id: string) => void;
  refreshContents: (collectionId: string) => Promise<void>;

  createCollection: (name: string) => Promise<Collection | null>;
  renameCollection: (id: string, name: string) => Promise<boolean>;
  deleteCollection: (id: string) => Promise<boolean>;

  createFolder: (
    collectionId: string,
    parentFolderId: string | null,
    name: string,
  ) => Promise<Folder | null>;
  renameFolder: (collectionId: string, id: string, name: string) => Promise<boolean>;
  deleteFolder: (collectionId: string, id: string) => Promise<boolean>;

  saveRequest: (args: SaveRequestArgs) => Promise<SavedRequest | null>;
  /** Right-click "New request" on a collection or folder: the same
   * save_request command as any new save, starting from a blank body. */
  createRequest: (
    collectionId: string,
    folderId: string | null,
    name: string,
  ) => Promise<SavedRequest | null>;
  /** Right-click "Duplicate" on a request: reload it (rather than trust a
   * possibly-stale tree snapshot) and re-save its body under a new id and
   * name, alongside the original. */
  duplicateRequest: (
    collectionId: string,
    id: string,
    name: string,
  ) => Promise<SavedRequest | null>;
  loadRequest: (id: string) => Promise<SavedRequest | null>;
  renameRequest: (collectionId: string, id: string, name: string) => Promise<boolean>;
  moveRequest: (collectionId: string, id: string, folderId: string | null) => Promise<boolean>;
  deleteRequest: (collectionId: string, id: string) => Promise<boolean>;

  saveExample: (collectionId: string, args: SaveExampleArgs) => Promise<Example | null>;
  loadExample: (id: string) => Promise<Example | null>;
  renameExample: (collectionId: string, id: string, name: string) => Promise<boolean>;
  deleteExample: (collectionId: string, id: string) => Promise<boolean>;

  clearError: () => void;
}

export const useCollectionsStore = create<CollectionsState>((set, get) => ({
  collections: [],
  contentsById: {},
  expandedCollectionIds: new Set(),
  expandedFolderIds: new Set(),
  expandedRequestIds: new Set(),
  examplesById: {},
  error: null,

  loadCollections: async () => {
    const result = await collectionsService.listCollections();
    if (result.ok) {
      set({ collections: [...result.value].sort(byName) });
    } else {
      set({ error: result.error });
    }
  },

  toggleCollection: (id) => {
    if (!get().expandedCollectionIds.has(id)) {
      get().expandCollection(id);
      return;
    }
    const expanded = new Set(get().expandedCollectionIds);
    expanded.delete(id);
    set({ expandedCollectionIds: expanded });
  },

  toggleFolder: (id) => {
    if (!get().expandedFolderIds.has(id)) {
      get().expandFolder(id);
      return;
    }
    const expanded = new Set(get().expandedFolderIds);
    expanded.delete(id);
    set({ expandedFolderIds: expanded });
  },

  toggleRequest: (id) => {
    if (!get().expandedRequestIds.has(id)) {
      get().expandRequest(id);
      return;
    }
    const expanded = new Set(get().expandedRequestIds);
    expanded.delete(id);
    set({ expandedRequestIds: expanded });
  },

  expandCollection: (id) => {
    if (get().expandedCollectionIds.has(id)) {
      return;
    }
    const expanded = new Set(get().expandedCollectionIds);
    expanded.add(id);
    set({ expandedCollectionIds: expanded });
    if (!get().contentsById[id]) {
      void get().refreshContents(id);
    }
  },

  expandFolder: (id) => {
    if (get().expandedFolderIds.has(id)) {
      return;
    }
    const expanded = new Set(get().expandedFolderIds);
    expanded.add(id);
    set({ expandedFolderIds: expanded });
  },

  expandRequest: (id) => {
    if (get().expandedRequestIds.has(id)) {
      return;
    }
    const expanded = new Set(get().expandedRequestIds);
    expanded.add(id);
    set({ expandedRequestIds: expanded });
  },

  refreshContents: async (collectionId) => {
    const result = await collectionsService.collectionContents(collectionId);
    if (result.ok) {
      set({ contentsById: { ...get().contentsById, [collectionId]: result.value } });
    } else {
      set({ error: result.error });
    }
  },

  createCollection: async (name) => {
    const result = await collectionsService.createCollection(name);
    if (!result.ok) {
      set({ error: result.error });
      return null;
    }
    set({ collections: [...get().collections, result.value].sort(byName) });
    return result.value;
  },

  renameCollection: async (id, name) => {
    const result = await collectionsService.renameCollection(id, name);
    if (!result.ok) {
      set({ error: result.error });
      return false;
    }
    set({
      collections: get()
        .collections.map((collection) => (collection.id === id ? { ...collection, name } : collection))
        .sort(byName),
    });
    return true;
  },

  deleteCollection: async (id) => {
    const result = await collectionsService.deleteCollection(id);
    if (!result.ok) {
      set({ error: result.error });
      return false;
    }
    const expandedCollectionIds = new Set(get().expandedCollectionIds);
    expandedCollectionIds.delete(id);
    set({
      collections: get().collections.filter((collection) => collection.id !== id),
      contentsById: withoutKey(get().contentsById, id),
      expandedCollectionIds,
    });
    return true;
  },

  createFolder: async (collectionId, parentFolderId, name) => {
    const result = await collectionsService.createFolder(collectionId, parentFolderId, name);
    if (!result.ok) {
      set({ error: result.error });
      return null;
    }
    await get().refreshContents(collectionId);
    return result.value;
  },

  renameFolder: async (collectionId, id, name) => {
    const result = await collectionsService.renameFolder(id, name);
    if (!result.ok) {
      set({ error: result.error });
      return false;
    }
    await get().refreshContents(collectionId);
    return true;
  },

  deleteFolder: async (collectionId, id) => {
    const result = await collectionsService.deleteFolder(id);
    if (!result.ok) {
      set({ error: result.error });
      return false;
    }
    await get().refreshContents(collectionId);
    return true;
  },

  saveRequest: async (args) => {
    const result = await collectionsService.saveRequest(args);
    if (!result.ok) {
      set({ error: result.error });
      return null;
    }
    await get().refreshContents(args.collectionId);
    return result.value;
  },

  createRequest: async (collectionId, folderId, name) => {
    return get().saveRequest({
      id: null,
      collectionId,
      folderId,
      name,
      request: emptyRequestInput(),
    });
  },

  duplicateRequest: async (collectionId, id, name) => {
    const original = await collectionsService.loadRequest(id);
    if (!original.ok) {
      set({ error: original.error });
      return null;
    }
    return get().saveRequest({
      id: null,
      collectionId,
      folderId: original.value.folderId,
      name,
      request: original.value.request,
    });
  },

  loadRequest: async (id) => {
    const result = await collectionsService.loadRequest(id);
    if (!result.ok) {
      set({ error: result.error });
      return null;
    }
    return result.value;
  },

  renameRequest: async (collectionId, id, name) => {
    const result = await collectionsService.renameRequest(id, name);
    if (!result.ok) {
      set({ error: result.error });
      return false;
    }
    await get().refreshContents(collectionId);
    return true;
  },

  moveRequest: async (collectionId, id, folderId) => {
    const result = await collectionsService.moveRequest(id, folderId);
    if (!result.ok) {
      set({ error: result.error });
      return false;
    }
    await get().refreshContents(collectionId);
    return true;
  },

  deleteRequest: async (collectionId, id) => {
    const result = await collectionsService.deleteRequest(id);
    if (!result.ok) {
      set({ error: result.error });
      return false;
    }
    await get().refreshContents(collectionId);
    return true;
  },

  saveExample: async (collectionId, args) => {
    const result = await collectionsService.saveExample(args);
    if (!result.ok) {
      set({ error: result.error });
      return null;
    }
    // The request node has to open, or the new example lands out of sight —
    // the same reason "New request" expands its collection first.
    get().expandRequest(args.requestId);
    set((state) => ({ examplesById: { ...state.examplesById, [result.value.id]: result.value } }));
    await get().refreshContents(collectionId);
    return result.value;
  },

  loadExample: async (id) => {
    const result = await collectionsService.loadExample(id);
    if (!result.ok) {
      set({ error: result.error });
      return null;
    }
    set((state) => ({ examplesById: { ...state.examplesById, [id]: result.value } }));
    return result.value;
  },

  renameExample: async (collectionId, id, name) => {
    const result = await collectionsService.renameExample(id, name);
    if (!result.ok) {
      set({ error: result.error });
      return false;
    }
    set((state) => {
      const cached = state.examplesById[id];
      return cached
        ? { examplesById: { ...state.examplesById, [id]: { ...cached, name } } }
        : {};
    });
    await get().refreshContents(collectionId);
    return true;
  },

  deleteExample: async (collectionId, id) => {
    const result = await collectionsService.deleteExample(id);
    if (!result.ok) {
      set({ error: result.error });
      return false;
    }
    set((state) => ({ examplesById: withoutKey(state.examplesById, id) }));
    await get().refreshContents(collectionId);
    return true;
  },

  clearError: () => set({ error: null }),
}));

function byName(a: Collection, b: Collection): number {
  return a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
}

function withoutKey<T>(record: Record<string, T>, key: string): Record<string, T> {
  const next = { ...record };
  delete next[key];
  return next;
}
