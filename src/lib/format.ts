// http_client/src/lib/format.ts
//
// Pure display helpers. Components render what these return; they never do
// their own formatting (CLAUDE.md section 6).

/** Sub-second timings read better in ms; anything longer in seconds. */
export function formatDuration(milliseconds: number): string {
  if (!Number.isFinite(milliseconds) || milliseconds < 0) {
    return "—";
  }
  if (milliseconds < 1000) {
    return `${Math.round(milliseconds)} ms`;
  }
  return `${(milliseconds / 1000).toFixed(2)} s`;
}

const BYTE_UNITS = ["B", "KB", "MB", "GB"] as const;

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) {
    return "—";
  }
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const rounded = unit === 0 ? value : Number(value.toFixed(1));
  return `${rounded} ${BYTE_UNITS[unit]}`;
}

/**
 * Status class drives the colour of the status pill. Kept here so the
 * mapping exists once rather than in every component that shows a status.
 */
export type StatusClass = "success" | "redirect" | "clientError" | "serverError" | "unknown";

export function statusClass(status: number): StatusClass {
  if (status >= 200 && status < 300) return "success";
  if (status >= 300 && status < 400) return "redirect";
  if (status >= 400 && status < 500) return "clientError";
  if (status >= 500 && status < 600) return "serverError";
  return "unknown";
}

function pad(value: number, width: number): string {
  return String(value).padStart(width, "0");
}

/**
 * `HH:mm:ss.SSS` in local time, as the WebSocket log shows it. Milliseconds
 * matter there: an echo usually lands a few ms after its message.
 */
export function formatClockTime(epochMs: number): string {
  if (!Number.isFinite(epochMs)) {
    return "—";
  }
  const date = new Date(epochMs);
  return (
    `${pad(date.getHours(), 2)}:${pad(date.getMinutes(), 2)}:` +
    `${pad(date.getSeconds(), 2)}.${pad(date.getMilliseconds(), 3)}`
  );
}
