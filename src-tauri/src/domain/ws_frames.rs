// http_client/src-tauri/src/domain/ws_frames.rs
//
// WebSocket messages from the pieces a transport hands over (PLAN.md Phase
// 13b). Pure: no libcurl, no socket, no clock — the same shape as
// domain/cookies.rs, so the rules test without any infrastructure.
//
// libcurl's curl_ws_recv returns a message as chunks: a frame larger than the
// receive buffer comes in several, each saying how many bytes of that frame
// are still to come, and a fragmented message comes as several frames, all but
// the last flagged as continued. A message is complete when a chunk has no
// bytes left in its frame and no continuation flag (PLAN.md Phase 13a,
// finding 6). Control frames (ping, pong, close) may arrive between the
// fragments of a data message (RFC 6455 section 5.4), so they are assembled
// separately and never disturb a message in progress.

/// RFC 6455 section 7.4.1 status codes this app sends or reacts to.
pub const CLOSE_NORMAL: u16 = 1000;
pub const CLOSE_INVALID_PAYLOAD: u16 = 1007;
pub const CLOSE_MESSAGE_TOO_BIG: u16 = 1009;

/// A close frame's payload is a control payload: at most 125 bytes, two of
/// them the status code (RFC 6455 section 5.5).
const MAX_CLOSE_REASON_BYTES: usize = 123;

/// What one chunk belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    Text,
    Binary,
    Close,
    Ping,
    Pong,
}

impl FrameKind {
    pub fn is_control(self) -> bool {
        matches!(self, Self::Close | Self::Ping | Self::Pong)
    }
}

/// One piece of a frame, as a transport delivers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameChunk {
    pub kind: FrameKind,
    /// More frames of the same message follow (libcurl's `CURLWS_CONT`).
    pub more_fragments: bool,
    /// Bytes of this frame still to come after this chunk.
    pub bytes_left: u64,
    pub data: Vec<u8>,
}

/// A close frame's content. `code` is None when the payload was empty, which
/// RFC 6455 allows and means "no status code was given".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseInfo {
    pub code: Option<u16>,
    pub reason: String,
}

/// A complete message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assembled {
    Text(String),
    Binary(Vec<u8>),
    Close(CloseInfo),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
}

/// Why a message could not be accepted. Each one closes the connection with
/// the status code RFC 6455 assigns to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssemblyError {
    TooBig { limit: usize },
    InvalidUtf8,
}

impl AssemblyError {
    pub fn close_code(&self) -> u16 {
        match self {
            Self::TooBig { .. } => CLOSE_MESSAGE_TOO_BIG,
            Self::InvalidUtf8 => CLOSE_INVALID_PAYLOAD,
        }
    }

    pub fn reason(&self) -> String {
        match self {
            Self::TooBig { limit } => format!("message larger than {limit} bytes"),
            Self::InvalidUtf8 => "text message is not valid UTF-8".to_string(),
        }
    }
}

#[derive(Default)]
struct Partial {
    kind: Option<FrameKind>,
    bytes: Vec<u8>,
}

impl Partial {
    fn take(&mut self, fallback: FrameKind) -> (FrameKind, Vec<u8>) {
        let kind = self.kind.take().unwrap_or(fallback);
        (kind, std::mem::take(&mut self.bytes))
    }
}

/// Folds chunks into messages and enforces the size limit on data messages.
pub struct Reassembler {
    limit: usize,
    data: Partial,
    control: Partial,
}

impl Reassembler {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            data: Partial::default(),
            control: Partial::default(),
        }
    }

    /// `Ok(None)` while a message is still arriving.
    ///
    /// The limit is checked against the size the frame *declares*, not only
    /// what has arrived: a server announcing a gigabyte is refused on its
    /// first chunk rather than after the gigabyte has been buffered.
    pub fn push(&mut self, chunk: FrameChunk) -> Result<Option<Assembled>, AssemblyError> {
        let complete = chunk.bytes_left == 0 && !chunk.more_fragments;
        let partial = if chunk.kind.is_control() {
            &mut self.control
        } else {
            let left = usize::try_from(chunk.bytes_left).unwrap_or(usize::MAX);
            let declared = self
                .data
                .bytes
                .len()
                .saturating_add(chunk.data.len())
                .saturating_add(left);
            if declared > self.limit {
                self.data = Partial::default();
                return Err(AssemblyError::TooBig { limit: self.limit });
            }
            &mut self.data
        };
        if partial.kind.is_none() {
            partial.kind = Some(chunk.kind);
        }
        partial.bytes.extend_from_slice(&chunk.data);
        if !complete {
            return Ok(None);
        }
        let (kind, bytes) = partial.take(chunk.kind);
        finish(kind, bytes).map(Some)
    }
}

