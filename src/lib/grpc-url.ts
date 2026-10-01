// http_client/src/lib/grpc-url.ts
//
// The gRPC URL field (PLAN-GRPC.md D6). A server is a host and a port plus
// the lock toggle, as in Letterbox; there is no path, because the method is
// picked separately. A scheme typed or pasted in front sets the toggle:
// `grpcs://` and `https://` mean TLS, `grpc://` and `http://` mean plain
// text. Pure; the store calls it and the UI shows its errors.
import type { GrpcTarget } from "@/types/grpc";
import type { Result } from "@/types/result";

/** The port used when none is typed: the scheme's own, as a browser would. */
export const DEFAULT_TLS_PORT = 443;
export const DEFAULT_PLAINTEXT_PORT = 80;

const SCHEMES: ReadonlyArray<readonly [prefix: string, tls: boolean]> = [
  ["grpcs://", true],
  ["grpc://", false],
  ["https://", true],
  ["http://", false],
];

export interface SplitScheme {
  /** The text after the scheme, trimmed. */
  rest: string;
  /** What the scheme says about TLS; null when none was typed. */
  tls: boolean | null;
}

/**
 * Takes a leading scheme off. The URL field calls this when text is pasted
 * or the field loses focus, then shows `rest` and moves the lock to `tls`,
 * so the field never keeps a scheme that disagrees with the lock.
 */
export function splitGrpcScheme(text: string): SplitScheme {
  const trimmed = text.trim();
  const lower = trimmed.toLowerCase();
  for (const [prefix, tls] of SCHEMES) {
    if (lower.startsWith(prefix)) {
      return { rest: trimmed.slice(prefix.length), tls };
    }
  }
  return { rest: trimmed, tls: null };
}

/**
 * The target a call goes to, from the URL field (already substituted) and
 * the lock. A scheme in the text wins over the lock. A trailing `/` is
 * dropped; anything after it is refused, since a gRPC target has no path.
 *
 * IPv6 addresses go in brackets, as in any URL: `[::1]:50051`. A bare
 * address with no brackets and no port (`::1`) is accepted and bracketed.
 */
export function grpcTarget(text: string, lockTls: boolean): Result<GrpcTarget, string> {
  const { rest, tls: schemeTls } = splitGrpcScheme(text);
  const tls = schemeTls ?? lockTls;
  const hostPort = rest.endsWith("/") ? rest.slice(0, -1) : rest;

  if (hostPort === "") {
    return fail("Enter the server as host:port.");
  }
  if (/\s/.test(hostPort)) {
    return fail("The server address cannot contain spaces.");
  }
  if (hostPort.includes("/")) {
    return fail(
      "A gRPC server has no path. Enter host:port and pick the method from the method list.",
    );
  }
  if (/[?#@]/.test(hostPort)) {
    return fail("Enter the server as host:port, without a query, fragment or user name.");
  }

  const split = splitHostPort(hostPort);
  if (!split.ok) {
    return split;
  }
  const { host, port: typedPort } = split.value;
  let port = tls ? DEFAULT_TLS_PORT : DEFAULT_PLAINTEXT_PORT;
  if (typedPort !== null) {
    if (!/^\d{1,5}$/.test(typedPort) || Number(typedPort) < 1 || Number(typedPort) > 65535) {
      return fail(`"${typedPort}" is not a port. Use a number from 1 to 65535.`);
    }
    port = Number(typedPort);
  }
  return { ok: true, value: { authority: `${host}:${port}`, tls } };
}

function splitHostPort(text: string): Result<{ host: string; port: string | null }, string> {
  if (text.startsWith("[")) {
    const close = text.indexOf("]");
    if (close < 0) {
      return fail("An IPv6 address needs its closing ].");
    }
    const address = text.slice(1, close);
    if (!isIpv6(address)) {
      return fail(`"${address}" is not an IPv6 address.`);
    }
    const after = text.slice(close + 1);
    if (after === "") {
      return { ok: true, value: { host: `[${address}]`, port: null } };
    }
    if (!after.startsWith(":")) {
      return fail("Put the port after the IPv6 address as [address]:port.");
    }
    return { ok: true, value: { host: `[${address}]`, port: after.slice(1) } };
  }

  const colons = text.split(":").length - 1;
  if (colons > 1) {
    // More than one colon and no brackets: a bare IPv6 address, with no
    // way to tell a port from its last group.
    return isIpv6(text)
      ? { ok: true, value: { host: `[${text}]`, port: null } }
      : fail("Put an IPv6 address in brackets: [address]:port.");
  }
  const [host = "", port] = text.split(":");
  if (host === "") {
    return fail("Enter the host before the port.");
  }
  if (!/^[A-Za-z0-9._-]+$/.test(host)) {
    return fail(`"${host}" is not a host name.`);
  }
  return { ok: true, value: { host, port: port ?? null } };
}

/** Loose on purpose: hex groups and colons, one `::` at most, and an
 * optional zone. libcurl gives the precise verdict when it connects. */
function isIpv6(text: string): boolean {
  const [address = "", zone] = text.split("%");
  if (zone !== undefined && !/^[A-Za-z0-9._-]+$/.test(zone)) {
    return false;
  }
  if (!/^[0-9A-Fa-f:.]+$/.test(address) || !address.includes(":")) {
    return false;
  }
  return address.split("::").length <= 2;
}

function fail<T>(message: string): Result<T, string> {
  return { ok: false, error: message };
}
