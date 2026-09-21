// http_client/src/lib/http-method-colors.ts
//
// One place mapping an HttpMethod to its Tailwind color class, so the
// collections tree and the method select stay in sync without either one
// hardcoding the method → color rules. The colors themselves are theme
// tokens in src/index.css (--method-get etc.), not hex here — see that
// file's comment for where get/post/put/delete came from.
import type { HttpMethod } from "@/types/http";

const METHOD_TEXT_COLOR: Record<HttpMethod, string> = {
  GET: "text-method-get",
  POST: "text-method-post",
  PUT: "text-method-put",
  PATCH: "text-method-patch",
  DELETE: "text-method-delete",
  HEAD: "text-method-head",
  OPTIONS: "text-method-options",
};

export function methodTextColor(method: HttpMethod): string {
  return METHOD_TEXT_COLOR[method];
}
