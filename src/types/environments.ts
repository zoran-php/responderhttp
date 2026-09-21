// http_client/src/types/environments.ts
//
// Mirrors EnvironmentDto, EnvironmentVariableDto and EnvironmentVariableInput
// in src-tauri/src/commands/dto.rs.
import type { SecretState } from "@/types/http";

export interface Environment {
  id: string;
  name: string;
}

/**
 * One {{variable}}. A secret one is encrypted at rest and masked in the
 * editor, and resolves exactly like any other. Its value still arrives here
 * in plain text — the editor shows it and substitution happens in the
 * frontend — so the protection is for the database file (PLAN.md Phase 9).
 */
export interface EnvironmentVariable {
  name: string;
  value: string;
  secret: boolean;
  /** Anything but "ok" means `value` is empty because it could not be read. */
  secretState: SecretState;
}

/** What the editor sends. Readability is decided by storage, not claimed. */
export interface EnvironmentVariableInput {
  name: string;
  value: string;
  secret: boolean;
}
