// http_client/src/services/app-lifecycle.ts
//
// Quitting from the window's close button (PLAN-LINUX.md 17b-2). On a Linux
// desktop with no tray, Rust does not close the window itself: it emits
// QUIT_REQUESTED_EVENT, the app decides whether anything would be lost
// (lib/quit-guard.ts) and asks if so, and then calls quit_app. Windows, and a
// Linux desktop with a tray, hide to the tray and never emit it.
//
// The one place the frontend listens for that event or calls that command,
// for the same reason invoke() lives only in services/ (CLAUDE.md §11 rule 3).
import { listen } from "@tauri-apps/api/event";

import { call } from "@/services/invoke";
import type { ApiError } from "@/types/http";
import type { Result } from "@/types/result";

/** Must match QUIT_REQUESTED_EVENT in src-tauri/src/desktop/window.rs. */
export const QUIT_REQUESTED_EVENT = "quit-requested";

/** Calls `handler` each time the close button asks to quit. Resolves to the
 * function that stops listening. */
export function onQuitRequested(handler: () => void): Promise<() => void> {
  return listen(QUIT_REQUESTED_EVENT, () => handler());
}

/** Ends the app. Resolves only if the command fails, since on success the
 * process is gone. */
export function quitApp(): Promise<Result<void, ApiError>> {
  return call<void>("quit_app");
}
