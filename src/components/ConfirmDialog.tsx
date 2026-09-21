// http_client/src/components/ConfirmDialog.tsx
//
// The one confirmation prompt for destructive actions (delete collection /
// folder / request) — a custom dialog rather than window.confirm so it is
// styled like the rest of the UI.
import { Modal } from "@/components/Modal";

interface ConfirmDialogProps {
  title: string;
  message: string;
  confirmLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({
  title,
  message,
  confirmLabel = "Delete",
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  return (
    <Modal onClose={onCancel} title={title}>
      <p className="mb-4 text-sm text-muted-foreground">{message}</p>
      <div className="flex justify-end gap-2">
        <button
          className="h-8 rounded-md border border-input px-3 text-sm hover:bg-accent active:bg-accent/70"
          onClick={onCancel}
          type="button"
        >
          Cancel
        </button>
        <button
          className="h-8 rounded-md bg-destructive px-3 text-sm font-medium text-destructive-foreground hover:bg-destructive/90 active:bg-destructive/80"
          onClick={onConfirm}
          type="button"
        >
          {confirmLabel}
        </button>
      </div>
    </Modal>
  );
}
