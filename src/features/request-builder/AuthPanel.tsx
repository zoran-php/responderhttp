// http_client/src/features/request-builder/AuthPanel.tsx
//
// Presentational: the chosen scheme and its values in, a replacement Auth
// out. Which headers a scheme turns into is decided once, in Rust
// (http/auth.rs) — this panel only collects what the user typed.
//
// Each scheme's secret field is masked and encrypted on disk (PLAN.md
// Phase 9). Usernames, key names and header names are not secrets.
import type { ReactNode } from "react";

import { SecretInput } from "@/components/SecretInput";
import type { ApiKeyLocation, Auth, SecretState } from "@/types/http";

const SCHEMES = [
  { kind: "none", label: "No Auth" },
  { kind: "basic", label: "Basic" },
  { kind: "bearer", label: "Bearer Token" },
  { kind: "apiKey", label: "API Key" },
  { kind: "custom", label: "Custom Header" },
] as const;

const INPUT_CLASS =
  "h-9 w-full rounded-md border border-input bg-background px-3 text-sm " +
  "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring";

const SECRET_NOTICES: Record<SecretState, string | null> = {
  ok: null,
  needsReentry:
    "The saved secret could not be decrypted on this computer, so the field is empty. " +
    "Enter it again and save.",
  unavailable:
    "The system credential store is unavailable, so the saved secret cannot be read " +
    "and a new one cannot be saved until it is.",
};

interface AuthPanelProps {
  auth: Auth;
  secretState: SecretState;
  onChange: (auth: Auth) => void;
}

export function AuthPanel({ auth, secretState, onChange }: AuthPanelProps) {
  const notice = auth.kind === "none" ? null : SECRET_NOTICES[secretState];
  return (
    <div className="flex flex-col gap-4 p-4">
      {notice !== null && (
        <p
          className="rounded-md border border-destructive/50 px-3 py-2 text-sm text-destructive"
          role="alert"
        >
          {notice}
        </p>
      )}

      <Field label="Type">
        <select
          className={INPUT_CLASS}
          onChange={(event) => onChange(blankAuth(event.target.value as Auth["kind"]))}
          value={auth.kind}
        >
          {SCHEMES.map((scheme) => (
            <option key={scheme.kind} value={scheme.kind}>
              {scheme.label}
            </option>
          ))}
        </select>
      </Field>

      {auth.kind === "none" && (
        <p className="text-sm text-muted-foreground">
          This request is sent without an authorization header.
        </p>
      )}

      {auth.kind === "basic" && (
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Username">
            <input
              className={INPUT_CLASS}
              onChange={(event) => onChange({ ...auth, username: event.target.value })}
              spellCheck={false}
              value={auth.username}
            />
          </Field>
          <Field label="Password">
            <SecretInput
              className={INPUT_CLASS}
              onChange={(password) => onChange({ ...auth, password })}
              value={auth.password}
            />
          </Field>
        </div>
      )}

      {auth.kind === "bearer" && (
        <Field label="Token">
          <SecretInput
            className={INPUT_CLASS}
            onChange={(token) => onChange({ ...auth, token })}
            placeholder="eyJhbGciOi... or {{token}}"
            value={auth.token}
          />
        </Field>
      )}

      {auth.kind === "apiKey" && (
        <div className="flex flex-col gap-4">
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Key">
              <input
                className={INPUT_CLASS}
                onChange={(event) => onChange({ ...auth, key: event.target.value })}
                placeholder="X-Api-Key"
                spellCheck={false}
                value={auth.key}
              />
            </Field>
            <Field label="Value">
              <SecretInput
                className={INPUT_CLASS}
                onChange={(value) => onChange({ ...auth, value })}
                value={auth.value}
              />
            </Field>
          </div>
          <Field
            hint="A key in the query string is recorded by servers and proxies; the header is not."
            label="Send in"
          >
            <select
              className={INPUT_CLASS}
              onChange={(event) =>
                onChange({ ...auth, location: event.target.value as ApiKeyLocation })
              }
              value={auth.location}
            >
              <option value="header">Header</option>
              <option value="query">Query parameter</option>
            </select>
          </Field>
        </div>
      )}

      {auth.kind === "custom" && (
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Header name">
            <input
              className={INPUT_CLASS}
              onChange={(event) => onChange({ ...auth, headerName: event.target.value })}
              placeholder="X-Signature"
              spellCheck={false}
              value={auth.headerName}
            />
          </Field>
          <Field label="Header value">
            <SecretInput
              className={INPUT_CLASS}
              onChange={(headerValue) => onChange({ ...auth, headerValue })}
              value={auth.headerValue}
            />
          </Field>
        </div>
      )}

      {auth.kind !== "none" && (
        <p className="text-sm text-muted-foreground">
          A header of the same name in the Headers tab overrides this. Put credentials here or in a
          secret variable rather than in Headers: only these are encrypted on disk and kept out of
          history.
        </p>
      )}
    </div>
  );
}

function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      {children}
      {hint !== undefined && <span className="text-xs text-muted-foreground">{hint}</span>}
    </label>
  );
}

/** Switching scheme starts it empty rather than carrying values across. */
function blankAuth(kind: Auth["kind"]): Auth {
  switch (kind) {
    case "none":
      return { kind: "none" };
    case "basic":
      return { kind: "basic", username: "", password: "" };
    case "bearer":
      return { kind: "bearer", token: "" };
    case "apiKey":
      return { kind: "apiKey", key: "", value: "", location: "header" };
    case "custom":
      return { kind: "custom", headerName: "", headerValue: "" };
  }
}
