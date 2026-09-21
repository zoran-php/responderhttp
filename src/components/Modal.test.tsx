// http_client/src/components/Modal.test.tsx
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Modal } from "@/components/Modal";

afterEach(() => {
  cleanup();
});

function bodyOf(dialog: HTMLElement): HTMLElement {
  const body = dialog.querySelector<HTMLElement>("[data-modal-body]");
  if (!body) {
    throw new Error("no modal body");
  }
  return body;
}

describe("Modal", () => {
  it("never grows past the window and scrolls only its body", () => {
    render(
      <Modal onClose={vi.fn()} title="Tall">
        <p>content</p>
      </Modal>,
    );

    const dialog = screen.getByRole("dialog", { name: "Tall" });
    expect(dialog.className).toContain("max-h-full");
    expect(bodyOf(dialog).className).toContain("overflow-y-auto");
    expect(bodyOf(dialog).textContent).toBe("content");
  });

  it("keeps the footer outside the scrolling body", () => {
    render(
      <Modal footer={<button type="button">Import</button>} onClose={vi.fn()} title="Import">
        <p>report</p>
      </Modal>,
    );

    const dialog = screen.getByRole("dialog", { name: "Import" });
    const button = screen.getByRole("button", { name: "Import" });
    expect(bodyOf(dialog).contains(button)).toBe(false);
    expect(button.closest("[data-modal-footer]")).not.toBeNull();
  });

  it("renders no footer row when there is no footer", () => {
    render(
      <Modal onClose={vi.fn()} title="Plain">
        <p>content</p>
      </Modal>,
    );

    expect(document.querySelector("[data-modal-footer]")).toBeNull();
  });

  it("closes on Escape and on a click outside, not on a click inside", () => {
    const onClose = vi.fn();
    render(
      <Modal onClose={onClose} title="Close me">
        <p>inside</p>
      </Modal>,
    );

    fireEvent.click(screen.getByText("inside"));
    expect(onClose).not.toHaveBeenCalled();

    fireEvent.keyDown(document, { key: "Escape" });
    fireEvent.click(screen.getByRole("dialog").parentElement as HTMLElement);
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
