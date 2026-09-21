// http_client/src/lib/shortcuts.test.ts
import { describe, expect, it } from "vitest";

import { matchesDocsShortcut, matchesSaveShortcut, type ShortcutEvent } from "@/lib/shortcuts";

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

describe("matchesDocsShortcut", () => {
  const docs = {
    key: "d",
    ctrlKey: true,
    metaKey: false,
    altKey: false,
    shiftKey: true,
    repeat: false,
  };

  it("matches Ctrl+Shift+D and Cmd+Shift+D", () => {
    expect(matchesDocsShortcut(docs)).toBe(true);
    expect(matchesDocsShortcut({ ...docs, ctrlKey: false, metaKey: true })).toBe(true);
  });

  /** Ctrl+D is the browser's bookmark chord; this must not answer to it. */
  it("requires shift rather than tolerating it", () => {
    expect(matchesDocsShortcut({ ...docs, shiftKey: false })).toBe(false);
  });

  it("refuses alt, both modifiers at once, and neither", () => {
    expect(matchesDocsShortcut({ ...docs, altKey: true })).toBe(false);
    expect(matchesDocsShortcut({ ...docs, metaKey: true })).toBe(false);
    expect(matchesDocsShortcut({ ...docs, ctrlKey: false })).toBe(false);
  });

  it("ignores a held key and another letter", () => {
    expect(matchesDocsShortcut({ ...docs, repeat: true })).toBe(false);
    expect(matchesDocsShortcut({ ...docs, key: "s" })).toBe(false);
  });

  /** CapsLock reports an uppercase key. */
  it("matches whatever the case", () => {
    expect(matchesDocsShortcut({ ...docs, key: "D" })).toBe(true);
  });
});
