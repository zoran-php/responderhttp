// http_client/src/lib/ws-payload.ts
//
// The WebSocket composer's rules and the log's one-line previews (PLAN.md
// Phase 13e). Pure, so the component only renders what these return
// (CLAUDE.md section 6).
//
// Binary payloads are carried as hex strings end to end: that is how they
// cross IPC (commands/dto.rs, WsPayloadDto) and how the log shows them, so
// nothing here ever holds a Uint8Array.
import type { WsBinaryEncoding, WsMessageFormat, WsPayload } from "@/types/websocket";

export type HexParse = { ok: true; hex: string } | { ok: false; error: string };

const HEX_PAIRS = /^(?:[0-9a-fA-F]{2})*$/;

/**
 * What the user typed in Hex format, as the canonical wire spelling Rust
 * accepts: lowercase digit pairs, no separators. Tolerant of whitespace
 * between bytes (spaces, tabs, newlines) and a `0x` prefix on any group, so
 * `de ad`, `0xDEAD` and `0xde 0xad` are all the same two bytes. Anything else
 * fails with the offset of the first bad byte, counted from 0.
 */
export function parseHex(input: string): HexParse {
  let hex = "";
  for (const group of input.split(/\s+/)) {
    if (group === "") {
      continue;
    }
    const digits = /^0x/i.test(group) ? group.slice(2) : group;
    const offset = hex.length / 2;
    if (digits.length % 2 !== 0) {
      return { ok: false, error: `Byte ${offset}: "${group}" has an odd number of hex digits` };
    }
    if (!HEX_PAIRS.test(digits)) {
      const bad = firstBadPair(digits);
      return {
        ok: false,
        error: `Byte ${offset + bad.index}: "${bad.pair}" is not a hex byte`,
      };
    }
    hex += digits.toLowerCase();
  }
  return { ok: true, hex };
}

function firstBadPair(digits: string): { index: number; pair: string } {
  for (let index = 0; index * 2 < digits.length; index += 1) {
    const pair = digits.slice(index * 2, index * 2 + 2);
    if (!/^[0-9a-fA-F]{2}$/.test(pair)) {
      return { index, pair };
    }
  }
  return { index: 0, pair: digits };
}

const BASE64_CHARS = /^[A-Za-z0-9+/]*={0,2}$/;

/**
 * What the user typed in Base64 format, as the canonical hex Rust accepts.
 * Whitespace anywhere is ignored, and missing `=` padding is tolerated; the
 * URL-safe alphabet (`-`, `_`) is accepted too. Anything else fails with the
 * position of the first bad character, counted from 0 in the text as typed
 * with whitespace removed.
 */
export function parseBase64(input: string): HexParse {
  const compact = input.replace(/\s+/g, "").replace(/-/g, "+").replace(/_/g, "/");
  if (!BASE64_CHARS.test(compact)) {
    const index = compact.search(/[^A-Za-z0-9+/=]|=(?=[^=])/);
    const at = index === -1 ? compact.indexOf("=") : index;
    return {
      ok: false,
      error: `Character ${at}: "${compact.charAt(at)}" is not valid Base64`,
    };
  }
  const body = compact.replace(/=+$/, "");
  if (body.length % 4 === 1) {
    return { ok: false, error: "Not valid Base64: the length is one character too long" };
  }
  const binary = atob(body.padEnd(Math.ceil(body.length / 4) * 4, "="));
  let hex = "";
  for (let index = 0; index < binary.length; index += 1) {
    hex += binary.charCodeAt(index).toString(16).padStart(2, "0");
  }
  return { ok: true, hex };
}

/** The wire hex of a binary message, as Base64 for the log and for Copy. */
export function hexToBase64(hex: string): string {
  let binary = "";
  for (let index = 0; index + 1 < hex.length; index += 2) {
    binary += String.fromCharCode(Number.parseInt(hex.slice(index, index + 2), 16));
  }
  return btoa(binary);
}

/** What the composer's text means as binary, in the chosen encoding. */
export function parseBinary(encoding: WsBinaryEncoding, text: string): HexParse {
  return encoding === "hex" ? parseHex(text) : parseBase64(text);
}

export type OutgoingMessage = { ok: true; payload: WsPayload } | { ok: false; error: string };

/**
 * The composer's text as the message to send. Text, JSON, XML and HTML go
 * as a text frame, exactly as typed: nothing is refused for failing to
 * parse. Binary goes as a binary frame and refuses to send until the text
 * parses in its encoding.
 */
