// http_client/src/services/cookies.ts
//
// Cookie commands. The boundary crossing lives in services/invoke.ts
// (CLAUDE.md section 11, rule 3). The jar itself is applied in Rust, on the
// way out of the app — nothing here sends cookies, it only inspects and
// removes them.
import { call } from "@/services/invoke";
import type { Cookie } from "@/types/cookies";
import type { ApiError } from "@/types/http";
import type { Result } from "@/types/result";

export function listCookies(): Promise<Result<Cookie[], ApiError>> {
  return call<Cookie[]>("list_cookies");
}

export function deleteCookie(
  domain: string,
  path: string,
  name: string,
): Promise<Result<void, ApiError>> {
  return call<void>("delete_cookie", { domain, path, name });
}

export function clearCookies(): Promise<Result<void, ApiError>> {
  return call<void>("clear_cookies");
}
