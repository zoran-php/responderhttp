// http_client/src/features/request-builder/RequestToolbar.tsx
//
// The strip above the URL row: where the request lives on the left, the
// actions that are *about* the request — rather than about sending it — on the
// right. Send and Cancel stay down with the URL, because those are the ones
// you reach for constantly.
//
// Deliberately quieter than the row below it: smaller text, smaller icons,
// muted until hovered. It is reference and occasional use, not the main event.
import { Cookie, Copy, Save } from "lucide-react";

interface RequestToolbarProps {
  /** Outside in, ending with the request name. See lib/request-path.ts. */
  pathSegments: string[];
  onCopyAsCurl: () => void;
  onManageCookies: () => void;
  onSave: () => void;
}

const ACTION =
  "inline-flex h-7 items-center gap-1.5 rounded border border-input px-2 text-xs text-muted-foreground hover:bg-accent hover:text-foreground active:bg-accent/70";

export function RequestToolbar({
  pathSegments,
  onCopyAsCurl,
  onManageCookies,
  onSave,
}: RequestToolbarProps) {
  // Not `.at(-1)`: tsconfig targets ES2020, where Array.prototype.at does not
  // exist. `noUncheckedIndexedAccess` is why the fallback is not decorative.
  const name = pathSegments[pathSegments.length - 1] ?? "";
  const ancestors = pathSegments.slice(0, -1);

  return (
    <div className="flex items-center gap-2 px-4 pt-2">
      {/* A label, not navigation: no links, no click targets, and aria-hidden
          separators so a screen reader reads the names rather than a row of
          greater-than signs. */}
      <div className="flex min-w-0 flex-1 items-center gap-1 text-xs">
        {/* The path shrinks and the name does not. When the window is narrow
            it is the collection and folders that should ellipsise — the
            request's own name is the part you are actually reading. */}
        {ancestors.length > 0 && (
          <>
            <span className="min-w-0 truncate text-muted-foreground">
              {ancestors.map((segment, index) => (
                <span key={`${index}-${segment}`}>
                  {index > 0 && (
                    <span aria-hidden className="px-1 text-muted-foreground/60">
                      &gt;
                    </span>
                  )}
                  {segment}
                </span>
              ))}
            </span>
            {/* Outside the truncating span on purpose: separators between the
                ancestors can vanish with the text they belong to, but the one
                joining the path to the name must not, or a clipped path runs
                straight into the request name. */}
            <span aria-hidden className="shrink-0 px-1 text-muted-foreground/60">
              &gt;
            </span>
          </>
        )}
        <span className="shrink-0 font-medium text-foreground">{name}</span>
      </div>

      <button aria-label="Copy as cURL" className={ACTION} onClick={onCopyAsCurl} type="button">
        <Copy aria-hidden className="h-3.5 w-3.5" />
        cURL
      </button>

      <button aria-label="Cookies" className={ACTION} onClick={onManageCookies} type="button">
        <Cookie aria-hidden className="h-3.5 w-3.5" />
        Cookies
      </button>

      <button aria-label="Save request" className={ACTION} onClick={onSave} type="button">
        <Save aria-hidden className="h-3.5 w-3.5" />
        Save
      </button>
    </div>
  );
}
