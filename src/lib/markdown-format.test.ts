// http_client/src/lib/markdown-format.test.ts
import { describe, expect, it } from "vitest";

import { applyFormat } from "@/lib/markdown-format";

/** The text with the returned selection marked, so a test reads as one line. */
function marked(text: string, selection: { start: number; end: number }): string {
  return `${text.slice(0, selection.start)}[${text.slice(selection.start, selection.end)}]${text.slice(selection.end)}`;
}

describe("applyFormat", () => {
  it("wraps the selection and leaves it selected", () => {
    const result = applyFormat("make this loud", { start: 5, end: 9 }, "bold");

    expect(result.text).toBe("make **this** loud");
    expect(marked(result.text, result.selection)).toBe("make **[this]** loud");
  });

  it("inserts a placeholder when nothing is selected, ready to type over", () => {
    const result = applyFormat("", { start: 0, end: 0 }, "bold");

    expect(result.text).toBe("**bold text**");
    expect(marked(result.text, result.selection)).toBe("**[bold text]**");
  });

  /** Pressing the button twice is how a user undoes it. */
  it("unwraps text that is already wrapped", () => {
    const result = applyFormat("make **this** loud", { start: 5, end: 13 }, "bold");

    expect(result.text).toBe("make this loud");
  });

  it("unwraps when the markers sit just outside the selection", () => {
    const result = applyFormat("make **this** loud", { start: 7, end: 11 }, "bold");

    expect(result.text).toBe("make this loud");
    expect(marked(result.text, result.selection)).toBe("make [this] loud");
  });

  it("does not mistake bold for italic", () => {
    const result = applyFormat("**already bold**", { start: 0, end: 16 }, "italic");

    expect(result.text).toBe("***already bold***");
  });

  it("wraps inline code in backticks", () => {
    const result = applyFormat("call getUser now", { start: 5, end: 12 }, "code");

    expect(result.text).toBe("call `getUser` now");
  });

  it("puts a fenced block on its own lines", () => {
    const result = applyFormat("", { start: 0, end: 0 }, "codeBlock");

    expect(result.text).toBe("```\ncode\n```");
    expect(marked(result.text, result.selection)).toBe("```\n[code]\n```");
  });

  /** A fence that starts on the same line as a paragraph is not a fence. */
  it("breaks the line before a fence that follows text", () => {
    const result = applyFormat("intro", { start: 5, end: 5 }, "codeBlock");

    expect(result.text).toBe("intro\n```\ncode\n```");
  });

  it("leaves the caret on the url of a new link", () => {
    const result = applyFormat("see the docs", { start: 8, end: 12 }, "link");

    expect(result.text).toBe("see the [docs](https://)");
    expect(marked(result.text, result.selection)).toBe("see the [docs]([https://])");
  });

  it("prefixes every line the selection touches, not only the first", () => {
    const result = applyFormat("one\ntwo\nthree", { start: 0, end: 13 }, "bulletList");

    expect(result.text).toBe("- one\n- two\n- three");
  });

  it("numbers a numbered list from one", () => {
    const result = applyFormat("one\ntwo\nthree", { start: 0, end: 13 }, "numberedList");

    expect(result.text).toBe("1. one\n2. two\n3. three");
  });

  it("removes the prefix when every line already has it", () => {
    const result = applyFormat("- one\n- two", { start: 0, end: 11 }, "bulletList");

    expect(result.text).toBe("one\ntwo");
  });

  /** A caret inside a line should affect that whole line, not split it. */
  it("prefixes the whole line the caret is inside", () => {
    const result = applyFormat("first\nsecond", { start: 8, end: 8 }, "quote");

    expect(result.text).toBe("first\n> second");
  });

  it("prefixes a heading and a quote with their own markers", () => {
    expect(applyFormat("Title", { start: 0, end: 5 }, "heading").text).toBe("## Title");
    expect(applyFormat("Said so", { start: 0, end: 7 }, "quote").text).toBe("> Said so");
  });

  it("gives an empty line a placeholder so the button does something visible", () => {
    const result = applyFormat("", { start: 0, end: 0 }, "bulletList");

    expect(result.text).toBe("- item");
  });

  it("leaves the text alone when the selection runs past its end", () => {
    const result = applyFormat("short", { start: 2, end: 99 }, "bold");

    expect(result.text).toBe("sh**ort**");
  });

  it("treats a backwards selection as a caret at its start", () => {
    const result = applyFormat("abc", { start: 2, end: 1 }, "bold");

    expect(result.text).toBe("ab**bold text**c");
  });
});
