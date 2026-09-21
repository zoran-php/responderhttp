// http_client/src/lib/key-values.test.ts
import { describe, expect, it } from "vitest";

import {
  emptyRow,
  removeRow,
  rowsFromKeyValues,
  toKeyValues,
  updateRow,
  withTrailingBlank,
  type KeyValueRow,
} from "@/lib/key-values";

function rows(...pairs: [string, string][]): KeyValueRow[] {
  return pairs.map(([name, value]) => ({ ...emptyRow(), name, value }));
}

describe("withTrailingBlank", () => {
  it("adds a blank row to type into", () => {
    const result = withTrailingBlank(rows(["a", "1"]));

    expect(result).toHaveLength(2);
    expect(result[1]).toMatchObject({ name: "", value: "" });
  });

  it("does not stack blank rows", () => {
    const once = withTrailingBlank(rows(["a", "1"]));

    expect(withTrailingBlank(once)).toHaveLength(2);
  });

  it("gives every row a distinct id", () => {
    const result = withTrailingBlank(withTrailingBlank([]));
    const ids = new Set(result.map((row) => row.id));

    expect(ids.size).toBe(result.length);
  });
});

describe("updateRow / removeRow", () => {
  it("patches only the addressed row", () => {
    const initial = rows(["a", "1"], ["b", "2"]);
    const target = initial[1];
    if (!target) throw new Error("fixture should have two rows");

    const updated = updateRow(initial, target.id, { value: "changed" });

    expect(updated[0]?.value).toBe("1");
    expect(updated[1]?.value).toBe("changed");
  });

  it("removes by id", () => {
    const initial = rows(["a", "1"], ["b", "2"]);
    const target = initial[0];
    if (!target) throw new Error("fixture should have two rows");

    expect(removeRow(initial, target.id)).toHaveLength(1);
  });
});

describe("toKeyValues", () => {
  it("drops rows with no name and trims the name", () => {
    const result = toKeyValues(rows([" a ", "1"], ["", "orphan"], ["b", ""]));

    expect(result).toEqual([
      { name: "a", value: "1" },
      { name: "b", value: "" },
    ]);
  });
});

describe("rowsFromKeyValues", () => {
  it("gives each pair a fresh row id and appends a trailing blank", () => {
    const result = rowsFromKeyValues([{ name: "a", value: "1" }]);

    expect(result).toHaveLength(2);
    expect(result[0]).toMatchObject({ name: "a", value: "1" });
    expect(result[1]).toMatchObject({ name: "", value: "" });
  });

  it("gives every row a distinct id", () => {
    const result = rowsFromKeyValues([
      { name: "a", value: "1" },
      { name: "b", value: "2" },
    ]);
    const ids = new Set(result.map((row) => row.id));

    expect(ids.size).toBe(result.length);
  });
});
