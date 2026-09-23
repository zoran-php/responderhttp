// http_client/src-tauri/src/domain/sse.rs
//
// The server-sent events parser (PLAN-SSE.md, 14a). Pure: bytes in, blocks
// out, no clock and no I/O, so every rule here is tested without a server.
//
// The rules are the WHATWG HTML "server-sent events" section, with two
// deliberate differences, both because this is a viewer rather than an
// `EventSource`:
//
//  1. A block with fields but no `data` is still reported. The spec
//     dispatches nothing for it; hiding it would lose an `id:` or `retry:`
//     the server actually sent, which is exactly what someone debugging a
//     stream wants to see.
//  2. A last block that arrives without its closing blank line is reported
//     when the stream ends. The spec discards it; a stream cut off mid-event
//     is worth showing, and the Raw view would show it anyway.
//
// Everything a server sends ends up in some block's `raw`, terminators
// included, so the Raw view is the stream as it arrived and not a
// reconstruction, and so the byte total the viewer shows adds up. The blank
// line that ends a block belongs to that block. A blank line that ends
// nothing — the gap after a comment, or a run of them between blocks —
// cannot be given to a block that has already been reported, so it is
// carried onto the front of the next one. The single exception is a stream
// whose very last line is such a blank: there is no next block to carry it
// to, and those one or two bytes go unattributed.

/// What `event:` means when a block does not say (the spec's default).
pub const DEFAULT_EVENT_NAME: &str = "message";