export function encodeOutgoing(
  format: WsMessageFormat,
  encoding: WsBinaryEncoding,
  text: string,
): OutgoingMessage {
  if (format !== "binary") {
    return { ok: true, payload: { kind: "text", text } };
  }
  const parsed = parseBinary(encoding, text);
  return parsed.ok
    ? { ok: true, payload: { kind: "binary", hex: parsed.hex } }
    : { ok: false, error: parsed.error };
}

/** The editor language for a format. */
export function editorLanguage(format: WsMessageFormat): "json" | "xml" | "html" | "plaintext" {
  return format === "json" || format === "xml" || format === "html" ? format : "plaintext";
}

/** Null for valid JSON, or for nothing typed yet. */
export function jsonWarning(text: string): string | null {
  if (text.trim() === "") {
    return null;
  }
  try {
    JSON.parse(text);
    return null;
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    return `Not valid JSON: ${detail}`;
  }
}

const DUMP_WIDTH = 16;

/**
 * The classic dump an expanded binary row shows: an 8-digit offset, sixteen
 * bytes split in two groups of eight, and their printable ASCII. `hex` is the
 * wire spelling, as the log holds it.
 */
export function hexDump(hex: string): string {
  const bytes = hex.match(/.{2}/g) ?? [];
  const lines: string[] = [];
  for (let start = 0; start < bytes.length; start += DUMP_WIDTH) {
    const row = bytes.slice(start, start + DUMP_WIDTH);
    const cells = row.concat(Array<string>(DUMP_WIDTH - row.length).fill("  "));
    const left = cells.slice(0, 8).join(" ");
    const right = cells.slice(8).join(" ");
    const ascii = row
      .map((byte) => {
        const code = Number.parseInt(byte, 16);
        return code >= 0x20 && code < 0x7f ? String.fromCharCode(code) : ".";
      })
      .join("");
    lines.push(`${start.toString(16).padStart(8, "0")}  ${left}  ${right}  |${ascii}|`);
  }
  return lines.join("\n");
}

/** How much of a message a collapsed log row shows. */
export const PREVIEW_CHARS = 200;
const PREVIEW_BYTES = 16;
/**
 * Bytes a Base64 preview encodes: a multiple of 3, so the prefix is exactly
 * the start of the whole message's Base64 and no padding appears mid-way.
 */
const PREVIEW_BASE64_BYTES = 48;

/**
 * How much of a text message the preview looks at. Whitespace folding can
 * only shrink text, so a window a few times the preview's length always
 * yields a full preview; it keeps the cost of a row flat however big the
 * message is (PLAN-WEBSOCKET.md 13h: a log of 1 MiB messages spent 1.6 s per
 * render folding whole messages it then cut to 200 characters).
 */
const PREVIEW_WINDOW = PREVIEW_CHARS * 8;

/**
 * The single line a collapsed log row shows. Line breaks and runs of
 * whitespace fold to one space, so pretty-printed JSON still reads on one
 * line; the expanded row shows the message as it was. A binary message is
 * shown in `encoding`, the composer's, so a message sent as Base64 reads
 * back as Base64.
 */
export function payloadPreview(payload: WsPayload, encoding: WsBinaryEncoding = "hex"): string {
  if (payload.kind === "text") {
    const head = payload.text.slice(0, PREVIEW_WINDOW);
    const folded = head.replace(/\s+/g, " ").trim();
    const cut = folded.length > PREVIEW_CHARS || payload.text.length > PREVIEW_WINDOW;
    return cut ? `${folded.slice(0, PREVIEW_CHARS)}…` : folded;
  }
  // Only the bytes that are shown are split out; the size comes from the
  // length, so a 1 MiB payload costs the same as a 16-byte one.
  const byteCount = Math.floor(payload.hex.length / 2);
  const noun = byteCount === 1 ? "byte" : "bytes";
  if (encoding === "base64") {
    const shown = hexToBase64(payload.hex.slice(0, PREVIEW_BASE64_BYTES * 2));
    const more = byteCount > PREVIEW_BASE64_BYTES ? "…" : "";
    return `Binary, ${byteCount} ${noun}${shown === "" ? "" : `: ${shown}${more}`}`;
  }
  const shown = (payload.hex.slice(0, PREVIEW_BYTES * 2).match(/.{2}/g) ?? []).join(" ");
  const more = byteCount > PREVIEW_BYTES ? " …" : "";
  return `Binary, ${byteCount} ${noun}${shown === "" ? "" : `: ${shown}${more}`}`;
}
