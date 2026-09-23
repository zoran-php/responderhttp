// http_client/src/lib/beautify.ts
//
// The WebSocket composer's Beautify button, for JSON, XML and HTML. Pure and
// tested here rather than in the component (CLAUDE.md section 6).
//
// When the text does not parse, the answer is the reason and never a
// rewritten message: a formatter that "repairs" a broken document would send
// something the user did not write.
import type { WsMessageFormat } from "@/types/websocket";

export type BeautifyResult = { ok: true; text: string } | { ok: false; reason: string };

export type BeautifiableFormat = "json" | "xml" | "html";

const INDENT = "  ";

/** HTML elements that never have a closing tag. */
const VOID_ELEMENTS = new Set([
  "area",
  "base",
  "br",
  "col",
  "embed",
  "hr",
  "img",
  "input",
  "link",
  "meta",
  "param",
  "source",
  "track",
  "wbr",
]);

/** HTML elements whose content is raw text, not markup. */
const RAW_TEXT_ELEMENTS = new Set(["script", "style"]);

export function isBeautifiable(format: WsMessageFormat): format is BeautifiableFormat {
  return format === "json" || format === "xml" || format === "html";
}

export function beautify(format: BeautifiableFormat, text: string): BeautifyResult {
  if (text.trim() === "") {
    return { ok: true, text };
  }
  return format === "json" ? beautifyJson(text) : beautifyMarkup(text, format);
}

function beautifyJson(text: string): BeautifyResult {
  try {
    return { ok: true, text: JSON.stringify(JSON.parse(text), null, 2) };
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    return { ok: false, reason: `Not valid JSON: ${detail}` };
  }
}

type Token =
  | { kind: "open"; name: string; raw: string; selfClosing: boolean }
  | { kind: "close"; name: string; raw: string }
  /** Declarations, processing instructions, comments, CDATA, DOCTYPE. */
  | { kind: "other"; raw: string }
  | { kind: "text"; raw: string };

type Scan = { ok: true; tokens: Token[] } | { ok: false; reason: string };

const NAME = /^<\/?([A-Za-z_:][\w:.-]*)/;

/**
 * Splits markup into tags and text. XML is strict: a `<` that does not
 * start a tag, or a tag that never ends, is an error. HTML treats such a `<`
 * as text, as browsers do, and reads script and style content as raw text.
 */
function scan(text: string, html: boolean): Scan {
  const tokens: Token[] = [];
  let at = 0;
  while (at < text.length) {
    const lt = text.indexOf("<", at);
    if (lt === -1) {
      tokens.push({ kind: "text", raw: text.slice(at) });
      break;
    }
    if (lt > at) {
      tokens.push({ kind: "text", raw: text.slice(at, lt) });
    }
    for (const [start, end] of [
      ["<!--", "-->"],
      ["<![CDATA[", "]]>"],
    ] as const) {
      if (text.startsWith(start, lt)) {
        const close = text.indexOf(end, lt + start.length);
        if (close === -1) {
          return { ok: false, reason: `Character ${lt}: ${start} is never closed` };
        }
        tokens.push({ kind: "other", raw: text.slice(lt, close + end.length) });
        at = close + end.length;
        break;
      }
    }
    if (at > lt) {
      continue;
    }
    const gt = tagEnd(text, lt);
    if (gt === -1) {
      if (html) {
        tokens.push({ kind: "text", raw: text.slice(lt) });
        break;
      }
      return { ok: false, reason: `Character ${lt}: this tag is never closed with ">"` };
    }
    const raw = text.slice(lt, gt + 1);
    at = gt + 1;
    if (raw.startsWith("<?") || raw.startsWith("<!")) {
      tokens.push({ kind: "other", raw });
      continue;
    }
    const name = NAME.exec(raw)?.[1];
    if (name === undefined) {
      if (html) {
        tokens.push({ kind: "text", raw });
        continue;
      }
      return { ok: false, reason: `Character ${lt}: "${raw}" is not a tag` };
    }
    const key = html ? name.toLowerCase() : name;
    if (raw.startsWith("</")) {
      tokens.push({ kind: "close", name: key, raw });
      continue;
    }
    const selfClosing = /\/\s*>$/.test(raw) || (html && VOID_ELEMENTS.has(key));
    tokens.push({ kind: "open", name: key, raw, selfClosing });
    if (html && !selfClosing && RAW_TEXT_ELEMENTS.has(key)) {
      const close = text.toLowerCase().indexOf(`</${key}`, at);
      const contentEnd = close === -1 ? text.length : close;
      if (contentEnd > at) {
        tokens.push({ kind: "text", raw: text.slice(at, contentEnd) });
      }
      at = contentEnd;
    }
  }
  return { ok: true, tokens };
}

