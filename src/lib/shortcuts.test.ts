// http_client/src/lib/shortcuts.test.ts
import { describe, expect, it } from "vitest";

import { matchesSaveShortcut, type ShortcutEvent } from "@/lib/shortcuts";

function press(overrides: Partial<ShortcutEvent>): ShortcutEvent {
  return {
    key: "s",
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    shiftKey: false,
    repeat: false,
    ...overrides,
  };
}

describe("matchesSaveShortcut", () => {
  it("matches Ctrl+S and Cmd+S, because the app builds for both", () => {
    expect(matchesSaveShortcut(press({ ctrlKey: true }))).toBe(true);
    expect(matchesSaveShortcut(press({ metaKey: true }))).toBe(true);
  });

  it("matches whatever case the key arrives in", () => {
    expect(matchesSaveShortcut(press({ ctrlKey: true, key: "S" }))).toBe(true);
  });

  it("ignores a bare s, or the key would fire while typing a URL", () => {
    expect(matchesSaveShortcut(press({}))).toBe(false);
  });

  /** Different chords. Treating them as this one is how a shortcut starts
   * firing when nobody asked it to. */
  it("ignores the same key with another modifier held", () => {
    expect(matchesSaveShortcut(press({ ctrlKey: true, shiftKey: true }))).toBe(false);
    expect(matchesSaveShortcut(press({ ctrlKey: true, altKey: true }))).toBe(false);
    expect(matchesSaveShortcut(press({ metaKey: true, shiftKey: true }))).toBe(false);
  });

  it("ignores Ctrl and Cmd held together, which is its own chord", () => {
    expect(matchesSaveShortcut(press({ ctrlKey: true, metaKey: true }))).toBe(false);
  });

  /** Otherwise holding the keys down saves once per repeat. */
  it("ignores an auto-repeat", () => {
    expect(matchesSaveShortcut(press({ ctrlKey: true, repeat: true }))).toBe(false);
  });

  it("ignores the modifier on another key", () => {
    expect(matchesSaveShortcut(press({ ctrlKey: true, key: "a" }))).toBe(false);
  });
});
