// http_client/src/lib/utils.ts
//
// The one piece of infrastructure shadcn-generated components in
// components/ui/ require. Everything else in lib/ is added as Phase 1+
// features need pure helpers (validators, formatters, parsers, the
// curl-string builder) — see CLAUDE.md §6.
import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
