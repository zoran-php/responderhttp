// http_client/src/lib/status-colors.ts
//
// One place mapping an HTTP status to its colour classes, for the same
// reason http-method-colors.ts exists: the response viewer, the history
// panel and the example viewer all show a status, and three copies of the
// same mapping is three chances for them to drift (CLAUDE.md section 7, DRY).
import { statusClass, type StatusClass } from "@/lib/format";

/** Filled pill, for the one status a view is primarily about. */
const PILL: Record<StatusClass, string> = {
  success: "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400",
  redirect: "bg-amber-500/15 text-amber-600 dark:text-amber-400",
  clientError: "bg-orange-500/15 text-orange-600 dark:text-orange-400",
  serverError: "bg-destructive/15 text-destructive",
  unknown: "bg-muted text-muted-foreground",
};

/** Text only, for a status shown inline in a dense list. */
const TEXT: Record<StatusClass, string> = {
  success: "text-emerald-600 dark:text-emerald-400",
  redirect: "text-amber-600 dark:text-amber-400",
  clientError: "text-orange-600 dark:text-orange-400",
  serverError: "text-destructive",
  unknown: "text-muted-foreground",
};

export function statusPillClass(status: number): string {
  return PILL[statusClass(status)];
}

export function statusTextColor(status: number): string {
  return TEXT[statusClass(status)];
}
