// http_client/src/types/cookies.ts
//
// Mirrors CookieDto in src-tauri/src/commands/dto.rs. expiresAt is unix
// seconds, or null for a session cookie — one that is dropped on launch.
export interface Cookie {
  name: string;
  value: string;
  domain: string;
  path: string;
  expiresAt: number | null;
  secure: boolean;
  httpOnly: boolean;
  /** True when the cookie belongs to exactly this host, not its subdomains. */
  hostOnly: boolean;
}
