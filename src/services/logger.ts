// http_client/src/services/logger.ts
//
// Frontend half of the logger. Lives in services/ because @tauri-apps/plugin-log
// reaches across the Tauri boundary exactly like invoke() does, and rule 3 puts
// every such call here rather than in a component.
//
// Ported from the Lockoncam desktop app's LogService (Angular) — same five
// levels, same "console first, then forward to the file" shape — minus the DI,
// since this app has no injector.
//
// Every message ends up in src-tauri/src/logging.rs's format closure, which is
// where redaction happens. There is deliberately no second redactor here: two
// implementations of that rule would drift, and the Rust one cannot be routed
// around.
import { isTauri } from "@tauri-apps/api/core";
import { debug, error, info, trace, warn } from "@tauri-apps/plugin-log";

type LogLevel = "trace" | "debug" | "info" | "warn" | "error";

const SINKS: Record<LogLevel, (message: string) => Promise<void>> = {
  trace,
  debug,
  info,
  warn,
  error,
};

/**
 * The console keeps working in `vite dev` and in tests, where there is no
 * Tauri host to forward to. `isTauri()` is what stops the plugin call from
 * rejecting in those environments.
 *
 * Never throws and never returns a promise the caller has to handle: a failed
 * log must not be able to fail the thing it was reporting on.
 */
function write(level: LogLevel, message: string): void {
  // The one place in the app a console call belongs. `no-console` is not
  // enabled in .eslintrc.cjs, so there is no directive to suppress here —
  // turning that rule on would make this the enforced exception rather than
  // the conventional one.
  console[level](message);
  if (!isTauri()) {
    return;
  }
  void SINKS[level](message).catch(() => {
    // Swallowed on purpose. The console line above already happened, and a
    // logger that can throw turns a diagnostic into an outage.
  });
}

export function logTrace(message: string): void {
  write("trace", message);
}

export function logDebug(message: string): void {
  write("debug", message);
}

export function logInfo(message: string): void {
  write("info", message);
}

export function logWarn(message: string): void {
  write("warn", message);
}

export function logError(message: string): void {
  write("error", message);
}
