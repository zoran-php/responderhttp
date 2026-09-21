// http_client/src/lib/pretty-print.ts
//
// Formatting a response body is a pure transformation, so it lives here and
// the viewer just renders whatever comes back.
import type { BodyLanguage } from "@/lib/content-type";

const INDENT = "  ";

/**
 * Returns the input unchanged when it cannot be parsed: a truncated or
 * malformed body is still worth reading, and silently "fixing" it would
 * misrepresent what the server actually sent.
 */
export function prettyPrint(text: string, language: BodyLanguage): string {
  switch (language) {
    case "json":
      return prettyJson(text);
    case "xml":
    case "html":
      return prettyXml(text);
    case "plaintext":
      return text;
  }
}

function prettyJson(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}

/**
 * Deliberately simple: splits on tags and indents. It is a readability aid,
 * not a parser — mixed inline text keeps its own line rather than being
 * reflowed.
 */
function prettyXml(text: string): string {
  const trimmed = text.trim();
  if (!trimmed.startsWith("<")) {
    return text;
  }
  const withBreaks = trimmed.replace(/>\s*</g, ">\n<");
  let depth = 0;
  return withBreaks
    .split("\n")
    .map((line) => {
      const isClosing = /^<\//.test(line);
      const isSelfContained = /^<[^>]+>[^<]*<\/[^>]+>$/.test(line);
      const isDeclaration = /^<[?!]/.test(line);
      if (isClosing) {
        depth = Math.max(0, depth - 1);
      }
      const indented = `${INDENT.repeat(depth)}${line}`;
      const opensChild =
        !isClosing && !isSelfContained && !isDeclaration && !/\/>$/.test(line) && /^<[^/]/.test(line);
      if (opensChild) {
        depth += 1;
      }
      return indented;
    })
    .join("\n");
}
