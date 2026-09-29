import { describe, expect, it } from "vitest";

import { grpcTarget, splitGrpcScheme } from "@/lib/grpc-url";

function target(text: string, tls: boolean) {
  const result = grpcTarget(text, tls);
  if (!result.ok) {
    throw new Error(`expected a target, got: ${result.error}`);
  }
  return result.value;
}

function error(text: string, tls = false): string {
  const result = grpcTarget(text, tls);
  if (result.ok) {
    throw new Error(`expected an error, got ${result.value.authority}`);
  }
  return result.error;
}

describe("splitGrpcScheme", () => {
  it("takes each scheme off and says what it means for TLS", () => {
    expect(splitGrpcScheme("grpcs://api.test:443")).toEqual({ rest: "api.test:443", tls: true });
    expect(splitGrpcScheme("grpc://localhost:50051")).toEqual({
      rest: "localhost:50051",
      tls: false,
    });
    expect(splitGrpcScheme("https://api.test")).toEqual({ rest: "api.test", tls: true });
    expect(splitGrpcScheme("http://api.test")).toEqual({ rest: "api.test", tls: false });
  });

  it("ignores case and surrounding spaces, and leaves text with no scheme alone", () => {
    expect(splitGrpcScheme("  GRPCS://Api.Test:8443 ")).toEqual({
      rest: "Api.Test:8443",
      tls: true,
    });
    expect(splitGrpcScheme(" localhost:50051 ")).toEqual({ rest: "localhost:50051", tls: null });
  });
});

describe("grpcTarget", () => {
  it("keeps a typed port and follows the lock", () => {
    expect(target("localhost:50051", false)).toEqual({ authority: "localhost:50051", tls: false });
    expect(target("api.test:8443", true)).toEqual({ authority: "api.test:8443", tls: true });
  });

  it("uses 443 with TLS and 80 without when no port is typed", () => {
    expect(target("api.test", true).authority).toBe("api.test:443");
    expect(target("api.test", false).authority).toBe("api.test:80");
  });

  it("lets a scheme in the text win over the lock, default port included", () => {
    expect(target("grpcs://api.test", false)).toEqual({ authority: "api.test:443", tls: true });
    expect(target("grpc://api.test", true)).toEqual({ authority: "api.test:80", tls: false });
  });

  it("drops one trailing slash", () => {
    expect(target("grpcs://api.test:443/", false).authority).toBe("api.test:443");
  });

  it("handles IPv6 in brackets, with and without a port", () => {
    expect(target("[::1]:50051", false).authority).toBe("[::1]:50051");
    expect(target("[2001:db8::7]", true).authority).toBe("[2001:db8::7]:443");
    expect(target("[fe80::1%eth0]:9000", false).authority).toBe("[fe80::1%eth0]:9000");
  });

  it("brackets a bare IPv6 address, which cannot carry a port", () => {
    expect(target("::1", false).authority).toBe("[::1]:80");
    expect(target("2001:db8::7", true).authority).toBe("[2001:db8::7]:443");
  });

  it("refuses a path and says where the method goes instead", () => {
    expect(error("api.test:443/shop.v1.Shop/GetOrder")).toContain("method list");
  });

  it("refuses what is not host:port", () => {
    expect(error("")).toContain("host:port");
    expect(error("grpcs://")).toContain("host:port");
    expect(error("api test:443")).toContain("spaces");
    expect(error("user@api.test:443")).toContain("user name");
    expect(error(":50051")).toContain("host");
    expect(error("api_test$:1")).toContain("not a host name");
  });

  it("refuses a port that is not a number from 1 to 65535", () => {
    for (const port of ["0", "65536", "http", "", "123456"]) {
      expect(error(`api.test:${port}`)).toContain("port");
    }
  });

  it("refuses broken IPv6", () => {
    expect(error("[::1:50051")).toContain("closing ]");
    expect(error("[::1]50051")).toContain("[address]:port");
    expect(error("[nothex]:1")).toContain("not an IPv6 address");
    expect(error("1::2::3")).toContain("brackets");
  });
});
