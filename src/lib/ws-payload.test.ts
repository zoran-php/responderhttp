import { describe, expect, it } from "vitest";

import {
  editorLanguage,
  encodeOutgoing,
  hexDump,
  hexToBase64,
  jsonWarning,
  parseBase64,
  parseHex,
  payloadPreview,
  PREVIEW_CHARS,
} from "@/lib/ws-payload";

describe("parseHex", () => {
  it("accepts spaces, newlines, upper case and 0x prefixes, and returns the wire spelling", () => {
    expect(parseHex("de ad")).toEqual({ ok: true, hex: "dead" });
    expect(parseHex("0xDEAD")).toEqual({ ok: true, hex: "dead" });
    expect(parseHex("  0xde\n0xAD\tbeef ")).toEqual({ ok: true, hex: "deadbeef" });
  });

  it("treats nothing typed as an empty binary message", () => {
    expect(parseHex("")).toEqual({ ok: true, hex: "" });
    expect(parseHex("   ")).toEqual({ ok: true, hex: "" });
  });

  it("names the byte offset of a character that is not hex", () => {
    expect(parseHex("de ad zz")).toEqual({ ok: false, error: 'Byte 2: "zz" is not a hex byte' });
    expect(parseHex("deadbeXf")).toEqual({ ok: false, error: 'Byte 3: "Xf" is not a hex byte' });
  });

  it("names the byte offset of a group with an odd number of digits", () => {
    expect(parseHex("de a")).toEqual({
      ok: false,
      error: 'Byte 1: "a" has an odd number of hex digits',
    });
  });
});

describe("parseBase64 and hexToBase64", () => {
  it("decodes to the wire hex, tolerating whitespace, missing padding and the URL-safe alphabet", () => {
    expect(parseBase64("3q2+7w==")).toEqual({ ok: true, hex: "deadbeef" });
    expect(parseBase64(" 3q2+\n7w ")).toEqual({ ok: true, hex: "deadbeef" });
    expect(parseBase64("3q2-7w")).toEqual({ ok: true, hex: "deadbeef" });
    expect(parseBase64("")).toEqual({ ok: true, hex: "" });
  });

  it("names the first character that is not Base64", () => {
    expect(parseBase64("3q2*7w==")).toEqual({
      ok: false,
      error: 'Character 3: "*" is not valid Base64',
    });
    expect(parseBase64("ab=c")).toEqual({
      ok: false,
      error: 'Character 2: "=" is not valid Base64',
    });
    expect(parseBase64("abcde").ok).toBe(false);
  });

  it("round-trips every byte", () => {
    const hex = Array.from({ length: 256 }, (_, byte) => byte.toString(16).padStart(2, "0")).join(
      "",
    );

    expect(parseBase64(hexToBase64(hex))).toEqual({ ok: true, hex });
    expect(hexToBase64("deadbeef")).toBe("3q2+7w==");
  });
});

describe("encodeOutgoing", () => {
  it("sends text, JSON, XML and HTML as a text frame exactly as typed, even when invalid", () => {
    for (const format of ["text", "json", "xml", "html"] as const) {
      expect(encodeOutgoing(format, "base64", "{oops <a>")).toEqual({
        ok: true,
        payload: { kind: "text", text: "{oops <a>" },
      });
    }
  });

  it("sends binary as a binary frame in either encoding, and refuses what does not parse", () => {
    expect(encodeOutgoing("binary", "hex", "0xde ad")).toEqual({
      ok: true,
      payload: { kind: "binary", hex: "dead" },
    });
    expect(encodeOutgoing("binary", "base64", "3q0=")).toEqual({
      ok: true,
      payload: { kind: "binary", hex: "dead" },
    });
    expect(encodeOutgoing("binary", "hex", "xyz").ok).toBe(false);
    expect(encodeOutgoing("binary", "base64", "***").ok).toBe(false);
  });
});

