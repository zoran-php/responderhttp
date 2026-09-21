// http_client/src/lib/query-sync.test.ts
import { describe, expect, it } from "vitest";

import {
  encodeUrlForSend,
  foldLegacyParams,
  queryPairs,
  splitUrl,
  withQueryPairs,
} from "@/lib/query-sync";

describe("splitUrl", () => {
  it("separates base, query and fragment", () => {
    expect(splitUrl("https://x.test/a?b=1#c")).toEqual({
      base: "https://x.test/a",
      query: "b=1",
      fragment: "#c",
    });
  });

  it("tells a missing query from an empty one", () => {
    expect(splitUrl("https://x.test/a").query).toBeNull();
    expect(splitUrl("https://x.test/a?").query).toBe("");
  });

  it("treats a ? after the fragment as part of the fragment", () => {
    expect(splitUrl("https://x.test/a#b?c=1")).toEqual({
      base: "https://x.test/a",
      query: null,
      fragment: "#b?c=1",
    });
  });
});

describe("queryPairs", () => {
  it("reads pairs as typed, without decoding", () => {
    expect(queryPairs("https://x.test/?q=a%20b&name={{name}}&n=1+1")).toEqual([
      { name: "q", value: "a%20b" },
      { name: "name", value: "{{name}}" },
      { name: "n", value: "1+1" },
    ]);
  });

  it("splits on the first = only and reads a bare flag as an empty value", () => {
    expect(queryPairs("https://x.test/?expr=a=b&verbose")).toEqual([
      { name: "expr", value: "a=b" },
      { name: "verbose", value: "" },
    ]);
  });

  it("skips empty segments but keeps a pair with only a value", () => {
    expect(queryPairs("https://x.test/?&a=1&&=x&")).toEqual([
      { name: "a", value: "1" },
      { name: "", value: "x" },
    ]);
  });

  it("has nothing to say about a URL without a query", () => {
    expect(queryPairs("https://x.test/")).toEqual([]);
    expect(queryPairs("https://x.test/?")).toEqual([]);
    expect(queryPairs("")).toEqual([]);
  });

  it("works while the URL is still being typed", () => {
    expect(queryPairs("?a")).toEqual([{ name: "a", value: "" }]);
    expect(queryPairs("{{base_url}}/users?page=")).toEqual([{ name: "page", value: "" }]);
  });
});

describe("withQueryPairs", () => {
  it("replaces the query and keeps base and fragment", () => {
    expect(
      withQueryPairs("https://x.test/a?old=1#top", [
        { name: "q", value: "cats" },
        { name: "page", value: "2" },
      ]),
    ).toBe("https://x.test/a?q=cats&page=2#top");
  });

  it("adds a query to a URL that had none", () => {
    expect(withQueryPairs("https://x.test/a", [{ name: "q", value: "1" }])).toBe(
      "https://x.test/a?q=1",
    );
  });

  it("drops the ? when the last pair goes", () => {
    expect(withQueryPairs("https://x.test/a?q=1", [])).toBe("https://x.test/a");
    expect(withQueryPairs("https://x.test/a?q=1", [{ name: "", value: "" }])).toBe(
      "https://x.test/a",
    );
  });

  it("writes text as typed, including spaces and variables", () => {
    expect(
      withQueryPairs("{{base_url}}/s", [
        { name: "q", value: "a b" },
        { name: "token", value: "{{token}}" },
      ]),
    ).toBe("{{base_url}}/s?q=a b&token={{token}}");
  });

  it("escapes only what would change the query's structure", () => {
    expect(
      withQueryPairs("https://x.test/", [
        { name: "a=b&c#d", value: "x=y&z#w" },
        { name: "plus", value: "1+1%20" },
      ]),
    ).toBe("https://x.test/?a%3Db%26c%23d=x=y%26z%23w&plus=1+1%20");
  });

  it("writes a name with no value bare", () => {
    expect(withQueryPairs("https://x.test/", [{ name: "verbose", value: "" }])).toBe(
      "https://x.test/?verbose",
    );
  });

  it("round-trips with queryPairs for anything the table can hold", () => {
    const pairs = [
      { name: "q", value: "a b" },
      { name: "a&b", value: "c=d&e#f" },
      { name: "", value: "orphan" },
      { name: "flag", value: "" },
      { name: "u", value: "Köln%2B+" },
    ];

    const url = withQueryPairs("https://x.test/p#frag", pairs);

    expect(queryPairs(url)).toEqual([
      { name: "q", value: "a b" },
      { name: "a%26b", value: "c=d%26e%23f" },
      { name: "", value: "orphan" },
      { name: "flag", value: "" },
      { name: "u", value: "Köln%2B+" },
    ]);
    expect(withQueryPairs(url, queryPairs(url))).toBe(url);
  });
});

describe("foldLegacyParams", () => {
  it("appends old Params rows after the URL's own query, encoded as they were sent", () => {
    expect(
      foldLegacyParams(
        "https://x.test/s?page=2",
        [
          { name: "q", value: "a b&c" },
          { name: "  ", value: "ignored" },
          { name: "n", value: "1+1" },
        ],
        true,
      ),
    ).toBe("https://x.test/s?page=2&q=a%20b%26c&n=1%2B1");
  });

  it("keeps rows raw when the request had encoding off, escaping only structure", () => {
    expect(foldLegacyParams("https://x.test/s", [{ name: "q", value: "a b&c" }], false)).toBe(
      "https://x.test/s?q=a b%26c",
    );
  });

  it("leaves the URL alone when there is nothing to fold", () => {
    expect(foldLegacyParams("https://x.test/s?x=1#f", [], true)).toBe("https://x.test/s?x=1#f");
  });
});

describe("encodeUrlForSend", () => {
  it("leaves a valid URL byte-for-byte alone", () => {
    for (const url of [
      "https://example.com/users/42?q=a%20b&tags=x,y&sum=1+2#top",
      "https://user:p%40ss@example.com:8443/a;b/c?x=&y=*!$'()@:/?",
      "http://[::1]:8080/",
      "https://example.com",
    ]) {
      expect(encodeUrlForSend(url)).toBe(url);
    }
  });

  it("encodes what a URL cannot carry, as the backend does", () => {
    expect(encodeUrlForSend('https://example.com/a b/ö?q=a b&c="x"&d=<{1}>|^`\\')).toBe(
      "https://example.com/a%20b/%C3%B6?q=a%20b&c=%22x%22&d=%3C%7B1%7D%3E%7C%5E%60%5C",
    );
  });

  it("encodes a stray % and keeps a real escape", () => {
    expect(encodeUrlForSend("https://example.com/?a=100%&b=%41&c=%4&d=%zz&e=%")).toBe(
      "https://example.com/?a=100%25&b=%41&c=%254&d=%25zz&e=%25",
    );
  });

  it("leaves the host and anything without a scheme alone", () => {
    expect(encodeUrlForSend("https://bücher.example/ä")).toBe("https://bücher.example/%C3%A4");
    expect(encodeUrlForSend("{{base}}/a b")).toBe("{{base}}/a b");
  });
});
