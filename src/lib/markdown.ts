// http_client/src/lib/markdown.ts
//
// Markdown to HTML, sanitised — the single place in this app that turns
// Markdown into anything a browser will render (PLAN.md Phase 12).
//
// **Why the sanitising is not optional here.** Documentation does not only
// come from the person at the keyboard. An OpenAPI import carries the
// descriptions written into the spec by whoever published it, and this app's
// webview holds Tauri IPC — a script that runs in it can call commands. So
// the preview pane renders third-party text into a privileged context, and
// the rendered HTML is treated as hostile regardless of where it came from.
//
// Three rules follow, and all three live in this file so there is nowhere
// else to get them wrong:
//
//   1. Everything goes through DOMPurify with an explicit allowlist. Not
//      "remove the dangerous parts" — name the safe ones and drop the rest.
//   2. Remote images do not load. `![](https://tracker/x.png)` in an imported
//      description is a beacon that fires the moment the pane renders, and the
//      Store listing promises no telemetry and no analytics. The `src` is
//      stripped and the alt text is left, so the reader sees that a picture
//      was meant to be there.
//   3. Links keep their href as inert text, never as something the browser
//      will follow. A plain anchor click inside the app replaces the whole UI
//      with a remote page and there is no way back — no chrome, no back
//      button. The preview renders them as `<span>`s carrying the URL in a
//      data attribute, and the component decides what a click does.
import DOMPurify from "dompurify";
import { marked } from "marked";

/**
 * Elements the preview may contain. Everything the spec's Markdown subset
 * produces — headers, code blocks, lists, blockquotes, tables, emphasis —
 * and nothing else. Absent by omission: script, style, iframe, object,
 * embed, form, input, and every other way a document asks for behaviour.
 */
const ALLOWED_TAGS = [
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "p",
  "br",
  "hr",
  "strong",
  "em",
  "del",
  "code",
  "pre",
  "blockquote",
  "ul",
  "ol",
  "li",
  "table",
  "thead",
  "tbody",
  "tr",
  "th",
  "td",
  "img",
  "span",
  "div",
];

/**
 * Attributes those elements may carry. No `href`, no `src`, and no event
 * handler can survive this list — `onclick` and friends are not on it, and
 * DOMPurify drops anything unnamed.
 */
const ALLOWED_ATTR = ["alt", "title", "class", "align", "data-href"];

/** Marks a link whose href was moved out of the anchor. */
export const LINK_DATA_ATTRIBUTE = "data-href";

/** The class the component hooks to style and to catch clicks. */
export const LINK_CLASS = "md-link";

/** Put on an image whose remote source was stripped. */
export const BLOCKED_IMAGE_CLASS = "md-blocked-image";

/**
 * A URL scheme safe to hand to the OS or to show as a link.
 *
 * Deliberately a small allowlist. `javascript:` is the obvious one to keep
 * out; `data:` is the less obvious one, since a `data:text/html` URL is a
 * whole document. Everything not named here is dropped.
 */
function isSafeUrl(url: string): boolean {
  const trimmed = url.trim().toLowerCase();
  return (
    trimmed.startsWith("http://") || trimmed.startsWith("https://") || trimmed.startsWith("mailto:")
  );
}

/**
 * Turns anchors into inert spans before sanitising.
 *
 * Done as a DOMPurify hook rather than afterwards on the HTML string, so it
 * runs on parsed nodes and cannot be defeated by an anchor spelled in a way a
 * regular expression would miss.
 */
function hardenNode(node: Element): void {
  if (node.tagName === "A") {
    const href = node.getAttribute("href") ?? "";
    const span = node.ownerDocument.createElement("span");
    span.className = LINK_CLASS;
    if (isSafeUrl(href)) {
      span.setAttribute(LINK_DATA_ATTRIBUTE, href.trim());
    }
    while (node.firstChild !== null) {
      span.appendChild(node.firstChild);
    }
    node.replaceWith(span);
    return;
  }

  if (node.tagName === "IMG") {
    // Every image source is dropped, not only the remote ones: there is no
    // local one to keep. Documentation has no attachments (PLAN.md Phase 12,
    // out of scope), so any src here points off the machine.
    node.removeAttribute("src");
    node.removeAttribute("srcset");
    node.setAttribute("class", BLOCKED_IMAGE_CLASS);
  }
}

let hookInstalled = false;

function installHook(): void {
  if (hookInstalled) {
    return;
  }
  DOMPurify.addHook("uponSanitizeElement", (node) => {
    if (node instanceof Element) {
      hardenNode(node);
    }
  });
  hookInstalled = true;
}

/**
 * Renders Markdown to HTML that is safe to insert.
 *
 * The return value is the only string in this app that reaches
 * `dangerouslySetInnerHTML`, and it got there through the allowlist above.
 * Anything that renders Markdown must call this rather than `marked`
 * directly — a second call site is the bug this file exists to prevent.
 */
export function renderMarkdown(markdown: string): string {
  installHook();

  // `async: false` keeps marked's return type a string rather than a promise;
  // no extension here is asynchronous.
  const html = marked.parse(markdown, { async: false, gfm: true, breaks: false });

  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS,
    ALLOWED_ATTR,
    // Belt and braces alongside the tag list: even if a tag were added above
    // by mistake, these two can never carry their contents through.
    FORBID_TAGS: ["script", "style", "iframe", "object", "embed", "form"],
    FORBID_ATTR: ["style", "srcset", "formaction", "href", "src"],
  });
}
