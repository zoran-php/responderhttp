// http_client/src/lib/json-reformat.ts
//
// Re-indents JSON text without parsing it into JavaScript values. The gRPC
// message editor needs this for Beautify: `JSON.parse` turns
// 9007199254740993 into 9007199254740992, and an int64 field would silently
// change (PLAN-GRPC.md section 3, point 5). Every token here is copied
// through exactly as written; only the whitespace between tokens changes.
//
// Text that is not valid JSON is refused with where it went wrong, never
// "repaired", as Beautify does for the WebSocket composer.
import type { BeautifyResult } from "@/lib/beautify";

const NUMBER = /-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/y;
const LITERAL = /true|false|null/y;

class JsonSyntaxError extends Error {}

export function reformatJson(text: string, indent = "  "): BeautifyResult {
  if (text.trim() === "") {
    return { ok: true, text };
  }
  const reader = new Reader(text);
  try {
    reader.skipSpace();
    const out = reader.value(0, indent);
    reader.skipSpace();
    if (!reader.done()) {
      reader.fail("unexpected text after the end of the JSON");
    }
    return { ok: true, text: out };
  } catch (error) {
    if (error instanceof JsonSyntaxError) {
      return { ok: false, reason: `Not valid JSON: ${error.message}` };
    }
    throw error;
  }
}

class Reader {
  private at = 0;

  constructor(private readonly text: string) {}

  done(): boolean {
    return this.at >= this.text.length;
  }

  skipSpace(): void {
    while (!this.done() && " \t\n\r".includes(this.text.charAt(this.at))) {
      this.at += 1;
    }
  }

  fail(message: string): never {
    const before = this.text.slice(0, this.at);
    const line = before.split("\n").length;
    const column = this.at - before.lastIndexOf("\n");
    throw new JsonSyntaxError(`${message} at line ${line}, column ${column}`);
  }

  value(depth: number, indent: string): string {
    const next = this.text.charAt(this.at);
    if (next === "{") {
      return this.container("{", "}", depth, indent, true);
    }
    if (next === "[") {
      return this.container("[", "]", depth, indent, false);
    }
    if (next === '"') {
      return this.string();
    }
    const token = this.match(NUMBER) ?? this.match(LITERAL);
    if (token === null) {
      this.fail(this.done() ? "the JSON ends too early" : `unexpected "${next}"`);
    }
    return token;
  }

  private container(
    open: string,
    close: string,
    depth: number,
    indent: string,
    keyed: boolean,
  ): string {
    this.at += 1;
    this.skipSpace();
    if (this.text.charAt(this.at) === close) {
      this.at += 1;
      return open + close;
    }
    const inner = indent.repeat(depth + 1);
    const items: string[] = [];
    for (;;) {
      this.skipSpace();
      let item = "";
      if (keyed) {
        if (this.text.charAt(this.at) !== '"') {
          this.fail("expected a quoted property name");
        }
        item = `${this.string()}: `;
        this.skipSpace();
        if (this.text.charAt(this.at) !== ":") {
          this.fail('expected ":" after the property name');
        }
        this.at += 1;
        this.skipSpace();
      }
      items.push(inner + item + this.value(depth + 1, indent));
      this.skipSpace();
      const separator = this.text.charAt(this.at);
      this.at += 1;
      if (separator === close) {
        break;
      }
      if (separator !== ",") {
        this.at -= 1;
        this.fail(this.done() ? "the JSON ends too early" : `expected "," or "${close}"`);
      }
    }
    return `${open}\n${items.join(",\n")}\n${indent.repeat(depth)}${close}`;
  }

  private string(): string {
    const start = this.at;
    this.at += 1;
    while (!this.done()) {
      const char = this.text.charAt(this.at);
      if (char === '"') {
        this.at += 1;
        return this.text.slice(start, this.at);
      }
      if (char === "\\") {
        this.at += 2;
        continue;
      }
      if (char < " ") {
        this.fail("a control character inside a string must be escaped");
      }
      this.at += 1;
    }
    this.at = start;
    return this.fail("a string is not closed");
  }

  private match(pattern: RegExp): string | null {
    pattern.lastIndex = this.at;
    const found = pattern.exec(this.text);
    if (found === null) {
      return null;
    }
    this.at += found[0].length;
    return found[0];
  }
}
