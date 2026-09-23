// http_client/src/features/response-viewer/SaveExampleButton.tsx
//
// Shared by the ordinary response viewer and the event-stream one.
// Presentational.

/**
 * Null-shaped on purpose: when the response cannot be saved as an example
 * the button stays, disabled, carrying the reason — an action that silently
 * disappears is harder to understand than one that says why it is off.
 */
export type SaveExampleAction = { onSave: () => void } | { disabledReason: string };

export function SaveExampleButton({ action }: { action: SaveExampleAction }) {
  if ("onSave" in action) {
    return (
      <button
        className="mr-1 rounded px-2 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground active:bg-accent/80"
        onClick={action.onSave}
        type="button"
      >
        Save response
      </button>
    );
  }
  return (
    <button
      className="mr-1 rounded px-2 py-1 text-xs text-muted-foreground"
      disabled
      title={action.disabledReason}
      type="button"
    >
      Save response
    </button>
  );
}