const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseBlockKind {
    Event {
        /// `event:`, or `message` when the block did not say.
        name: String,
        /// Every `data:` line of the block, joined with `\n`.
        data: String,
        /// `id:` as this block gave it. Not inherited from an earlier block:
        /// the list shows what the server sent, where it sent it.
        id: Option<String>,
        /// `retry:` in milliseconds. Shown, not acted on — reconnecting is
        /// not this phase (PLAN-SSE.md section 8).
        retry: Option<u64>,
    },
    /// A line starting with `:`. Servers send these to keep a connection
    /// alive; the spec ignores them and the log shows them quietly.
    Comment { text: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseBlock {
    pub kind: SseBlockKind,
    /// The block's own lines, exactly as they arrived, terminators included.
    pub raw: String,
}

impl SseBlock {
    /// The event name a block shows, including for a comment row.
    pub fn name(&self) -> &str {
        match &self.kind {
            SseBlockKind::Event { name, .. } => name,
            SseBlockKind::Comment { .. } => "comment",
        }
    }
}

/// Feed it whatever libcurl hands over; it keeps whatever is incomplete.
#[derive(Debug, Default)]
pub struct SseParser {
    /// Bytes not yet formed into a complete line.
    pending: Vec<u8>,
    /// False until the byte-order mark has been dealt with, which is once.
    started: bool,
    block: Block,
    /// Blank lines that closed no block yet, waiting for the block they
    /// will be reported with. See the note at the top of the file.
    carry: String,
    /// The last `id:` seen, which a reconnect would send back as
    /// `Last-Event-ID`. Kept for that day; nothing reads it yet.
    last_event_id: Option<String>,
}

/// The block being built: the fields seen since the last blank line.
#[derive(Debug, Default)]
struct Block {
    name: Option<String>,
    data: String,
    /// True once a `data:` line has been seen, so an empty `data:` is not
    /// mistaken for no data at all.
    has_data: bool,
    id: Option<String>,
    retry: Option<u64>,
    /// Any field line at all, including ones this app ignores.
    has_field: bool,
    raw: String,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// What a reconnect would send as `Last-Event-ID`.
    pub fn last_event_id(&self) -> Option<&str> {
        self.last_event_id.as_deref()
    }

    /// Every block completed by these bytes, in order. A line split across
    /// two calls, `\r\n` split between them included, is held until it is
    /// whole.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<SseBlock> {
        self.pending.extend_from_slice(bytes);
        if !self.started {
            if self.pending.starts_with(&BOM) {
                self.pending.drain(..BOM.len());
                self.started = true;
            } else if self.pending.len() >= BOM.len() || !BOM.starts_with(&self.pending) {
                // Enough bytes to know it is not a byte-order mark.
                self.started = true;
            } else {
                // Could still become one; wait for the rest.
                return Vec::new();
            }
        }

        let mut blocks = Vec::new();
        while let Some(line) = self.take_line() {
            self.read_line(&line.text, &line.terminator, &mut blocks);
        }
        blocks
    }

    /// The stream ended. Reports a block the server never closed with a
    /// blank line, and the partial last line with it.
    pub fn finish(&mut self) -> Vec<SseBlock> {
        let mut blocks = Vec::new();
        if !self.pending.is_empty() {
            let text = String::from_utf8_lossy(&self.pending).into_owned();
            self.pending.clear();
            self.read_line(&text, "", &mut blocks);
        }
        self.dispatch("", &mut blocks);
        self.started = true;
        blocks
    }

    fn take_line(&mut self) -> Option<Line> {
        let at = self
            .pending
            .iter()
            .position(|byte| *byte == b'\n' || *byte == b'\r')?;
        let carriage_return = self.pending[at] == b'\r';
        if carriage_return && at + 1 == self.pending.len() {
            // A trailing `\r` may still become `\r\n`, and the two must not
            // count as two line endings.
            return None;
        }
        let terminator_len = if carriage_return && self.pending[at + 1] == b'\n' {
            2
        } else {
            1
        };
        let text = String::from_utf8_lossy(&self.pending[..at]).into_owned();
        let terminator =
            String::from_utf8_lossy(&self.pending[at..at + terminator_len]).into_owned();
        self.pending.drain(..at + terminator_len);
        Some(Line { text, terminator })
    }

    fn read_line(&mut self, text: &str, terminator: &str, blocks: &mut Vec<SseBlock>) {
        if text.is_empty() {
            self.dispatch(terminator, blocks);
            return;
        }
        if let Some(comment) = text.strip_prefix(':') {
            // Its own block, in the order it arrived, so a keep-alive never
            // disappears and never joins the event around it.
            let carried = std::mem::take(&mut self.carry);
            blocks.push(SseBlock {
                kind: SseBlockKind::Comment {
                    text: comment.strip_prefix(' ').unwrap_or(comment).to_string(),
                },
                raw: format!("{carried}{text}{terminator}"),
            });
            return;
        }

        if !self.block.has_field {
            self.block.raw.push_str(&std::mem::take(&mut self.carry));
        }
        self.block.has_field = true;
        self.block.raw.push_str(text);
        self.block.raw.push_str(terminator);

        let (name, value) = match text.split_once(':') {
            // "A single leading space is removed", and only one.
            Some((name, value)) => (name, value.strip_prefix(' ').unwrap_or(value)),
            // A line with no colon is a field with an empty value.
            None => (text, ""),
        };
        match name {
            "event" => self.block.name = Some(value.to_string()),
            "data" => {
                if self.block.has_data {
                    self.block.data.push('\n');
                }
                self.block.data.push_str(value);
                self.block.has_data = true;
            }
            // A NUL in an id makes the whole field ignored, per the spec.
            "id" if !value.contains('\0') => self.block.id = Some(value.to_string()),
            // Digits only: anything else is ignored rather than guessed at.
            "retry" if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
                self.block.retry = value.parse().ok();
            }
            // Any other field name is kept in `raw` and otherwise ignored.
            _ => {}
        }
    }

    fn dispatch(&mut self, terminator: &str, blocks: &mut Vec<SseBlock>) {
        let block = std::mem::take(&mut self.block);
        if !block.has_field {
            // A blank line with nothing before it: the gap between blocks,
            // or the one that follows a comment. Its bytes wait for the next
            // block rather than being dropped.
            self.carry.push_str(terminator);
            return;
        }
        if let Some(id) = &block.id {
            self.last_event_id = Some(id.clone());
        }
        blocks.push(SseBlock {
            kind: SseBlockKind::Event {
                name: block.name.unwrap_or_else(|| DEFAULT_EVENT_NAME.to_string()),
                data: block.data,
                id: block.id,
                retry: block.retry,
            },
            raw: block.raw + terminator,
        });
    }
}

