// spikes/grpc-libcurl/src/frame.rs
//
// gRPC length-prefixed messages: one flag byte (compressed or not), a
// big-endian u32 length, then the message. The prototype of 16c's decoder.

pub const PREFIX_LEN: usize = 5;

pub fn encode(message: &[u8]) -> Vec<u8> {
    let length = u32::try_from(message.len()).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(PREFIX_LEN + message.len());
    out.push(0);
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(message);
    out
}

/// Accepts the body in whatever chunks the transport hands over and yields
/// each message once it is complete.
#[derive(Default)]
pub struct Decoder {
    buffer: Vec<u8>,
}

impl Decoder {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Vec<u8>> {
        self.buffer.extend_from_slice(chunk);
        let mut messages = Vec::new();
        let mut start = 0;
        while self.buffer.len() - start >= PREFIX_LEN {
            let prefix = &self.buffer[start..start + PREFIX_LEN];
            let length = u32::from_be_bytes([prefix[1], prefix[2], prefix[3], prefix[4]]) as usize;
            let end = start + PREFIX_LEN + length;
            if self.buffer.len() < end {
                break;
            }
            messages.push(self.buffer[start + PREFIX_LEN..end].to_vec());
            start = end;
        }
        self.buffer.drain(..start);
        messages
    }

    #[cfg(test)]
    pub fn pending(&self) -> usize {
        self.buffer.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_split_at_every_offset_comes_out_whole() {
        let mut wire = encode(b"hello");
        wire.extend(encode(b""));
        wire.extend(encode(b"world!"));
        for split in 0..=wire.len() {
            let mut decoder = Decoder::default();
            let mut out = decoder.push(&wire[..split]);
            out.extend(decoder.push(&wire[split..]));
            assert_eq!(
                out,
                vec![b"hello".to_vec(), Vec::new(), b"world!".to_vec()],
                "split {split}"
            );
            assert_eq!(decoder.pending(), 0);
        }
    }
}