fn finish(kind: FrameKind, bytes: Vec<u8>) -> Result<Assembled, AssemblyError> {
    Ok(match kind {
        FrameKind::Text => {
            Assembled::Text(String::from_utf8(bytes).map_err(|_| AssemblyError::InvalidUtf8)?)
        }
        FrameKind::Binary => Assembled::Binary(bytes),
        FrameKind::Close => Assembled::Close(parse_close(&bytes)),
        FrameKind::Ping => Assembled::Ping(bytes),
        FrameKind::Pong => Assembled::Pong(bytes),
    })
}

/// Reads a close frame's payload. A one-byte payload is malformed; it is read
/// as "no code" rather than refused, since the connection is ending anyway.
pub fn parse_close(payload: &[u8]) -> CloseInfo {
    match payload {
        [high, low, reason @ ..] => CloseInfo {
            code: Some(u16::from_be_bytes([*high, *low])),
            reason: String::from_utf8_lossy(reason).into_owned(),
        },
        _ => CloseInfo {
            code: None,
            reason: String::new(),
        },
    }
}

/// Builds a close frame's payload. A reason too long for a control frame is
/// cut at a character boundary rather than making the frame invalid.
pub fn close_payload(code: u16, reason: &str) -> Vec<u8> {
    let mut end = reason.len().min(MAX_CLOSE_REASON_BYTES);
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    let mut payload = code.to_be_bytes().to_vec();
    payload.extend_from_slice(&reason.as_bytes()[..end]);
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: usize = 1024;

    fn chunk(kind: FrameKind, more: bool, left: u64, data: &[u8]) -> FrameChunk {
        FrameChunk {
            kind,
            more_fragments: more,
            bytes_left: left,
            data: data.to_vec(),
        }
    }

    #[test]
    fn a_single_chunk_text_frame_is_a_message() {
        let mut assembler = Reassembler::new(LIMIT);

        let message = assembler.push(chunk(FrameKind::Text, false, 0, b"hello"));

        assert_eq!(message, Ok(Some(Assembled::Text("hello".into()))));
    }

    /// Exactly what libcurl produced for three server fragments in the 13a
    /// spike: TEXT|CONT, TEXT|CONT, TEXT, each with nothing left in its frame.
    #[test]
    fn fragments_are_joined_into_one_message() {
        let mut assembler = Reassembler::new(LIMIT);

        assert_eq!(
            assembler.push(chunk(FrameKind::Text, true, 0, b"hel")),
            Ok(None)
        );
        assert_eq!(
            assembler.push(chunk(FrameKind::Text, true, 0, b"lo ")),
            Ok(None)
        );
        let message = assembler.push(chunk(FrameKind::Text, false, 0, b"world"));

        assert_eq!(message, Ok(Some(Assembled::Text("hello world".into()))));
    }

    /// A frame bigger than the receive buffer: the spike saw a 200,000-byte
    /// frame arrive in four chunks with `bytesleft` counting down.
    #[test]
    fn a_frame_split_across_chunks_is_joined_by_bytes_left() {
        let mut assembler = Reassembler::new(LIMIT);

        assert_eq!(
            assembler.push(chunk(FrameKind::Binary, false, 4, &[1, 2])),
            Ok(None)
        );
        assert_eq!(
            assembler.push(chunk(FrameKind::Binary, false, 2, &[3, 4])),
            Ok(None)
        );
        let message = assembler.push(chunk(FrameKind::Binary, false, 0, &[5, 6]));

        assert_eq!(message, Ok(Some(Assembled::Binary(vec![1, 2, 3, 4, 5, 6]))));
    }

    #[test]
    fn a_ping_between_fragments_does_not_disturb_the_message() {
        let mut assembler = Reassembler::new(LIMIT);

        assert_eq!(
            assembler.push(chunk(FrameKind::Text, true, 0, b"a")),
            Ok(None)
        );
        let ping = assembler.push(chunk(FrameKind::Ping, false, 0, b"p1"));
        let text = assembler.push(chunk(FrameKind::Text, false, 0, b"b"));

        assert_eq!(ping, Ok(Some(Assembled::Ping(b"p1".to_vec()))));
        assert_eq!(text, Ok(Some(Assembled::Text("ab".into()))));
    }

    #[test]
    fn a_message_exactly_at_the_limit_is_accepted() {
        let mut assembler = Reassembler::new(4);

        let message = assembler.push(chunk(FrameKind::Binary, false, 0, &[0; 4]));

        assert_eq!(message, Ok(Some(Assembled::Binary(vec![0; 4]))));
    }

    #[test]
    fn a_message_over_the_limit_is_refused() {
        let mut assembler = Reassembler::new(4);

        let error = assembler.push(chunk(FrameKind::Binary, false, 0, &[0; 5]));

        assert_eq!(error, Err(AssemblyError::TooBig { limit: 4 }));
    }

    /// Refused on the first chunk, before the rest of an announced frame is
    /// buffered.
    #[test]
    fn a_frame_that_declares_too_much_is_refused_before_it_arrives() {
        let mut assembler = Reassembler::new(LIMIT);

        let error = assembler.push(chunk(FrameKind::Binary, false, 1_000_000_000, &[0; 8]));

        assert_eq!(error, Err(AssemblyError::TooBig { limit: LIMIT }));
    }

    #[test]
    fn fragments_that_add_up_past_the_limit_are_refused() {
        let mut assembler = Reassembler::new(4);

        assert_eq!(
            assembler.push(chunk(FrameKind::Text, true, 0, b"abc")),
            Ok(None)
        );
        let error = assembler.push(chunk(FrameKind::Text, false, 0, b"de"));

        assert_eq!(error, Err(AssemblyError::TooBig { limit: 4 }));
    }

    #[test]
    fn text_that_is_not_utf8_is_refused() {
        let mut assembler = Reassembler::new(LIMIT);

        let error = assembler.push(chunk(FrameKind::Text, false, 0, &[0xff, 0xfe]));

        assert_eq!(error, Err(AssemblyError::InvalidUtf8));
        assert_eq!(
            AssemblyError::InvalidUtf8.close_code(),
            CLOSE_INVALID_PAYLOAD
        );
    }

    #[test]
    fn the_next_message_after_a_refusal_starts_clean() {
        let mut assembler = Reassembler::new(4);
        let _ = assembler.push(chunk(FrameKind::Binary, false, 0, &[0; 5]));

        let message = assembler.push(chunk(FrameKind::Text, false, 0, b"ok"));

        assert_eq!(message, Ok(Some(Assembled::Text("ok".into()))));
    }

    #[test]
    fn a_close_frame_carries_its_code_and_reason() {
        let mut assembler = Reassembler::new(LIMIT);

        let message = assembler.push(chunk(
            FrameKind::Close,
            false,
            0,
            &close_payload(1001, "bye"),
        ));

        assert_eq!(
            message,
            Ok(Some(Assembled::Close(CloseInfo {
                code: Some(1001),
                reason: "bye".into()
            })))
        );
    }

    #[test]
    fn an_empty_close_frame_has_no_code() {
        assert_eq!(
            parse_close(&[]),
            CloseInfo {
                code: None,
                reason: String::new()
            }
        );
        assert_eq!(parse_close(&[0x03]).code, None);
    }

    /// Control frames are capped at 125 bytes; a longer reason is cut, and
    /// never in the middle of a character.
    #[test]
    fn a_long_close_reason_is_cut_at_a_character_boundary() {
        let reason = "ђ".repeat(100);

        let payload = close_payload(CLOSE_NORMAL, &reason);

        assert!(payload.len() <= 125);
        assert!(std::str::from_utf8(&payload[2..]).is_ok());
        assert_eq!(parse_close(&payload).code, Some(CLOSE_NORMAL));
    }

    #[test]
    fn each_refusal_closes_with_its_rfc_code() {
        assert_eq!(AssemblyError::TooBig { limit: 1 }.close_code(), 1009);
        assert_eq!(AssemblyError::InvalidUtf8.close_code(), 1007);
    }
}