struct Line {
    text: String,
    terminator: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(block: &SseBlock) -> (&str, &str, Option<&str>, Option<u64>) {
        match &block.kind {
            SseBlockKind::Event {
                name,
                data,
                id,
                retry,
            } => (name, data, id.as_deref(), *retry),
            other => panic!("expected an event, got {other:?}"),
        }
    }

    fn blocks(stream: &str) -> Vec<SseBlock> {
        let mut parser = SseParser::new();
        let mut all = parser.push(stream.as_bytes());
        all.extend(parser.finish());
        all
    }

    /// The stream in the feature request: one `data:` line per word, ending
    /// with a named event.
    #[test]
    fn the_reference_stream_reads_as_one_event_per_word() {
        let stream = "data: A\n\ndata: group\n\ndata: of\n\nevent: end\ndata: Stream ended\n\n";

        let parsed = blocks(stream);

        assert_eq!(parsed.len(), 4);
        assert_eq!(event(&parsed[0]), ("message", "A", None, None));
        assert_eq!(event(&parsed[1]), ("message", "group", None, None));
        assert_eq!(event(&parsed[3]), ("end", "Stream ended", None, None));
        assert_eq!(parsed[3].raw, "event: end\ndata: Stream ended\n\n");
    }

    /// The invariant the Raw view and the size total both rest on. It was
    /// not true once: the blank line that ends a block was dropped, so a
    /// stream's raw came back one byte per block short of what arrived.
    #[test]
    fn concatenating_every_raw_gives_the_stream_back() {
        let stream = ": keep-alive\n\ndata: A\n\n\nevent: end\nid: 7\ndata: done\n\n";

        let joined: String = blocks(stream)
            .iter()
            .map(|block| block.raw.as_str())
            .collect();

        assert_eq!(joined, stream);
    }

    #[test]
    fn data_lines_of_one_block_join_with_a_newline() {
        let parsed = blocks("data: first\ndata: second\ndata:\n\n");

        assert_eq!(event(&parsed[0]).1, "first\nsecond\n");
    }

    #[test]
    fn a_field_carries_its_id_and_retry_and_only_one_leading_space_is_dropped() {
        let parsed = blocks("id: 42\nretry: 3000\nevent: tick\ndata:  padded\n\n");

        assert_eq!(
            event(&parsed[0]),
            ("tick", " padded", Some("42"), Some(3000))
        );
    }

    /// The spec's rules for values this app would otherwise have to guess at.
    #[test]
    fn a_retry_that_is_not_a_number_and_an_id_with_a_nul_are_ignored() {
        let parsed = blocks("retry: soon\nid: a\0b\ndata: x\n\n");

        assert_eq!(event(&parsed[0]), ("message", "x", None, None));
    }

    #[test]
    fn a_line_without_a_colon_is_a_field_with_an_empty_value() {
        let parsed = blocks("data\n\n");

        assert_eq!(event(&parsed[0]).1, "");
        assert_eq!(parsed[0].raw, "data\n\n");
    }

    #[test]
    fn a_comment_is_its_own_quiet_block_in_the_order_it_arrived() {
        let parsed = blocks(": keep-alive\ndata: x\n\n");

        assert_eq!(
            parsed[0].kind,
            SseBlockKind::Comment {
                text: "keep-alive".into()
            }
        );
        assert_eq!(parsed[0].name(), "comment");
        assert_eq!(parsed[0].raw, ": keep-alive\n");
        assert_eq!(event(&parsed[1]).1, "x");
    }

