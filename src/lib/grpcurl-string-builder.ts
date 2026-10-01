// http_client/src/lib/grpcurl-string-builder.ts
//
// The Code snippet for a gRPC request: the equivalent `grpcurl` command
// (PLAN-GRPC.md 16h). A display artifact only, like Copy as cURL — the app
// never runs it; calls go through libcurl in Rust (CLAUDE.md section 4).
//
// It is built from the resolved request, so a secret variable appears here
// only after substitution, as it does in the cURL snippet.
import type { GrpcRequestInput } from "@/types/grpc";
import type { Auth, KeyValue } from "@/types/http";

const LINE_CONTINUATION = " \\\n  ";

export interface GrpcurlInput {
  request: GrpcRequestInput;
  /** The message JSON as it would be sent. Empty sends `{}`. */
  message: string;
  /**
   * Import names of the schema's files, for `-proto`. Empty for a reflected
   * schema: grpcurl then asks the server itself. The names are import names,
   * not paths: no `-import-path` is emitted because the library stores the
   * sources, not the folders they came from. The command runs as-is only from
   * a folder that holds every file under its import name; files imported from
   * several folders need `-import-path` added by hand, or the `-proto` flags
   * dropped when the server supports reflection.
   */
  protoFiles: readonly string[];
}

export function buildGrpcurlCommand({ request, message, protoFiles }: GrpcurlInput): string {
  const parts: string[] = ["grpcurl"];
  const { settings, target } = request;

  if (!target.tls) {
    parts.push("-plaintext");
  } else if (!settings.verifyTls) {
    parts.push("-insecure");
  }
  for (const file of protoFiles) {
    parts.push(`-proto ${quote(file)}`);
  }
  for (const pair of [...nonEmpty(request.metadata), ...authMetadata(request.auth)]) {
    parts.push(`-H ${quote(`${pair.name.trim()}: ${pair.value.trim()}`)}`);
  }
  parts.push(`-d ${quote(message.trim() === "" ? "{}" : message)}`);

  parts.push(`-connect-timeout ${seconds(settings.connectTimeoutMs)}`);
  if (settings.deadlineMs !== null) {
    parts.push(`-max-time ${seconds(settings.deadlineMs)}`);
  }
  parts.push(`-max-msg-sz ${settings.maxReceiveBytes}`);
  if (settings.includeDefaults) {
    parts.push("-emit-defaults");
  }
  // No proxy flag: grpcurl has none, and reads https_proxy from the
  // environment instead.

  parts.push(quote(target.authority.trim()));
  parts.push(quote(request.methodPath.replace(/^\//, "")));
  return parts.join(LINE_CONTINUATION);
}

/**
 * What auth adds, as http/auth.rs builds it for the call: the same header
 * names, the same "empty means nothing" rules. grpcurl has no -u, so Basic
 * is written out as the header it becomes. An API key set to the query
 * string adds nothing: gRPC has no query string, and the call is refused.
 */
export function authMetadata(auth: Auth): KeyValue[] {
  switch (auth.kind) {
    case "none":
      return [];
    case "basic":
      return auth.username === "" && auth.password === ""
        ? []
        : [
            {
              name: "authorization",
              value: `Basic ${base64(`${auth.username}:${auth.password}`)}`,
            },
          ];
    case "bearer": {
      const token = auth.token.trim();
      return token === "" ? [] : [{ name: "authorization", value: `Bearer ${token}` }];
    }
    case "apiKey": {
      const key = auth.key.trim();
      return key === "" || auth.location === "query" ? [] : [{ name: key, value: auth.value }];
    }
    case "custom": {
      const name = auth.headerName.trim();
      return name === "" ? [] : [{ name, value: auth.headerValue }];
    }
  }
}

function nonEmpty(pairs: readonly KeyValue[]): KeyValue[] {
  return pairs.filter((pair) => pair.name.trim() !== "");
}

/** UTF-8, then base64, as Rust's encoder does it. */
function base64(text: string): string {
  let binary = "";
  for (const byte of new TextEncoder().encode(text)) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary);
}

/** grpcurl takes seconds, with a fraction. */
function seconds(ms: number): string {
  return String(Math.round(ms) / 1000);
}

/** POSIX single quotes, as in the cURL builder. */
function quote(value: string): string {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}
