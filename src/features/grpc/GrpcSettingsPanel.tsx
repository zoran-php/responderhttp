// http_client/src/features/grpc/GrpcSettingsPanel.tsx
//
// Settings for a gRPC request (PLAN-GRPC.md section 2): certificate
// verification, default values in the response, the receive limit, the
// connect timeout, a deadline (D5) and a proxy. Follows the WebSocket
// settings panel's layout.
import { ShieldAlert } from "lucide-react";

import { clampInt } from "@/lib/number-input";
import type { GrpcSettings } from "@/types/grpc";

const MIB = 1024 * 1024;
/** Rust caps the limit here (domain/grpc_wire.rs, MAX_RECEIVE_BYTES_CAP). */
const MAX_RECEIVE_MIB = 256;

interface GrpcSettingsPanelProps {
  settings: GrpcSettings;
  onChange: (patch: Partial<GrpcSettings>) => void;
}

export function GrpcSettingsPanel({ settings, onChange }: GrpcSettingsPanelProps) {
  const deadlineSeconds = settings.deadlineMs === null ? null : settings.deadlineMs / 1000;

  return (
    <div className="space-y-4 p-4 text-sm">
      <label className="flex items-center gap-2">
        <span className="w-48 text-muted-foreground">Connect timeout (seconds)</span>
        <input
          className="h-8 w-24 rounded-md border border-input bg-background px-2"
          min={1}
          onChange={(event) =>
            onChange({ connectTimeoutMs: clampInt(event.target.value, 1, 300) * 1000 })
          }
          type="number"
          value={Math.round(settings.connectTimeoutMs / 1000)}
        />
        <span className="text-xs text-muted-foreground">
          Connecting only. A long stream is bounded by the deadline.
        </span>
      </label>

      <div className="flex items-center gap-2">
        <label className="flex w-48 items-center gap-2 text-muted-foreground">
          <input
            checked={settings.deadlineMs !== null}
            onChange={(event) => onChange({ deadlineMs: event.target.checked ? 30_000 : null })}
            type="checkbox"
          />
          Deadline (seconds)
        </label>
        <input
          aria-label="Deadline in seconds"
          className="h-8 w-24 rounded-md border border-input bg-background px-2 disabled:opacity-50"
          disabled={deadlineSeconds === null}
          min={1}
          onChange={(event) =>
            onChange({ deadlineMs: clampInt(event.target.value, 1, 86_400) * 1000 })
          }
          type="number"
          value={deadlineSeconds === null ? "" : Math.round(deadlineSeconds)}
        />
        <span className="text-xs text-muted-foreground">
          Sent as grpc-timeout. The call ends with DEADLINE_EXCEEDED when it passes.
        </span>
      </div>

      <label className="flex items-center gap-2">
        <span className="w-48 text-muted-foreground">Max response message (MB)</span>
        <input
          className="h-8 w-24 rounded-md border border-input bg-background px-2"
          min={1}
          onChange={(event) =>
            onChange({ maxReceiveBytes: clampInt(event.target.value, 1, MAX_RECEIVE_MIB) * MIB })
          }
          type="number"
          value={Math.max(1, Math.round(settings.maxReceiveBytes / MIB))}
        />
        <span className="text-xs text-muted-foreground">
          A bigger message ends the call with RESOURCE_EXHAUSTED. At most {MAX_RECEIVE_MIB} MB.
        </span>
      </label>

      <label className="flex items-center gap-2">
        <input
          checked={settings.includeDefaults}
          onChange={(event) => onChange({ includeDefaults: event.target.checked })}
          type="checkbox"
        />
        Show fields with default values in received messages
      </label>

      <label className="flex items-center gap-2">
        <span className="w-48 text-muted-foreground">Proxy</span>
        <input
          className="h-8 flex-1 rounded-md border border-input bg-background px-2 font-mono text-xs"
          onChange={(event) => onChange({ proxy: event.target.value || null })}
          placeholder="http://127.0.0.1:8080"
          spellCheck={false}
          value={settings.proxy ?? ""}
        />
      </label>

      <div className="space-y-1 border-t border-border pt-4">
        <label className="flex items-center gap-2">
          <input
            checked={!settings.verifyTls}
            onChange={(event) => onChange({ verifyTls: !event.target.checked })}
            type="checkbox"
          />
          Disable TLS certificate verification
        </label>
        {!settings.verifyTls && (
          <p className="flex items-center gap-2 text-xs text-destructive">
            <ShieldAlert aria-hidden className="h-4 w-4" />
            Calls with TLS on are not protected against interception.
          </p>
        )}
      </div>
    </div>
  );
}