describe("editorLanguage", () => {
  it("follows the format, with binary as plain text", () => {
    expect(
      ["text", "json", "xml", "html", "binary"].map((f) => editorLanguage(f as never)),
    ).toEqual(["plaintext", "json", "xml", "html", "plaintext"]);
  });
});

describe("jsonWarning", () => {
  it("is null for valid JSON and for an empty composer", () => {
    expect(jsonWarning('{"a": [1, 2]}')).toBeNull();
    expect(jsonWarning("  ")).toBeNull();
  });

  it("describes invalid JSON without blocking anything", () => {
    expect(jsonWarning("{oops")).toMatch(/^Not valid JSON: /);
  });
});

describe("hexDump", () => {
  it("shows offset, two groups of eight bytes and the printable characters", () => {
    const hex = "48656c6c6f2c20576f726c64210a00ff" + "41";

    expect(hexDump(hex).split("\n")).toEqual([
      "00000000  48 65 6c 6c 6f 2c 20 57  6f 72 6c 64 21 0a 00 ff  |Hello, World!...|",
      "00000010  41                                                |A|",
    ]);
  });

  it("is empty for an empty message", () => {
    expect(hexDump("")).toBe("");
  });
});

describe("payloadPreview", () => {
  it("folds line breaks so pretty-printed JSON reads on one line", () => {
    expect(payloadPreview({ kind: "text", text: '{\n  "a": 1\n}' })).toBe('{ "a": 1 }');
  });

  it("cuts long text with an ellipsis", () => {
    const preview = payloadPreview({ kind: "text", text: "x".repeat(PREVIEW_CHARS + 5) });

    expect(preview).toBe(`${"x".repeat(PREVIEW_CHARS)}…`);
  });

  /** 13h: previews are rebuilt on every render of the log, so their cost
   * must not grow with the message. */
  it("looks at a bounded window of a huge message, and still cuts it", () => {
    const huge = "a ".repeat(512 * 1024);

    const started = performance.now();
    const preview = payloadPreview({ kind: "text", text: huge });
    const binary = payloadPreview({ kind: "binary", hex: "ab".repeat(1024 * 1024) });
    const elapsed = performance.now() - started;

    expect(preview).toBe(`${"a ".repeat(PREVIEW_CHARS / 2).slice(0, PREVIEW_CHARS)}…`);
    expect(binary).toBe(`Binary, 1048576 bytes: ${Array<string>(16).fill("ab").join(" ")} …`);
    // Generous: the unbounded version took over 100 ms for these two.
    expect(elapsed).toBeLessThan(20);
  });

  it("marks a text message cut even when its whitespace folds it short", () => {
    const text = `${"x".repeat(10)}${" ".repeat(PREVIEW_CHARS * 8)}tail`;

    expect(payloadPreview({ kind: "text", text })).toBe(`${"x".repeat(10)}…`);
  });

  it("shows binary as its size and its first bytes", () => {
    expect(payloadPreview({ kind: "binary", hex: "deadbeef" })).toBe(
      "Binary, 4 bytes: de ad be ef",
    );
    expect(payloadPreview({ kind: "binary", hex: "00".repeat(20) })).toBe(
      `Binary, 20 bytes: ${Array<string>(16).fill("00").join(" ")} …`,
    );
    expect(payloadPreview({ kind: "binary", hex: "" })).toBe("Binary, 0 bytes");
  });

  /** Found in use: a message sent as Base64 read back as hex in the log. */
  it("shows binary in Base64 when that is the encoding in use", () => {
    expect(payloadPreview({ kind: "binary", hex: "deadbeef" }, "base64")).toBe(
      "Binary, 4 bytes: 3q2+7w==",
    );
    const long = payloadPreview({ kind: "binary", hex: "00".repeat(100) }, "base64");
    expect(long).toBe(`Binary, 100 bytes: ${"A".repeat(64)}…`);
  });
});
