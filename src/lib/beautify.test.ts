import { describe, expect, it } from "vitest";

import { beautify, isBeautifiable } from "@/lib/beautify";

describe("isBeautifiable", () => {
  it("offers Beautify for JSON, XML and HTML only", () => {
    expect(
      ["text", "json", "xml", "html", "binary"].map((f) => isBeautifiable(f as never)),
    ).toEqual([false, true, true, true, false]);
  });
});

describe("beautify JSON", () => {
  it("indents by two spaces", () => {
    expect(beautify("json", '{"a":[1,2],"b":{"c":null}}')).toEqual({
      ok: true,
      text: '{\n  "a": [\n    1,\n    2\n  ],\n  "b": {\n    "c": null\n  }\n}',
    });
  });

  it("leaves broken JSON alone and says why", () => {
    const result = beautify("json", '{"a":');

    expect(result.ok).toBe(false);
    expect(result.ok ? "" : result.reason).toMatch(/^Not valid JSON: /);
  });
});

describe("beautify XML", () => {
  it("puts one element per line, keeping short text inline", () => {
    const xml = '<?xml version="1.0"?><order id="7"><item qty="2">Tea</item><note/></order>';

    expect(beautify("xml", xml)).toEqual({
      ok: true,
      text: [
        '<?xml version="1.0"?>',
        '<order id="7">',
        '  <item qty="2">Tea</item>',
        "  <note/>",
        "</order>",
      ].join("\n"),
    });
  });

  it("keeps a > inside a quoted attribute as part of the tag", () => {
    expect(beautify("xml", '<a title="x>y"><b/></a>')).toEqual({
      ok: true,
      text: '<a title="x>y">\n  <b/>\n</a>',
    });
  });

  it("keeps comments and CDATA whole", () => {
    expect(beautify("xml", "<a><!-- <b> --><![CDATA[<c>]]></a>")).toEqual({
      ok: true,
      text: "<a>\n  <!-- <b> -->\n  <![CDATA[<c>]]>\n</a>",
    });
  });

  it("names the problem when the document does not balance", () => {
    const reason = (text: string) => {
      const result = beautify("xml", text);
      return result.ok ? null : result.reason;
    };

    expect(reason("<a><b></a>")).toBe("Not valid XML: </a> closes <b>");
    expect(reason("<a>")).toBe("Not valid XML: <a> is never closed");
    expect(reason("</a>")).toBe("Not valid XML: </a> has no opening tag");
    expect(reason("<a></a>tail")).toBe("Not valid XML: Text outside the root element");
    expect(reason("<a")).toMatch(/is never closed with ">"/);
    expect(reason("plain text")).toBe("Not XML: there is no element to format");
  });
});

describe("beautify HTML", () => {
  it("knows void elements, and forgives what browsers forgive", () => {
    const html = "<div><p>One<br>two<img src=x></div><p>left open";

    expect(beautify("html", html)).toEqual({
      ok: true,
      text: [
        "<div>",
        "  <p>",
        "    One",
        "    <br>",
        "    two",
        "    <img src=x>",
        "</div>",
        "<p>",
        "  left open",
      ].join("\n"),
    });
  });

  it("does not read markup inside a script", () => {
    const result = beautify("html", "<script>if (a < b) { x(); }</script>");

    expect(result).toEqual({ ok: true, text: "<script>if (a < b) { x(); }</script>" });
  });

  it("refuses text with no element in it", () => {
    expect(beautify("html", "just words")).toEqual({
      ok: false,
      reason: "Not HTML: there is no element to format",
    });
  });
});

it("leaves an empty composer as it is", () => {
  expect(beautify("xml", "  ")).toEqual({ ok: true, text: "  " });
});
