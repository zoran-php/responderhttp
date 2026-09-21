// http_client/src/lib/markdown.test.ts
import { describe, expect, it } from "vitest";

import {
  BLOCKED_IMAGE_CLASS,
  LINK_CLASS,
  LINK_DATA_ATTRIBUTE,
  renderMarkdown,
} from "@/lib/markdown";

/**
 * Asserts on the parsed result rather than on the HTML text. `data-href`
 * contains the substring "href=", so a string search cannot tell an inert
 * data attribute from a navigable one — only the DOM can.
 */
function parse(html: string): HTMLElement {
  const host = document.createElement("div");
  host.innerHTML = html;
  return host;
}

function hasNavigableHref(html: string): boolean {
  const host = parse(html);
  return Array.from(host.querySelectorAll("*")).some((element) => element.hasAttribute("href"));
}

describe("renderMarkdown", () => {
  it("renders the constructs the editor's toolbar can produce", () => {
    expect(renderMarkdown("# Overview")).toContain("<h1");
    expect(renderMarkdown("**bold**")).toContain("<strong>bold</strong>");
    expect(renderMarkdown("*italic*")).toContain("<em>italic</em>");
    expect(renderMarkdown("- one\n- two")).toContain("<li>");
    expect(renderMarkdown("> quoted")).toContain("<blockquote>");
    expect(renderMarkdown("`inline`")).toContain("<code>inline</code>");
  });

  it("renders a fenced code block", () => {
    const html = renderMarkdown("```json\n{}\n```");

    expect(html).toContain("<pre>");
    expect(html).toContain("<code");
  });

  it("renders a GitHub-flavoured table", () => {
    const html = renderMarkdown("| a | b |\n| - | - |\n| 1 | 2 |");

    expect(html).toContain("<table>");
    expect(html).toContain("<th>");
    expect(html).toContain("<td>");
  });

  // --- The part that matters -----------------------------------------------
  //
  // An imported OpenAPI description is third-party text rendered into a
  // webview that holds Tauri IPC. Each of these is a way in.

  it("drops a script tag", () => {
    const html = renderMarkdown("before\n\n<script>alert(1)</script>\n\nafter");

    expect(html).not.toContain("<script");
    expect(html).not.toContain("alert(1)");
  });

  it("drops an inline event handler", () => {
    const html = renderMarkdown('<img src="x" onerror="alert(1)">');

    expect(html).not.toContain("onerror");
    expect(html).not.toContain("alert(1)");
  });

  it("drops an iframe", () => {
    const html = renderMarkdown('<iframe src="https://example.com"></iframe>');

    expect(html).not.toContain("<iframe");
  });

  it("keeps a javascript: link's text but never its target", () => {
    const html = renderMarkdown("[click me](javascript:alert(1))");

    expect(html).toContain("click me");
    expect(html).not.toContain("javascript:");
    expect(hasNavigableHref(html)).toBe(false);
    // No data attribute either: an unsafe scheme is not merely un-clickable,
    // it never reaches the component at all.
    expect(html).not.toContain(LINK_DATA_ATTRIBUTE);
  });

  it("refuses a data: URL, which can carry a whole document", () => {
    const html = renderMarkdown("[x](data:text/html;base64,PHNjcmlwdD4=)");

    expect(html).not.toContain("data:text/html");
  });

  it("never emits an href, so no click can navigate the app away", () => {
    const html = renderMarkdown("[docs](https://example.com/docs)");

    expect(hasNavigableHref(html)).toBe(false);
    expect(parse(html).querySelector("a")).toBeNull();
  });

  it("carries a safe link's URL as inert data for the component to handle", () => {
    const html = renderMarkdown("[docs](https://example.com/docs)");

    expect(html).toContain(LINK_CLASS);
    expect(html).toContain(`${LINK_DATA_ATTRIBUTE}="https://example.com/docs"`);
    expect(html).toContain("docs");
  });

  it("strips a remote image source so the pane cannot phone home", () => {
    const html = renderMarkdown("![a picture](https://tracker.example.com/beacon.png)");

    expect(html).not.toContain("tracker.example.com");
    expect(html).not.toContain("src=");
    expect(html).toContain(BLOCKED_IMAGE_CLASS);
    // The alt text stays, so the reader can see something was meant to be there.
    expect(html).toContain("a picture");
  });

  it("strips srcset as well as src", () => {
    const html = renderMarkdown('<img srcset="https://tracker.example.com/1x.png 1x">');

    expect(html).not.toContain("srcset");
    expect(html).not.toContain("tracker.example.com");
  });

  it("drops a style attribute, which can load a remote url of its own", () => {
    const html = renderMarkdown('<p style="background:url(https://tracker.example.com/x)">hi</p>');

    expect(html).not.toContain("style=");
    expect(html).not.toContain("tracker.example.com");
  });

  it("escapes text that looks like markup rather than rendering it", () => {
    const html = renderMarkdown("Use `<script>` carefully");

    expect(html).toContain("&lt;script&gt;");
    expect(html).not.toContain("<script>");
  });

  it("returns an empty string for empty documentation", () => {
    expect(renderMarkdown("").trim()).toBe("");
  });

  /**
   * The hook that rewrites anchors is installed once and shared. If it were
   * installed per call it would stack up, and if it were never reinstalled
   * after the first call the second render would leak an href.
   */
  it("hardens every render, not only the first", () => {
    renderMarkdown("[one](https://example.com/one)");
    const second = renderMarkdown("[two](https://example.com/two)");

    expect(hasNavigableHref(second)).toBe(false);
    expect(second).toContain(`${LINK_DATA_ATTRIBUTE}="https://example.com/two"`);
  });
});
