// http_client/src/features/request-builder/SendButton.tsx
//
// Split button: Send on the left, a chevron on the right opening the
// variants. The reason it is a split rather than two
// buttons is that Send is what you want nine times out of ten — the variant
// should cost a second click, not half the width of the primary action.
//
// Presentational: it knows nothing about requests, only that it has a
// primary action and a menu of alternatives.
import { useEffect, useRef, useState } from "react";
import { ChevronDown, Download, Send } from "lucide-react";

interface SendButtonProps {
  onSend: () => void;
  onSendAndDownload: () => void;
}

export function SendButton({ onSend, onSendAndDownload }: SendButtonProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) {
      return;
    }
    function onPointerDown(event: PointerEvent) {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        setOpen(false);
      }
    }
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setOpen(false);
      }
    }
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  return (
    <div className="relative" ref={ref}>
      <div className="flex">
        {/* Submits the form, so Enter in the URL bar still sends. */}
        <button
          className="inline-flex h-9 items-center gap-2 rounded-l-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90 active:bg-primary/80"
          type="submit"
        >
          <Send className="h-4 w-4" aria-hidden />
          Send
        </button>
        <span aria-hidden className="w-px bg-primary-foreground/25" />
        <button
          aria-expanded={open}
          aria-haspopup="menu"
          aria-label="Send options"
          className="inline-flex h-9 items-center rounded-r-md bg-primary px-1.5 text-primary-foreground hover:bg-primary/90 active:bg-primary/80"
          onClick={() => setOpen((wasOpen) => !wasOpen)}
          type="button"
        >
          <ChevronDown className="h-4 w-4" aria-hidden />
        </button>
      </div>

      {open && (
        <div
          className="absolute right-0 z-50 mt-1 min-w-56 rounded-md border border-border bg-popover py-1 text-sm text-popover-foreground shadow-lg"
          role="menu"
        >
          <button
            className="flex w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-accent"
            onClick={() => {
              setOpen(false);
              onSend();
            }}
            role="menuitem"
            type="button"
          >
            <Send className="h-3.5 w-3.5" aria-hidden />
            Send
          </button>
          <button
            className="flex w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-accent"
            onClick={() => {
              setOpen(false);
              onSendAndDownload();
            }}
            role="menuitem"
            type="button"
          >
            <Download className="h-3.5 w-3.5" aria-hidden />
            Send and download
          </button>
        </div>
      )}
    </div>
  );
}
