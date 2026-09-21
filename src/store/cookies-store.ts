// http_client/src/store/cookies-store.ts
//
// The jar as the manager sees it. Cookies are written by Rust as responses
// arrive, so this store only reads and removes — there is no "save".
import { create } from "zustand";

import * as cookiesService from "@/services/cookies";
import type { Cookie } from "@/types/cookies";
import type { ApiError } from "@/types/http";

interface CookiesState {
  cookies: Cookie[];
  error: ApiError | null;

  loadCookies: () => Promise<void>;
  deleteCookie: (domain: string, path: string, name: string) => Promise<boolean>;
  clearCookies: () => Promise<boolean>;
  clearError: () => void;
}

export const useCookiesStore = create<CookiesState>((set, get) => ({
  cookies: [],
  error: null,

  loadCookies: async () => {
    const result = await cookiesService.listCookies();
    if (!result.ok) {
      set({ error: result.error });
      return;
    }
    set({ cookies: result.value, error: null });
  },

  deleteCookie: async (domain, path, name) => {
    const result = await cookiesService.deleteCookie(domain, path, name);
    if (!result.ok) {
      set({ error: result.error });
      return false;
    }
    await get().loadCookies();
    return true;
  },

  clearCookies: async () => {
    const result = await cookiesService.clearCookies();
    if (!result.ok) {
      set({ error: result.error });
      return false;
    }
    await get().loadCookies();
    return true;
  },

  clearError: () => set({ error: null }),
}));
