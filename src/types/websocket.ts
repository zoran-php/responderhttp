// http_client/src/types/websocket.ts
//
// Mirrors the WebSocket DTOs in src-tauri/src/commands/dto.rs
// (WsMessageFormatDto, WebSocketSettingsDto, WebSocketRequestDto, WsDraftDto).
// Any change there has to land here in the same commit — these two files are
// one contract, and dto.rs pins the key names in a test.
import type { KeyValue } from "@/types/http";

/**
 * Mirrors WsMessageFormat: what the Message tab's format selector offers.
 * Every format but `binary` goes out as a text frame exactly as typed.
 */
export const WS_MESSAGE_FORMATS = ["text", "json", "xml", "html", "binary"] as const;

export type WsMessageFormat = (typeof WS_MESSAGE_FORMATS)[number];

/** Mirrors WsBinaryEncoding: how a binary message is typed and shown. */
export const WS_BINARY_ENCODINGS = ["base64", "hex"] as const;

export type WsBinaryEncoding = (typeof WS_BINARY_ENCODINGS)[number];

/**
 * Mirrors WebSocketSettings. A different set from RequestSettings: redirects,
 * HTTP version and the body options mean nothing once a connection has been
 * upgraded.
 */
export interface WebSocketSettings {
  /** Off is an explicit per-request opt-in, never the default. */
  verifyTls: boolean;
  proxy: string | null;
  /** Send the jar's cookies with the handshake. */
  sendCookies: boolean;
  /** The handshake only; an open connection has no overall timeout. */
  connectTimeoutMs: number;
  /** A bigger incoming message closes the connection with 1009. */
  maxMessageBytes: number;
  /** After an unexpected drop only — never after Disconnect or a refused handshake. */
  autoReconnect: boolean;
}

/**
 * Must match `WebSocketSettings::default()` in domain/models.rs
 * (DEFAULT_WS_CONNECT_TIMEOUT, DEFAULT_WS_MAX_MESSAGE_BYTES).
 */
export const DEFAULT_WS_SETTINGS: WebSocketSettings = {
  verifyTls: true,
  proxy: null,
  sendCookies: true,
  connectTimeoutMs: 30_000,
  maxMessageBytes: 1024 * 1024,
  autoReconnect: false,
};

/** Mirrors WebSocketRequest. The query string lives in `url`. */
export interface WebSocketRequest {
  url: string;
  /** Sent with the handshake only. */
  headers: KeyValue[];
  settings: WebSocketSettings;
}

/** Mirrors WsDraft: the unsent message, saved with the request. */
export interface WsDraft {
  format: WsMessageFormat;
  /** Kept while another format is chosen, so switching back restores it. */
  binaryEncoding: WsBinaryEncoding;
  text: string;
}

export const EMPTY_WS_DRAFT: WsDraft = { format: "text", binaryEncoding: "base64", text: "" };

/**
 * Mirrors WsPayloadDto, both ways. Binary crosses as lowercase hex with no
 * separators; Rust refuses anything else, so the composer normalises what
 * the user typed before sending.
 */
export type WsPayload = { kind: "text"; text: string } | { kind: "binary"; hex: string };

/** Mirrors WsClosedByDto. */
export type WsClosedBy = "user" | "server" | "error";

/**
 * Mirrors WsEventDto: one message on a connection's channel, in the order it
 * happened. `atMs` is Unix time in milliseconds, stamped in Rust when the
 * frame was written or read. `byteLength` is the size on the wire, which for
 * text is not `text.length`.
 */
export type WsEvent =
  | { type: "connected"; atMs: number; url: string; status: number; headers: KeyValue[] }
  | { type: "sent"; atMs: number; byteLength: number; payload: WsPayload }
  | { type: "received"; atMs: number; byteLength: number; payload: WsPayload }
  | {
      type: "closed";
      atMs: number;
      /** Null when no close frame was exchanged (a drop, a failed rule). */
      code: number | null;
      reason: string;
      by: WsClosedBy;
    }
  | { type: "error"; atMs: number; message: string }
  | {
      type: "reconnecting";
      atMs: number;
      attempt: number;
      maxAttempts: number;
      delayMs: number;
    };
