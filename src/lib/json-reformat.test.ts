import { describe, expect, it } from "vitest";

import { reformatJson } from "@/lib/json-reformat";

function formatted(text: string): string {
  const result = reformatJson(text);
  if (!result.ok) {
    throw new Error(result.reason);
  }
  return result.text;
}

function reason(text: string): string {
  const result = reformatJson(text);
  if (result.ok) {
    throw new Error(`expected an error, got ${result.text}`);
  }
  return result.reason;
}

describe("reformatJson", () => {
  it("indents objects and arrays the way JSON.stringify does", () => {
    const text = '{"a":1,"b":[true,null,{"c":"d"}],"e":{},"f":[]}';

    expect(formatted(text)).toBe(JSON.stringify(JSON.parse(text), null, 2));
  });

  /** The reason this exists: JSON.parse would print 9007199254740992. */
  it("keeps every number exactly as written, 64-bit integers included", () => {
    expect(formatted('{"id":9007199254740993,"f":1.50,"e":1E+400,"n":-0}')).toBe(
      '{\n  "id": 9007199254740993,\n  "f": 1.50,\n  "e": 1E+400,\n  "n": -0\n}',
    );
  });

  it("keeps strings and escapes exactly as written", () => {
    expect(formatted('["a\\"b","\\u00e9",  "x y"]')).toBe(
      '[\n  "a\\"b",\n  "\\u00e9",\n  "x y"\n]',
    );
  });

  it("formats a lone value, and leaves empty text alone", () => {
    expect(formatted("  42 ")).toBe("42");
    expect(formatted("   ")).toBe("   ");
  });

  it("re-indents text that is already indented differently", () => {
    expect(formatted('{\n\t"a" :\n [ 1 ,2 ]\n}')).toBe('{\n  "a": [\n    1,\n    2\n  ]\n}');
  });

  it("refuses invalid JSON and says where", () => {
    expect(reason('{"a":1,}')).toBe(
      "Not valid JSON: expected a quoted property name at line 1, column 8",
    );
    expect(reason('{"a" 1}')).toBe(
      'Not valid JSON: expected ":" after the property name at line 1, column 6',
    );
    expect(reason("[1 2]")).toBe('Not valid JSON: expected "," or "]" at line 1, column 4');
    expect(reason('{"a":\n  tru}')).toBe('Not valid JSON: unexpected "t" at line 2, column 3');
    expect(reason("[1,")).toBe("Not valid JSON: the JSON ends too early at line 1, column 4");
    expect(reason('"open')).toBe("Not valid JSON: a string is not closed at line 1, column 1");
    expect(reason("{} x")).toBe(
      "Not valid JSON: unexpected text after the end of the JSON at line 1, column 4",
    );
    expect(reason("01")).toContain("unexpected text after the end");
  });
});
