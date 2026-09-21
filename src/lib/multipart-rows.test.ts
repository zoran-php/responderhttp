// http_client/src/lib/multipart-rows.test.ts
import { describe, expect, it } from "vitest";

import {
  basename,
  emptyMultipartRow,
  rowsFromMultipartParts,
  toMultipartParts,
  updateMultipartRow,
  withTrailingBlankPart,
  type MultipartRow,
} from "@/lib/multipart-rows";

function textRow(name: string, value: string): MultipartRow {
  return { ...emptyMultipartRow(), name, value };
}

function fileRow(name: string, path: string): MultipartRow {
  return {
    ...emptyMultipartRow(),
    kind: "file",
    name,
    path,
    fileName: basename(path),
    contentType: "image/png",
  };
}

describe("toMultipartParts", () => {
  it("keeps text and file rows with their own shapes", () => {
    const parts = toMultipartParts([textRow("caption", "hello"), fileRow("upload", "/tmp/a.png")]);

    expect(parts).toEqual([
      { kind: "text", name: "caption", value: "hello" },
      { kind: "file", name: "upload", path: "/tmp/a.png", contentType: "image/png" },
    ]);
  });

  it("drops the unnamed trailing row", () => {
    expect(toMultipartParts(withTrailingBlankPart([textRow("a", "1")]))).toHaveLength(1);
  });

  /** Sending it would fail validation in the backend for a row the user has
   * simply not finished filling in. */
  it("drops a file row whose file has not been chosen yet", () => {
    const unchosen: MultipartRow = { ...emptyMultipartRow(), kind: "file", name: "upload" };

    expect(toMultipartParts([unchosen])).toEqual([]);
  });

  it("trims the name but leaves the value alone", () => {
    expect(toMultipartParts([textRow("  caption  ", "  hello  ")])).toEqual([
      { kind: "text", name: "caption", value: "  hello  " },
    ]);
  });
});

describe("updateMultipartRow", () => {
  /** Toggling to File and back must not silently discard what was typed. */
  it("keeps the text value when a row is switched to a file and back", () => {
    const first = textRow("caption", "hello");
    const rows = [first];
    const id = first.id;

    const asFile = updateMultipartRow(rows, id, { kind: "file" });
    const backToText = updateMultipartRow(asFile, id, { kind: "text" });

    expect(backToText[0]?.value).toBe("hello");
  });
});

describe("rowsFromMultipartParts", () => {
  it("round-trips through toMultipartParts", () => {
    const parts = toMultipartParts([textRow("caption", "hello"), fileRow("upload", "/tmp/a.png")]);

    expect(toMultipartParts(rowsFromMultipartParts(parts))).toEqual(parts);
  });

  it("leaves a blank row to type into", () => {
    const rows = rowsFromMultipartParts([{ kind: "text", name: "a", value: "1" }]);

    expect(rows).toHaveLength(2);
    expect(rows[1]?.name).toBe("");
  });

  it("shows the file name for a stored path", () => {
    const rows = rowsFromMultipartParts([
      { kind: "file", name: "upload", path: "/home/z/pics/a.png", contentType: null },
    ]);

    expect(rows[0]?.fileName).toBe("a.png");
  });
});

describe("basename", () => {
  /** A collection saved on Windows can be opened on macOS, where the path is
   * a meaningless string but its last segment is still worth showing. */
  it("handles both separators", () => {
    expect(basename("C:\\Users\\Zoran\\notes.txt")).toBe("notes.txt");
    expect(basename("/home/zoran/notes.txt")).toBe("notes.txt");
    expect(basename("notes.txt")).toBe("notes.txt");
  });
});
