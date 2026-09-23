// http_client/src/features/websocket/WebSocketSettingsPanel.tsx
//
// Settings for a WebSocket request. Follows SettingsPanel's layout and
// conventions without forking it: once a connection is upgraded, redirects,
// the HTTP version and the body options mean nothing, so this is a
// different set of settings, not a subset of the same one.
import { ShieldAlert } from "lucide-react";

import type { WebSocketSettings } from "@/types/websocket";

const KIB = 1024;

interface WebSocketSettingsPanelProps {
  settings: WebSocketSettings;
  onChange: (patch: Partial<WebSocketSettings>) => void;
}

export function WebSocketSettingsPanel({ settings, onChange }: WebSocketSettingsPanelProps) {
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
          The handshake only. An open connection never times out.
        </span>
      </label>

      <label className="flex items-center gap-2">
        <span className="w-48 text-muted-foreground">Max message size (KB)</span>
        <input
          className="h-8 w-24 rounded-md border border-input bg-background px-2"
          min={1}
          onChange={(event) =>
            onChange({ maxMessageBytes: clampInt(event.target.value, 1, 128 * KIB) * KIB })
          }
          type="number"
          value={Math.round(settings.maxMessageBytes / KIB)}
        />
        <span className="text-xs text-muted-foreground">
          A bigger incoming message closes the connection with code 1009.
        </span>
      </label>

      <label className="flex items-center gap-2">
        <input
          checked={settings.autoReconnect}
          onChange={(event) => onChange({ autoReconnect: event.target.checked })}
          type="checkbox"
        />
        <span>
          Reconnect automatically
          <span className="ml-2 text-xs text-muted-foreground">
            After an unexpected drop only, never after Disconnect or a refused handshake. Waits 1 s,
            doubling to 30 s, up to 10 attempts.
          </span>
        </span>
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

      <label className="flex items-center gap-2">
        <input
          checked={settings.sendCookies}
          onChange={(event) => onChange({ sendCookies: event.target.checked })}
          type="checkbox"
        />
        Send stored cookies with the handshake
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
            <ShieldAlert className="h-4 w-4" aria-hidden />
            Messages on this connection are not protected against interception.
          </p>
        )}
      </div>
    </div>
  );
}

function clampInt(raw: string, min: number, max: number): number {
  const parsed = Number.parseInt(raw, 10);
  if (Number.isNaN(parsed)) {
    return min;
  }
  return Math.min(max, Math.max(min, parsed));
}
