// http_client/src/components/Modal.tsx
//
// The one modal shell. ConfirmDialog and the save/move dialogs in
// features/collections/ render inside this rather than each building their
// own backdrop and escape-key handling (CLAUDE.md section 6).
//
// Never taller than the window: the title and the optional footer stay put
// and only the body between them scrolls, so a dialog's buttons can always be
// reached however much it has to show (PLAN.md, Phase 8d follow-up).
import { useEffect, type ReactNode } from "react";

interface ModalProps {
  title: string;
  onClose: () => void;
  children: ReactNode;
  /** Wide for dialogs that show a report, not just a form. */
  size?: "default" | "wide";
  /**
   * Pinned below the scrolling body — the place for a dialog's buttons when
   * its content can grow. Rendered as a right-aligned row.
   */
  footer?: ReactNode;
}

const WIDTH_CLASSES: Record<NonNullable<ModalProps["size"]>, string> = {
  default: "max-w-md",
  wide: "max-w-2xl",
};

export function Modal({ title, onClose, children, size = "default", footer }: ModalProps) {
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  return (
    <div
      // The padding is the gap the dialog keeps from every window edge.
      className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-4"
      onClick={onClose}
    >
      <div
        aria-label={title}
        aria-modal="true"
        // Ours, not ARIA: App looks for this to know a modal owns the keyboard
        // and that its shortcuts should stay out of the way. `role="dialog"` is
        // not a safe query for that — Monaco's own widgets use ARIA roles too.
        data-modal
        className={`flex max-h-full w-full ${WIDTH_CLASSES[size]} flex-col rounded-lg border border-border bg-popover text-popover-foreground shadow-lg`}
        onClick={(event) => event.stopPropagation()}
        role="dialog"
      >
        <h2 className="shrink-0 px-4 pb-3 pt-4 text-sm font-semibold">{title}</h2>
        <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-4" data-modal-body>
          {children}
        </div>
        {footer !== undefined && (
          <div
            className="flex shrink-0 items-center justify-end gap-2 border-t border-border px-4 py-3 text-sm"
            data-modal-footer
          >
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}
