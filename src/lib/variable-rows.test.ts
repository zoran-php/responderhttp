// http_client/src/lib/variable-rows.test.ts
import { describe, expect, it } from "vitest";

import {
  emptyVariableRow,
  rowsFromVariables,
  toggleSecret,
  toVariableInputs,
  withEditedRows,
  type VariableRow,
} from "@/lib/variable-rows";

function row(overrides: Partial<VariableRow>): VariableRow {
  return { ...emptyVariableRow(), ...overrides };
}

describe("rowsFromVariables", () => {
  it("keeps the secret flag and state and adds a blank row", () => {
    const rows = rowsFromVariables([
      { name: "token", value: "", secret: true, secretState: "needsReentry" },
    ]);

    expect(rows).toHaveLength(2);
    expect(rows[0]).toMatchObject({ name: "token", secret: true, secretState: "needsReentry" });
    expect(rows[1]).toMatchObject({ name: "", value: "", secret: false });
  });
});

describe("toVariableInputs", () => {
  it("drops unnamed rows, trims names and sends no state", () => {
    const inputs = toVariableInputs([
      row({ name: " token ", value: "abc", secret: true, secretState: "unavailable" }),
      row({ name: "", value: "stray" }),
    ]);

    expect(inputs).toEqual([{ name: "token", value: "abc", secret: true }]);
  });
});

describe("toggleSecret", () => {
  it("flips only the targeted row", () => {
    const first = row({ name: "a" });
    const second = row({ name: "b" });

    const toggled = toggleSecret([first, second], first.id);

    expect(toggled[0]?.secret).toBe(true);
    expect(toggled[1]?.secret).toBe(false);
  });
});

describe("withEditedRows", () => {
  it("keeps each row's flag through a table edit", () => {
    const secret = row({ name: "token", value: "x", secret: true });

    const next = withEditedRows([secret], [{ id: secret.id, name: "token", value: "y" }]);

    expect(next[0]).toMatchObject({ value: "y", secret: true });
  });

  it("clears needs-reentry once the value is typed again", () => {
    const lost = row({ name: "token", secret: true, secretState: "needsReentry" });

    const untouched = withEditedRows([lost], [{ id: lost.id, name: "renamed", value: "" }]);
    const retyped = withEditedRows([lost], [{ id: lost.id, name: "token", value: "new" }]);

    expect(untouched[0]?.secretState).toBe("needsReentry");
    expect(retyped[0]?.secretState).toBe("ok");
  });

  it("keeps an unavailable warning even after typing", () => {
    const locked = row({ name: "token", secret: true, secretState: "unavailable" });

    const next = withEditedRows([locked], [{ id: locked.id, name: "token", value: "new" }]);

    expect(next[0]?.secretState).toBe("unavailable");
  });

  it("drops a removed row and keeps a blank row at the end", () => {
    const kept = row({ name: "a", value: "1" });
    const removed = row({ name: "b", value: "2" });

    const next = withEditedRows([kept, removed], [{ id: kept.id, name: "a", value: "1" }]);

    expect(next.map((candidate) => candidate.name)).toEqual(["a", ""]);
  });
});
