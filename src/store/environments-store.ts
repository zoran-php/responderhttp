// http_client/src/store/environments-store.ts
//
// Data and persistence actions for environments. Ephemeral UI state (which
// row is being renamed, which dialog is open) stays local to the components
// in features/environments/, as it does for collections.
import { create } from "zustand";

import * as environmentsService from "@/services/environments";
import type {
  Environment,
  EnvironmentVariable,
  EnvironmentVariableInput,
} from "@/types/environments";
import type { ApiError } from "@/types/http";

interface EnvironmentsState {
  environments: Environment[];
  variablesById: Record<string, EnvironmentVariable[]>;
  /**
   * Which environment requests resolve against. Deliberately separate from
   * whatever is open for editing, so you can edit Staging while sending
   * against Production. In memory only, like the open tabs — it resets on
   * launch rather than silently pointing at production after a restart.
   */
  activeEnvironmentId: string | null;
  error: ApiError | null;

  loadEnvironments: () => Promise<void>;
  createEnvironment: (name: string) => Promise<Environment | null>;
  renameEnvironment: (id: string, name: string) => Promise<boolean>;
  deleteEnvironment: (id: string) => Promise<boolean>;

  loadVariables: (environmentId: string) => Promise<void>;
  saveVariables: (environmentId: string, variables: EnvironmentVariableInput[]) => Promise<boolean>;

  setActiveEnvironment: (id: string | null) => void;
  /** The active environment's variables, or none when nothing is active. */
  activeVariables: () => EnvironmentVariable[];
  clearError: () => void;
}

export const useEnvironmentsStore = create<EnvironmentsState>((set, get) => ({
  environments: [],
  variablesById: {},
  activeEnvironmentId: null,
  error: null,

  loadEnvironments: async () => {
    const result = await environmentsService.listEnvironments();
    if (!result.ok) {
      set({ error: result.error });
      return;
    }
    set({ environments: result.value, error: null });
  },

  createEnvironment: async (name) => {
    const result = await environmentsService.createEnvironment(name);
    if (!result.ok) {
      set({ error: result.error });
      return null;
    }
    await get().loadEnvironments();
    return result.value;
  },

  renameEnvironment: async (id, name) => {
    const result = await environmentsService.renameEnvironment(id, name);
    if (!result.ok) {
      set({ error: result.error });
      return false;
    }
    await get().loadEnvironments();
    return true;
  },

  deleteEnvironment: async (id) => {
    const result = await environmentsService.deleteEnvironment(id);
    if (!result.ok) {
      set({ error: result.error });
      return false;
    }
    set((state) => {
      const variablesById = { ...state.variablesById };
      delete variablesById[id];
      return {
        variablesById,
        // Deleting the active environment leaves nothing active rather than
        // silently promoting another one.
        activeEnvironmentId: state.activeEnvironmentId === id ? null : state.activeEnvironmentId,
      };
    });
    await get().loadEnvironments();
    return true;
  },

  loadVariables: async (environmentId) => {
    const result = await environmentsService.environmentVariables(environmentId);
    if (!result.ok) {
      set({ error: result.error });
      return;
    }
    set((state) => ({
      variablesById: { ...state.variablesById, [environmentId]: result.value },
      error: null,
    }));
  },

  saveVariables: async (environmentId, variables) => {
    const result = await environmentsService.setEnvironmentVariables(environmentId, variables);
    if (!result.ok) {
      set({ error: result.error });
      return false;
    }
    // Re-read rather than trust the sent list: the backend drops blank names,
    // so what was sent and what is stored are not always the same.
    await get().loadVariables(environmentId);
    return true;
  },

  setActiveEnvironment: (id) => {
    set({ activeEnvironmentId: id });
    // Variables are otherwise only fetched when the editor is opened, so
    // activating an environment nobody has opened would substitute nothing.
    if (id !== null) {
      void get().loadVariables(id);
    }
  },

  activeVariables: () => {
    const { activeEnvironmentId, variablesById } = get();
    if (activeEnvironmentId === null) {
      return [];
    }
    return variablesById[activeEnvironmentId] ?? [];
  },

  clearError: () => set({ error: null }),
}));