/** The `>` that ends the tag starting at `lt`, skipping quoted values. */
function tagEnd(text: string, lt: number): number {
  let quote: string | null = null;
  for (let index = lt + 1; index < text.length; index += 1) {
    const char = text.charAt(index);
    if (quote !== null) {
      if (char === quote) {
        quote = null;
      }
    } else if (char === '"' || char === "'") {
      quote = char;
    } else if (char === ">") {
      return index;
    } else if (char === "<" && index === lt + 1) {
      return -1;
    }
  }
  return -1;
}

/**
 * XML must balance exactly. HTML is forgiving the way browsers are: a
 * closing tag with no match is ignored, one that skips open elements closes
 * them, and elements left open at the end are fine.
 */
function checkXmlBalance(tokens: Token[]): string | null {
  const open: string[] = [];
  let roots = 0;
  for (const token of tokens) {
    if (token.kind === "open") {
      if (open.length === 0) {
        roots += 1;
      }
      if (!token.selfClosing) {
        open.push(token.name);
      }
    } else if (token.kind === "close") {
      const top = open.pop();
      if (top === undefined) {
        return `</${token.name}> has no opening tag`;
      }
      if (top !== token.name) {
        return `</${token.name}> closes <${top}>`;
      }
    } else if (token.kind === "text" && open.length === 0 && token.raw.trim() !== "") {
      return "Text outside the root element";
    }
  }
  const unclosed = open.pop();
  if (unclosed !== undefined) {
    return `<${unclosed}> is never closed`;
  }
  if (roots === 0) {
    return "There is no element";
  }
  return null;
}

function beautifyMarkup(text: string, format: "xml" | "html"): BeautifyResult {
  const html = format === "html";
  const label = html ? "HTML" : "XML";
  const scanned = scan(text.trim(), html);
  if (!scanned.ok) {
    return { ok: false, reason: `Not valid ${label}: ${scanned.reason}` };
  }
  const { tokens } = scanned;
  if (!tokens.some((token) => token.kind === "open" || token.kind === "close")) {
    return { ok: false, reason: `Not ${label}: there is no element to format` };
  }
  if (!html) {
    const problem = checkXmlBalance(tokens);
    if (problem !== null) {
      return { ok: false, reason: `Not valid XML: ${problem}` };
    }
  }
  return { ok: true, text: layOut(tokens, html) };
}

/**
 * One tag per line, children indented. An element holding only a short run
 * of text stays on one line, `<b>like this</b>`, since splitting it helps
 * nobody read it.
 */
function layOut(tokens: Token[], html: boolean): string {
  const lines: string[] = [];
  const open: string[] = [];
  const pad = () => INDENT.repeat(open.length);

  for (let index = 0; index < tokens.length; index += 1) {
    const token = tokens[index];
    if (token === undefined) {
      break;
    }
    if (token.kind === "text") {
      const trimmed = token.raw.trim();
      if (trimmed !== "") {
        const raw = open.length > 0 && RAW_TEXT_ELEMENTS.has(open[open.length - 1] ?? "");
        const content = raw ? trimmed.split("\n").map((line) => line.trim()) : [collapse(trimmed)];
        for (const line of content) {
          if (line !== "") {
            lines.push(`${pad()}${line}`);
          }
        }
      }
      continue;
    }
    if (token.kind === "other") {
      lines.push(`${pad()}${token.raw}`);
      continue;
    }
    if (token.kind === "open") {
      const text = tokens[index + 1];
      const close = tokens[index + 2];
      if (
        !token.selfClosing &&
        text?.kind === "text" &&
        close?.kind === "close" &&
        close.name === token.name &&
        !text.raw.includes("\n")
      ) {
        lines.push(`${pad()}${token.raw}${collapse(text.raw.trim())}${close.raw}`);
        index += 2;
        continue;
      }
      lines.push(`${pad()}${token.raw}`);
      if (!token.selfClosing) {
        open.push(token.name);
      }
      continue;
    }
    // A closing tag. HTML may skip open elements (implicitly closing them)
    // or close one that is not open at all (ignored for depth).
    const depth = open.lastIndexOf(token.name);
    if (depth === -1) {
      if (!html) {
        open.pop();
      }
      lines.push(`${pad()}${token.raw}`);
      continue;
    }
    open.length = depth;
    lines.push(`${pad()}${token.raw}`);
  }
  return lines.join("\n");
}

function collapse(text: string): string {
  return text.replace(/\s+/g, " ");
}
