// http_client/src/features/docs/DocsEditor.test.tsx
//
// The spec's second acceptance scenario, as written: type Markdown in Edit
// mode, switch to Preview, see it rendered.
//
// Monaco is stubbed. It is several megabytes, it does not lay out in jsdom,
// and none of what is being tested here is Monaco's — what matters is that
// the toolbar switches panes, that what is typed reaches the store, and that
// the preview renders it.
//
// Plain assertions rather than jest-dom matchers: this project has no vitest
// setup file, so `toBeInTheDocument` and friends are not registered. Adding
// one for six assertions would change how every other suite starts up.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { DocsEditor } from "@/features/docs/DocsEditor";
import { useTabsStore, type DocsTab } from "@/store/request-store";
import type { DocsTarget } from "@/types/docs";

const itemDocs = vi.hoisted(() => vi.fn());
const setItemDocs = vi.hoisted(() => vi.fn());

vi.mock("@/services/docs", () => ({ itemDocs, setItemDocs }));

vi.mock("@/components/LazyCodeEditor", () => ({
  LazyCodeEditor: ({
    value,
    onChange,
    readOnly,
  }: {
    value: string;
    onChange?: (value: string) => void;
    readOnly?: boolean;
  }) => (
    <textarea
      aria-label="Documentation"
      onChange={(event) => onChange?.(event.target.value)}
      readOnly={readOnly}
      value={value}
    />
  ),
}));

const target: DocsTarget = { kind: "collection", id: "col_1" };

function currentDocsTab(): DocsTab {
  const tab = useTabsStore.getState().tabs.find((candidate) => candidate.kind === "docs");
  if (tab === undefined || tab.kind !== "docs") {
    throw new Error("expected a docs tab");
  }
  return tab;
}

/** Renders the live tab, so a store update re-renders the way App does. */
function DocsEditorHost() {
  const tab = useTabsStore((state) =>
    state.tabs.find((candidate): candidate is DocsTab => candidate.kind === "docs"),
  );
  return tab === undefined ? null : <DocsEditor tab={tab} />;
}

beforeEach(async () => {
  itemDocs.mockReset().mockResolvedValue({ ok: true, value: "" });
  setItemDocs.mockReset().mockResolvedValue({ ok: true, value: undefined });
  for (const tab of [...useTabsStore.getState().tabs]) {
    useTabsStore.getState().closeTab(tab.id);
  }
  useTabsStore.getState().openDocs(target);
  await waitFor(() => {
    expect(currentDocsTab().loaded).toBe(true);
  });
});

afterEach(() => {
  cleanup();
});

describe("DocsEditor", () => {
  it("renders an H1 in Preview for a heading typed in Edit", async () => {
    render(<DocsEditorHost />);

    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByLabelText("Documentation"), {
      target: { value: "# Overview" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Preview" }));

    const heading = await screen.findByRole("heading", { level: 1 });
    expect(heading.textContent).toBe("Overview");
  });

  it("opens in split view with both panes showing", () => {
    render(<DocsEditorHost />);

    expect(screen.getByRole("button", { name: "Split" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByLabelText("Documentation")).toBeTruthy();
  });

  it("hides the editor in preview mode and the preview in edit mode", () => {
    render(<DocsEditorHost />);

    fireEvent.click(screen.getByRole("button", { name: "Preview" }));
    expect(screen.queryByLabelText("Documentation")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    expect(screen.getByLabelText("Documentation")).toBeTruthy();
  });

  /** The buttons act on the text; what they do to it is tested in
   * lib/markdown-format.test.ts. This is the wiring. */
  it("a toolbar button changes the document", async () => {
    render(<DocsEditorHost />);

    fireEvent.click(screen.getByRole("button", { name: "Bulleted list" }));

    await waitFor(() => {
      expect(currentDocsTab().markdown).toBe("- item");
    });
  });

  it("shows a failed save rather than swallowing it", async () => {
    setItemDocs.mockResolvedValue({
      ok: false,
      error: { kind: "invalidRequest", message: "this documentation is too long" },
    });
    render(<DocsEditorHost />);

    fireEvent.change(screen.getByLabelText("Documentation"), { target: { value: "x" } });
    await useTabsStore.getState().saveDocs(currentDocsTab().id);

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("too long");
  });

  it("invites the user to write something when there is nothing yet", () => {
    render(<DocsEditorHost />);

    expect(screen.getByText(/Nothing documented yet/)).toBeTruthy();
  });

  /** An imported description is third-party text; the preview must not run
   * it. lib/markdown.test.ts covers the sanitising, this covers that the
   * component actually goes through it. */
  it("does not execute markup that arrives in the documentation", () => {
    render(<DocsEditorHost />);

    fireEvent.change(screen.getByLabelText("Documentation"), {
      target: { value: "<img src=x onerror=alert(1)>" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Preview" }));

    const image = document.querySelector("img");
    expect(image?.getAttribute("onerror") ?? null).toBeNull();
    expect(image?.getAttribute("src") ?? null).toBeNull();
  });
});
