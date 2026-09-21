// http_client/src/types/result.ts
//
// Errors cross the Tauri boundary as values, not exceptions: a rejected
// invoke() carries a serialised ApiError, and callers should have to handle
// it rather than remember to try/catch.
export type Result<T, E> = { ok: true; value: T } | { ok: false; error: E };
