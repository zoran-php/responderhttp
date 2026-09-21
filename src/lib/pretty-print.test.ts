// http_client/src/lib/pretty-print.test.ts
import { describe, expect, it } from "vitest";

import { prettyPrint } from "@/lib/pretty-print";

describe("prettyPrint", () => {
  it("indents JSON", () => {
    expect(prettyPrint('{"a":1,"b":[2,3]}', "json")).toBe(
      '{\n  "a": 1,\n  "b": [\n    2,\n    3\n  ]\n}',
    );
  });

  it("returns malformed JSON untouched rather than hiding what the server sent", () => {
    expect(prettyPrint('{"a":1', "json")).toBe('{"a":1');
  });

  it("indents nested XML elements", () => {
    expect(prettyPrint("<a><b>1</b><c><d/></c></a>", "xml")).toBe(
      "<a>\n  <b>1</b>\n  <c>\n    <d/>\n  </c>\n</a>",
    );
  });

  it("leaves plain text alone", () => {
    expect(prettyPrint("just words", "plaintext")).toBe("just words");
    expect(prettyPrint("not xml at all", "xml")).toBe("not xml at all");
  });
});
