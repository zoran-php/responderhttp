// http_client/src/lib/markdown-format.ts
//
// What each toolbar button does to the text (PLAN.md Phase 12).
//
// Pure, and separate from the editor component, because the interesting part
// is the rules — what happens on an empty selection, what happens when the
// user presses Bold on text that is already bold, where the caret should end
// up — and none of that needs a DOM to test (CLAUDE.md section 6).
//
// The component's only job is to hand over the text and the selection, and to
// put back what comes out.

/** The toolbar's actions. */
export type MarkdownAction =
  | "bold"
  | "italic"
  | "code"
  | "codeBlock"
  | "link"
  | "bulletList"
  | "numberedList"
  | "quote"
  | "heading";

/** A selection in the editor: the same shape a textarea or Monaco reports. */
export interface Selection {
  start: number;
  end: number;
}

export interface FormatResult {
  text: string;
  selection: Selection;
}

/** The placeholder dropped in when a button is pressed with nothing selected. */
const PLACEHOLDER: Record<MarkdownAction, string> = {
  bold: "bold text",
  italic: "italic text",
  code: "code",
  codeBlock: "code",
  link: "link text",
  bulletList: "item",
  numberedList: "item",
  quote: "quote",
  heading: "Heading",
};

/** The characters that go either side, for the actions that wrap. */
const WRAPPERS: Partial<Record<MarkdownAction, string>> = {
  bold: "**",
  italic: "*",
  code: "`",
};

/** The prefix that goes at the start of a line, for the actions that prefix. */
const LINE_PREFIXES: Partial<Record<MarkdownAction, (index: number) => string>> = {
  bulletList: () => "- ",
  numberedList: (index) => `${index + 1}. `,
  quote: () => "> ",
  heading: () => "## ",
};

/**
 * Applies a toolbar action.
 *
 * Wrapping actions toggle: pressing Bold on text that is already bold removes
 * the markers rather than adding a second pair, which is what every editor
 * does and what a user pressing the button twice expects.
 *
 * Line actions apply to every line the selection touches, not only to the
 * first — selecting three lines and pressing the list button should produce
 * three list items.
 */
export function applyFormat(
  text: string,
  selection: Selection,
  action: MarkdownAction,
): FormatResult {
  const start = Math.max(0, Math.min(selection.start, text.length));
  const end = Math.max(start, Math.min(selection.end, text.length));

  if (action === "link") {
    return applyLink(text, { start, end });
  }
  if (action === "codeBlock") {
    return applyCodeBlock(text, { start, end });
  }

  const wrapper = WRAPPERS[action];
  if (wrapper !== undefined) {
    return applyWrapper(text, { start, end }, wrapper, PLACEHOLDER[action]);
  }

  return applyLinePrefix(text, { start, end }, action);
}

function applyWrapper(
  text: string,
  selection: Selection,
  wrapper: string,
  placeholder: string,
): FormatResult {
  const { start, end } = selection;
  const selected = text.slice(start, end);
  const width = wrapper.length;

  // Already wrapped, with the markers inside the selection: `**this**` with
  // all eight characters selected.
  const innerWrapped =
    selected.length >= width * 2 &&
    selected.startsWith(wrapper) &&
    selected.endsWith(wrapper) &&
    // Not the middle of a longer run. Italic's marker is one asterisk, so
    // `**bold**` looks wrapped to it; stripping one asterisk from each end
    // would turn bold into italic rather than adding emphasis. If what is
    // left is still fenced by the same character, this is a longer run and
    // the action should add a level, not remove one.
    !isFencedBy(selected.slice(width, selected.length - width), wrapper.charAt(0));

  if (innerWrapped) {
    const inner = selected.slice(width, selected.length - width);
    return {
      text: text.slice(0, start) + inner + text.slice(end),
      selection: { start, end: start + inner.length },
    };
  }

  // Already wrapped, with the markers just outside the selection: `**this**`
  // with only `this` selected. The same longer-run guard applies, one
  // position further out.
  const outerWrapped =
    start >= width &&
    text.slice(start - width, start) === wrapper &&
    text.slice(end, end + width) === wrapper &&
    text.slice(start - width - 1, start - width) !== wrapper.charAt(0);

  if (outerWrapped) {
    return {
      text: text.slice(0, start - width) + selected + text.slice(end + width),
      selection: { start: start - width, end: start - width + selected.length },
    };
  }

  const body = selected.length > 0 ? selected : placeholder;
  return {
    text: text.slice(0, start) + wrapper + body + wrapper + text.slice(end),
    selection: { start: start + width, end: start + width + body.length },
  };
}

/** Whether `text` both starts and ends with `character`. */
function isFencedBy(text: string, character: string): boolean {
  return text.startsWith(character) && text.endsWith(character);
}

/**
 * `[text](url)`, with the caret left on the url so the next thing typed
 * replaces it — the part the user still has to supply.
 */
function applyLink(text: string, selection: Selection): FormatResult {
  const { start, end } = selection;
  const selected = text.slice(start, end);
  const label = selected.length > 0 ? selected : PLACEHOLDER.link;
  const url = "https://";
  const inserted = `[${label}](${url})`;
  const urlStart = start + label.length + 3;

  return {
    text: text.slice(0, start) + inserted + text.slice(end),
    selection: { start: urlStart, end: urlStart + url.length },
  };
}

/**
 * A fenced block on its own lines. The blank-line padding matters: a fence
 * that starts on the same line as a paragraph is not a code block at all.
 */
function applyCodeBlock(text: string, selection: Selection): FormatResult {
  const { start, end } = selection;
  const selected = text.slice(start, end);
  const body = selected.length > 0 ? selected : PLACEHOLDER.codeBlock;
  const before = start > 0 && !text.slice(0, start).endsWith("\n") ? "\n" : "";
  const after = end < text.length && !text.slice(end).startsWith("\n") ? "\n" : "";
  const inserted = `${before}\`\`\`\n${body}\n\`\`\`${after}`;
  const bodyStart = start + before.length + 4;

  return {
    text: text.slice(0, start) + inserted + text.slice(end),
    selection: { start: bodyStart, end: bodyStart + body.length },
  };
}

function applyLinePrefix(text: string, selection: Selection, action: MarkdownAction): FormatResult {
  const prefixFor = LINE_PREFIXES[action];
  if (prefixFor === undefined) {
    return { text, selection };
  }

  const lineStart = text.lastIndexOf("\n", Math.max(0, selection.start - 1)) + 1;
  const lineEndIndex = text.indexOf("\n", selection.end);
  const lineEnd = lineEndIndex === -1 ? text.length : lineEndIndex;
  const block = text.slice(lineStart, lineEnd);
  const lines = block.split("\n");

  const everyLinePrefixed = lines.every((line, index) => line.startsWith(prefixFor(index)));

  const rewritten = lines
    .map((line, index) => {
      const prefix = prefixFor(index);
      if (everyLinePrefixed) {
        return line.slice(prefix.length);
      }
      // An empty document, or an empty line the user is standing on, gets the
      // placeholder so the button visibly does something.
      const body = line.length > 0 ? line : PLACEHOLDER[action];
      return prefix + body;
    })
    .join("\n");

  return {
    text: text.slice(0, lineStart) + rewritten + text.slice(lineEnd),
    selection: { start: lineStart, end: lineStart + rewritten.length },
  };
}