    /// Deliberate difference from the spec: a block without `data` is still
    /// reported, because the server sent it.
    #[test]
    fn a_block_with_only_an_id_is_still_reported() {
        let parsed = blocks("id: 7\n\n");

        assert_eq!(event(&parsed[0]), ("message", "", Some("7"), None));
    }

    #[test]
    fn blank_lines_between_blocks_report_nothing() {
        assert_eq!(blocks("\n\n\n"), Vec::new());
    }

    #[test]
    fn every_line_ending_ends_a_line_and_crlf_counts_once() {
        let parsed = blocks("data: a\r\n\r\ndata: b\r\rdata: c\n\n");

        assert_eq!(parsed.len(), 3);
        assert_eq!(event(&parsed[0]), ("message", "a", None, None));
        assert_eq!(parsed[0].raw, "data: a\r\n\r\n");
        assert_eq!(event(&parsed[1]).1, "b");
        assert_eq!(event(&parsed[2]).1, "c");
    }

    /// The case a chunked transfer makes certain: libcurl hands over
    /// whatever arrived, which is rarely a whole line.
    #[test]
    fn a_line_split_across_chunks_is_held_until_it_is_whole() {
        let mut parser = SseParser::new();

        assert!(parser.push(b"data: hel").is_empty());
        assert!(parser.push(b"lo\n").is_empty());
        let parsed = parser.push(b"\n");

        assert_eq!(event(&parsed[0]), ("message", "hello", None, None));
    }

    #[test]
    fn a_crlf_split_between_two_chunks_is_one_line_ending() {
        let mut parser = SseParser::new();

        assert!(parser.push(b"data: x\r").is_empty());
        let parsed = parser.push(b"\n\r\n");

        assert_eq!(parsed.len(), 1);
        assert_eq!(event(&parsed[0]).1, "x");
        assert_eq!(parsed[0].raw, "data: x\r\n\r\n");
    }

    #[test]
    fn a_multi_byte_character_split_across_chunks_survives() {
        let mut parser = SseParser::new();
        let text = "data: Ђорђe\n\n".as_bytes();
        let (head, tail) = text.split_at(9);

        assert!(parser.push(head).is_empty());
        let parsed = parser.push(tail);

        assert_eq!(event(&parsed[0]).1, "Ђорђe");
    }

    #[test]
    fn a_byte_order_mark_is_dropped_once_even_when_it_arrives_alone() {
        let mut parser = SseParser::new();

        assert!(parser.push(&BOM[..2]).is_empty());
        let parsed = parser.push(&[
            BOM[2], b'd', b'a', b't', b'a', b':', b' ', b'x', b'\n', b'\n',
        ]);

        assert_eq!(event(&parsed[0]).1, "x");
        assert_eq!(parsed[0].raw, "data: x\n\n");
    }

    /// Deliberate difference from the spec: a stream cut off mid-event still
    /// shows what arrived.
    #[test]
    fn a_stream_that_ends_without_a_blank_line_still_reports_its_last_block() {
        let mut parser = SseParser::new();
        assert!(parser.push(b"data: half").is_empty());

        let parsed = parser.finish();

        assert_eq!(event(&parsed[0]), ("message", "half", None, None));
        assert!(parser.finish().is_empty());
    }

    #[test]
    fn the_last_id_is_remembered_for_a_reconnect_that_does_not_exist_yet() {
        let mut parser = SseParser::new();

        parser.push(b"id: 1\ndata: a\n\ndata: b\n\n");

        assert_eq!(parser.last_event_id(), Some("1"));
    }

    /// A stream is read one chunk at a time for as long as it runs, so the
    /// cost of parsing must not grow with what has already been read.
    #[test]
    fn a_long_stream_parses_in_step_with_its_length() {
        let mut parser = SseParser::new();
        let mut count = 0;

        for index in 0..20_000 {
            count += parser.push(format!("data: {index}\n\n").as_bytes()).len();
        }

        assert_eq!(count, 20_000);
        assert!(parser.pending.is_empty());
    }
}
