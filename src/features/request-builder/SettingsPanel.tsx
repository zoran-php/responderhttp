// http_client/src/features/request-builder/SettingsPanel.tsx
import { ShieldAlert } from "lucide-react";

import type { HttpVersionPreference, RequestSettings, TlsMinimum } from "@/types/http";

/** Labelled for what this app actually does:
 * libcurl exposes no general "strict parser", and rustls has no TLS below
 * 1.2 to disable. Copying the other product's wording would promise more
 * than the transport can deliver. */
const HTTP_VERSIONS: { value: HttpVersionPreference; label: string }[] = [
  { value: "auto", label: "Auto (HTTP/2, falling back to 1.1)" },
  { value: "http11", label: "HTTP/1.1" },
  { value: "http2", label: "HTTP/2" },
];

const TLS_MINIMUMS: { value: TlsMinimum; label: string }[] = [
  { value: "auto", label: "Library default" },
  { value: "tls12", label: "TLS 1.2 or higher" },
  { value: "tls13", label: "TLS 1.3 only" },
];

interface SettingsPanelProps {
  settings: RequestSettings;
  onChange: (patch: Partial<RequestSettings>) => void;
}

export function SettingsPanel({ settings, onChange }: SettingsPanelProps) {
  return (
    <div className="space-y-4 p-4 text-sm">
      <label className="flex items-center gap-2">
        <input
          checked={settings.followRedirects}
          onChange={(event) => onChange({ followRedirects: event.target.checked })}
          type="checkbox"
        />
        Follow redirects
      </label>

      <label className="flex items-center gap-2">
        <span className="w-40 text-muted-foreground">Max redirects</span>
        <input
          className="h-8 w-24 rounded-md border border-input bg-background px-2"
          disabled={!settings.followRedirects}
          min={0}
          onChange={(event) => onChange({ maxRedirects: clampInt(event.target.value, 0, 100) })}
          type="number"
          value={settings.maxRedirects}
        />
      </label>

      <label className="flex items-center gap-2">
        <span className="w-40 text-muted-foreground">Timeout (seconds)</span>
        <input
          className="h-8 w-24 rounded-md border border-input bg-background px-2"
          min={1}
          onChange={(event) =>
            onChange({ timeoutMs: clampInt(event.target.value, 1, 3600) * 1000 })
          }
          type="number"
          value={Math.round(settings.timeoutMs / 1000)}
        />
      </label>

      <label className="flex items-center gap-2">
        <span className="w-40 text-muted-foreground">Proxy</span>
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
        Send stored cookies
      </label>

      <label className="flex items-center gap-2">
        <span className="w-40 text-muted-foreground">HTTP version</span>
        <select
          className="h-8 flex-1 rounded-md border border-input bg-background px-2"
          onChange={(event) =>
            onChange({ httpVersion: event.target.value as HttpVersionPreference })
          }
          value={settings.httpVersion}
        >
          {HTTP_VERSIONS.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </label>

      <label className="flex items-center gap-2">
        <input
          checked={settings.encodeUrl}
          onChange={(event) => onChange({ encodeUrl: event.target.checked })}
          type="checkbox"
        />
        <span>
          Encode URL automatically
          <span className="ml-2 text-xs text-muted-foreground">
            Characters a URL cannot contain, such as spaces, are percent-encoded when sent. Existing
            %-escapes are left as typed.
          </span>
        </span>
      </label>

      <div className="space-y-4 border-t border-border pt-4">
        <label className="flex items-center gap-2">
          <input
            checked={settings.keepMethodOnRedirect}
            disabled={!settings.followRedirects}
            onChange={(event) => onChange({ keepMethodOnRedirect: event.target.checked })}
            type="checkbox"
          />
          <span>
            Keep the method on redirect
            <span className="ml-2 text-xs text-muted-foreground">
              Otherwise POST becomes GET on 301, 302 and 303.
            </span>
          </span>
        </label>

        <div className="space-y-1">
          <label className="flex items-center gap-2">
            <input
              checked={settings.keepAuthOnRedirect}
              disabled={!settings.followRedirects}
              onChange={(event) => onChange({ keepAuthOnRedirect: event.target.checked })}
              type="checkbox"
            />
            Keep the Authorization header across hosts
          </label>
          {settings.keepAuthOnRedirect && (
            <p className="flex items-center gap-2 text-xs text-warning">
              <ShieldAlert className="h-4 w-4 shrink-0" aria-hidden />
              A redirect can now hand your credentials to a different server.
            </p>
          )}
        </div>
      </div>

      <label className="flex items-center gap-2">
        <span className="w-40 text-muted-foreground">Minimum TLS version</span>
        <select
          className="h-8 flex-1 rounded-md border border-input bg-background px-2"
          onChange={(event) => onChange({ tlsMinimum: event.target.value as TlsMinimum })}
          value={settings.tlsMinimum}
        >
          {TLS_MINIMUMS.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </label>

      <label className="flex items-center gap-2">
        <input
          checked={settings.allowHttp09}
          onChange={(event) => onChange({ allowHttp09: event.target.checked })}
          type="checkbox"
        />
        <span>
          Accept HTTP/0.9 responses
          <span className="ml-2 text-xs text-muted-foreground">
            A bodies-only reply with no status line or headers.
          </span>
        </span>
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
            Responses on this request are not protected against interception.
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
