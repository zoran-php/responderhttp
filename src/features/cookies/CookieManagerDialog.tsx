// http_client/src/features/cookies/CookieManagerDialog.tsx
//
// Read and delete only: cookies are written by the Rust jar as responses
// arrive, never edited here. Grouped by domain because that is how you think
// about them when something is misbehaving.
import { useEffect, useMemo } from "react";
import { Trash2 } from "lucide-react";

import { Modal } from "@/components/Modal";
import { useCookiesStore } from "@/store/cookies-store";
import type { Cookie } from "@/types/cookies";

/** One domain and every cookie stored under it. */
type DomainGroup = [string, Cookie[]];

interface CookieManagerDialogProps {
  onClose: () => void;
}

export function CookieManagerDialog({ onClose }: CookieManagerDialogProps) {
  const cookies = useCookiesStore((state) => state.cookies);
  const error = useCookiesStore((state) => state.error);
  const loadCookies = useCookiesStore((state) => state.loadCookies);
  const deleteCookie = useCookiesStore((state) => state.deleteCookie);
  const clearCookies = useCookiesStore((state) => state.clearCookies);

  useEffect(() => {
    void loadCookies();
  }, [loadCookies]);

  const byDomain: DomainGroup[] = useMemo(() => groupByDomain(cookies), [cookies]);

  return (
    <Modal onClose={onClose} title="Cookies">
      {error && <p className="mb-3 text-sm text-destructive">{error.message}</p>}

      {cookies.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No cookies yet. They are stored automatically as responses set them.
        </p>
      ) : (
        <div className="max-h-96 space-y-4 overflow-auto">
          {byDomain.map(([domain, entries]) => (
            <div key={domain}>
              <p className="mb-1 font-mono text-xs font-semibold text-primary">{domain}</p>
              <table className="w-full text-sm">
                <tbody>
                  {entries.map((cookie) => (
                    <tr
                      className="border-b border-border/50"
                      key={`${cookie.domain}|${cookie.path}|${cookie.name}`}
                    >
                      <td className="py-1 pr-2 font-mono text-xs">{cookie.name}</td>
                      <td className="max-w-48 truncate py-1 pr-2 font-mono text-xs text-muted-foreground">
                        {cookie.value}
                      </td>
                      <td className="whitespace-nowrap py-1 pr-2 text-xs text-muted-foreground">
                        {describe(cookie)}
                      </td>
                      <td className="w-8 py-1 text-right">
                        <button
                          aria-label={`Delete ${cookie.name} for ${cookie.domain}`}
                          className="rounded p-1 text-muted-foreground hover:bg-accent hover:text-destructive"
                          onClick={() =>
                            void deleteCookie(cookie.domain, cookie.path, cookie.name)
                          }
                          type="button"
                        >
                          <Trash2 aria-hidden className="h-3.5 w-3.5" />
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ))}
        </div>
      )}

      <div className="mt-4 flex justify-between gap-2">
        <button
          className="h-8 rounded-md border border-input px-3 text-sm text-destructive hover:bg-destructive/10 active:bg-destructive/20 disabled:opacity-60"
          disabled={cookies.length === 0}
          onClick={() => void clearCookies()}
          type="button"
        >
          Clear all
        </button>
        <button
          className="h-8 rounded-md border border-input px-3 text-sm hover:bg-accent active:bg-accent/70"
          onClick={onClose}
          type="button"
        >
          Close
        </button>
      </div>
    </Modal>
  );
}

function groupByDomain(cookies: Cookie[]): DomainGroup[] {
  const groups = new Map<string, Cookie[]>();
  for (const cookie of cookies) {
    const existing = groups.get(cookie.domain);
    if (existing) {
      existing.push(cookie);
    } else {
      groups.set(cookie.domain, [cookie]);
    }
  }
  return [...groups.entries()];
}

/** Path plus the flags that decide whether a cookie is sent at all. */
function describe(cookie: Cookie): string {
  const parts = [cookie.path];
  if (cookie.secure) {
    parts.push("Secure");
  }
  if (cookie.httpOnly) {
    parts.push("HttpOnly");
  }
  parts.push(cookie.expiresAt === null ? "session" : "expires");
  return parts.join(" · ");
}
