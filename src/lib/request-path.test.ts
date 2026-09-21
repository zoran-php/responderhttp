// http_client/src/lib/request-path.test.ts
import { describe, expect, it } from "vitest";

import { requestPathSegments } from "@/lib/request-path";
import type { Folder } from "@/types/collections";

function folder(id: string, name: string, parentFolderId: string | null): Folder {
  return { id, name, parentFolderId, collectionId: "col_1" };
}

const FOLDERS: Folder[] = [
  folder("fld_auth", "auth", null),
  folder("fld_admin", "admin", "fld_auth"),
  folder("fld_other", "unrelated", null),
];

describe("requestPathSegments", () => {
  it("reads outside in, ending with the request", () => {
    expect(
      requestPathSegments({
        collectionName: "lockoncam",
        folders: FOLDERS,
        folderId: "fld_admin",
        requestName: "login",
      }),
    ).toEqual(["lockoncam", "auth", "admin", "login"]);
  });

  it("is collection and request only at the collection root", () => {
    expect(
      requestPathSegments({
        collectionName: "lockoncam",
        folders: FOLDERS,
        folderId: null,
        requestName: "root",
      }),
    ).toEqual(["lockoncam", "root"]);
  });

  /** A tab that has never been saved has nowhere to be. */
  it("is just the name when the request belongs to no collection", () => {
    expect(
      requestPathSegments({
        collectionName: null,
        folders: [],
        folderId: null,
        requestName: "Untitled Request",
      }),
    ).toEqual(["Untitled Request"]);
  });

  /** Contents can arrive before or after the request that references them. */
  it("skips a folder id the collection does not contain", () => {
    expect(
      requestPathSegments({
        collectionName: "lockoncam",
        folders: FOLDERS,
        folderId: "fld_missing",
        requestName: "login",
      }),
    ).toEqual(["lockoncam", "login"]);
  });

  /**
   * Nothing should ever write a parent chain that loops, but an infinite loop
   * while rendering a label is a hung window, so the walk refuses to revisit.
   */
  it("terminates on a folder chain that points at itself", () => {
    const looped: Folder[] = [
      folder("fld_a", "a", "fld_b"),
      folder("fld_b", "b", "fld_a"),
    ];

    expect(
      requestPathSegments({
        collectionName: "c",
        folders: looped,
        folderId: "fld_a",
        requestName: "r",
      }),
    ).toEqual(["c", "b", "a", "r"]);
  });

  it("always ends with the request name, so the last segment is never empty", () => {
    const segments = requestPathSegments({
      collectionName: "lockoncam",
      folders: FOLDERS,
      folderId: "fld_auth",
      requestName: "register",
    });

    expect(segments[segments.length - 1]).toBe("register");
  });
});
