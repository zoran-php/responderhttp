// http_client/src/lib/shortcuts.ts
//
// Which keystroke is which. Pure, and takes a plain object rather than a
// KeyboardEvent, so it tests without a DOM (CLAUDE.md §6) — a real
// KeyboardEvent satisfies the shape structurally, so callers pass one
// unchanged.
//
// Worth being a tested function rather than an inline condition: a key matcher
// is exactly the kind of thing that is subtly wrong in a way nobody notices
// until it fires on the wrong chord.

/** The parts of a keydown this app decides on. */
export interface ShortcutEvent {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  repeat: boolean;
}

/** Ctrl+S on Windows and Linux, Cmd+S on macOS. */
export function matchesSaveShortcut(event: ShortcutEvent): boolean {
  // Holding the keys down would otherwise fire a save per repeat.
  if (event.repeat) {
    return false;
  }
  // Ctrl+Shift+S and Ctrl+Alt+S are different chords, and treating them as
  // this one is how a shortcut starts firing when nobody asked it to.
  if (event.altKey || event.shiftKey) {
    return false;
  }
  // Exactly one of the two, never both and never neither: Ctrl+Cmd+S is its
  // own chord, not a sloppy spelling of this one.
  if (event.ctrlKey === event.metaKey) {
    return false;
  }
  // Lowercased because CapsLock reports "S".
  return event.key.toLowerCase() === "s";
}
