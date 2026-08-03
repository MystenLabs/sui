// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    borrow::Cow,
    pin::Pin,
    task::{Context, Poll, ready},
};

use base64::Engine as _;
use bytes::Bytes;
use http_body::Body;
use pin_project_lite::pin_project;
use tracing::{Span, trace};

use super::TARGET;

/// Length of the gRPC frame prefix: a 1-byte compressed flag and a 4-byte big-endian message
/// length.
const FRAME_PREFIX_LEN: usize = 5;

/// Per-request cap on captured messages. Every RPC method this layer captures is unary or
/// server-streaming, and reflection's own service isn't registered in the descriptor pool (so its
/// multi-message streams are never captured here either) — a well-behaved client's request body
/// always holds exactly one message. This instead guards against a hostile body: nothing stops a
/// client from stuffing extra tiny frames into a "unary" request after the real message, so this
/// stops that from amplifying into unbounded decode and log volume.
const MAX_CAPTURED_MESSAGES: u32 = 1;

pin_project! {
    /// Request body for [`GrpcRequestLog`], teeing data chunks into a gRPC frame parser while
    /// passing them through unchanged.
    ///
    /// [`GrpcRequestLog`]: super::GrpcRequestLog
    pub struct RequestLogBody<B> {
        #[pin]
        inner: B,
        capture: Option<CaptureState>,
    }
}

/// Everything needed to emit the messages of one request: the span carrying the filterable
/// `service`/`method` fields, and the frame parser's state.
pub(crate) struct CaptureState {
    span: Span,
    path: String,
    max_captured_message_size: usize,
    parser: FrameParser,
    message_index: u32,
}

/// Incremental parser for the gRPC message framing (a sequence of frames, each a
/// `FRAME_PREFIX_LEN`-byte prefix followed by that message's bytes). Chunk-boundary-agnostic: one
/// chunk may contain several frames or a fraction of one.
enum FrameParser {
    /// Accumulating the frame prefix.
    Prefix {
        buf: [u8; FRAME_PREFIX_LEN],
        filled: usize,
    },
    /// Accumulating `remaining` message bytes into `buf`.
    Message { remaining: usize, buf: Vec<u8> },
    /// Counting down a message that is not being buffered (compressed or over the size cap).
    Skip { remaining: usize },
    /// Parsing is over for this request — the framing was structurally invalid, or the
    /// per-request capture limit was reached. The rest of the stream passes through unparsed.
    Stopped,
}

/// A frame boundary [`FrameParser::feed`] reached.
enum FrameEvent {
    /// A complete, uncompressed message within the size cap.
    Message(Vec<u8>),
    /// A message whose bytes were counted but not buffered.
    Skipped {
        len: usize,
        reason: Cow<'static, str>,
    },
}

impl<B> RequestLogBody<B> {
    pub(crate) fn new(inner: B, capture: Option<CaptureState>) -> Self {
        Self { inner, capture }
    }
}

impl CaptureState {
    pub(crate) fn new(span: Span, path: String, max_captured_message_size: usize) -> Self {
        Self {
            span,
            path,
            max_captured_message_size,
            parser: FrameParser::new(),
            message_index: 0,
        }
    }

    /// Feed the next body chunk to the frame parser, emitting one event per completed frame.
    fn observe(&mut self, mut chunk: &[u8]) {
        while !chunk.is_empty() {
            // Checked before feeding the next frame, not after it completes, so an excess
            // frame's header never reaches the `Prefix` arm's `Vec::with_capacity` allocation in
            // `feed` — once the cap is hit, `feed` is never called again for this request. Gated
            // on `Prefix` (a fresh frame boundary) so this only fires once a message beyond the
            // cap actually arrives; a body with exactly `MAX_CAPTURED_MESSAGES` messages and
            // nothing more must not end in a spurious "too many messages" event.
            if self.message_index >= MAX_CAPTURED_MESSAGES
                && matches!(self.parser, FrameParser::Prefix { .. })
            {
                let _span = self.span.enter();
                trace!(
                    target: TARGET,
                    method = %self.path,
                    message_count = self.message_index,
                    "Captured request (capture stopped: too many messages)",
                );
                self.parser = FrameParser::Stopped;
                return;
            }

            let (consumed, completed) = self.parser.feed(chunk, self.max_captured_message_size);
            chunk = &chunk[consumed..];

            let Some(event) = completed else {
                continue;
            };

            // Enter the span so `EnvFilter` span-field directives (service/method) apply to the
            // event.
            let _span = self.span.enter();

            match event {
                FrameEvent::Message(message) => trace!(
                    target: TARGET,
                    method = %self.path,
                    message_index = self.message_index,
                    payload = %base64::engine::general_purpose::STANDARD.encode(&message),
                    "Captured request",
                ),
                FrameEvent::Skipped { len, reason } => trace!(
                    target: TARGET,
                    method = %self.path,
                    message_index = self.message_index,
                    message_len = len,
                    skipped = %reason,
                    "Captured request (payload skipped)",
                ),
            }
            self.message_index += 1;
        }
    }
}

impl FrameParser {
    fn new() -> Self {
        Self::Prefix {
            buf: [0; FRAME_PREFIX_LEN],
            filled: 0,
        }
    }

    /// Feed the parser up to `chunk.len()` bytes; returns how many bytes were consumed and, if a
    /// frame boundary was reached, what completed. Always consumes at least one byte of a
    /// non-empty chunk, so callers can loop until the chunk is exhausted.
    fn feed(&mut self, chunk: &[u8], max_message_size: usize) -> (usize, Option<FrameEvent>) {
        match self {
            Self::Prefix { buf, filled } => {
                let n = (FRAME_PREFIX_LEN - *filled).min(chunk.len());
                buf[*filled..*filled + n].copy_from_slice(&chunk[..n]);
                *filled += n;
                if *filled < FRAME_PREFIX_LEN {
                    return (n, None);
                }

                let flag = buf[0];
                let len = u32::from_be_bytes(buf[1..].try_into().unwrap()) as usize;
                if flag > 1 {
                    *self = Self::Stopped;
                    return (
                        n,
                        Some(FrameEvent::Skipped {
                            len,
                            reason: format!("invalid flag {flag}").into(),
                        }),
                    );
                }
                if flag == 1 {
                    *self = Self::skip(len);
                    return (
                        n,
                        Some(FrameEvent::Skipped {
                            len,
                            reason: "compressed message".into(),
                        }),
                    );
                }
                if len > max_message_size {
                    *self = Self::skip(len);
                    return (
                        n,
                        Some(FrameEvent::Skipped {
                            len,
                            reason: "message too large".into(),
                        }),
                    );
                }
                if len == 0 {
                    *self = Self::new();
                    return (n, Some(FrameEvent::Message(Vec::new())));
                }
                *self = Self::Message {
                    remaining: len,
                    // Reserve only what this chunk already has in hand, not the full claimed
                    // `len` (an unverified remote-supplied value) — `buf` grows via
                    // `extend_from_slice` as further chunks arrive, so a client that sends just
                    // the prefix and stalls costs ~0 bytes here, regardless of `len`.
                    buf: Vec::with_capacity((chunk.len() - n).min(len)),
                };
                (n, None)
            }
            Self::Message { remaining, buf } => {
                let n = (*remaining).min(chunk.len());
                buf.extend_from_slice(&chunk[..n]);
                *remaining -= n;
                if *remaining > 0 {
                    return (n, None);
                }
                let message = std::mem::take(buf);
                *self = Self::new();
                (n, Some(FrameEvent::Message(message)))
            }
            Self::Skip { remaining } => {
                let n = (*remaining).min(chunk.len());
                *remaining -= n;
                if *remaining == 0 {
                    *self = Self::new();
                }
                (n, None)
            }
            Self::Stopped => (chunk.len(), None),
        }
    }

    /// Enter [`FrameParser::Skip`] for `len` bytes (or start the next prefix immediately for an
    /// empty message).
    fn skip(len: usize) -> Self {
        if len == 0 {
            Self::new()
        } else {
            Self::Skip { remaining: len }
        }
    }
}

impl<B> Body for RequestLogBody<B>
where
    B: Body<Data = Bytes>,
{
    type Data = Bytes;
    type Error = B::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        let this = self.project();
        let result = ready!(this.inner.poll_frame(cx));

        if let (Some(capture), Some(Ok(frame))) = (this.capture.as_mut(), result.as_ref())
            && let Some(chunk) = frame.data_ref()
        {
            capture.observe(chunk.as_ref());
        }

        Poll::Ready(result)
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.inner.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed `chunks` through a fresh parser with `max` as the size cap, returning the events in
    /// order. `Message` events are mapped to `Ok(bytes)`, `Skipped` events to `Err(reason)`.
    fn parse(chunks: &[&[u8]], max: usize) -> Vec<Result<Vec<u8>, Cow<'static, str>>> {
        let mut parser = FrameParser::new();
        let mut events = Vec::new();
        for mut chunk in chunks.iter().copied() {
            while !chunk.is_empty() {
                let (consumed, completed) = parser.feed(chunk, max);
                chunk = &chunk[consumed..];
                match completed {
                    Some(FrameEvent::Message(message)) => events.push(Ok(message)),
                    Some(FrameEvent::Skipped { reason, .. }) => events.push(Err(reason)),
                    None => {}
                }
            }
        }
        events
    }

    fn frame(flag: u8, message: &[u8]) -> Vec<u8> {
        let mut frame = vec![flag];
        frame.extend_from_slice(&(message.len() as u32).to_be_bytes());
        frame.extend_from_slice(message);
        frame
    }

    #[test]
    fn single_frame_single_chunk() {
        let events = parse(&[&frame(0, b"hello")], 1024);
        assert_eq!(events, vec![Ok(b"hello".to_vec())]);
    }

    #[test]
    fn empty_message() {
        let events = parse(&[&frame(0, b"")], 1024);
        assert_eq!(events, vec![Ok(Vec::new())]);
    }

    #[test]
    fn frame_split_across_chunks() {
        let bytes = frame(0, b"hello");
        let chunks: Vec<&[u8]> = bytes.chunks(1).collect();
        let events = parse(&chunks, 1024);
        assert_eq!(events, vec![Ok(b"hello".to_vec())]);
    }

    #[test]
    fn multiple_frames_in_one_chunk() {
        let mut bytes = frame(0, b"one");
        bytes.extend_from_slice(&frame(0, b"two"));
        let events = parse(&[&bytes], 1024);
        assert_eq!(events, vec![Ok(b"one".to_vec()), Ok(b"two".to_vec())]);
    }

    #[test]
    fn oversized_message_skipped_next_frame_still_parses() {
        let mut bytes = frame(0, b"too big for the cap");
        bytes.extend_from_slice(&frame(0, b"ok"));
        let events = parse(&[&bytes], 4);
        assert_eq!(
            events,
            vec![Err("message too large".into()), Ok(b"ok".to_vec())]
        );
    }

    #[test]
    fn compressed_message_skipped_next_frame_still_parses() {
        let mut bytes = frame(1, b"compressed");
        bytes.extend_from_slice(&frame(0, b"ok"));
        let events = parse(&[&bytes], 1024);
        assert_eq!(
            events,
            vec![Err("compressed message".into()), Ok(b"ok".to_vec())]
        );
    }

    #[test]
    fn empty_compressed_message_skipped() {
        let mut bytes = frame(1, b"");
        bytes.extend_from_slice(&frame(0, b"ok"));
        let events = parse(&[&bytes], 1024);
        assert_eq!(
            events,
            vec![Err("compressed message".into()), Ok(b"ok".to_vec())]
        );
    }

    #[test]
    fn invalid_flag_poisons_parser() {
        let mut bytes = frame(2, b"garbage");
        bytes.extend_from_slice(&frame(0, b"never parsed"));
        let events = parse(&[&bytes], 1024);
        assert_eq!(events, vec![Err("invalid flag 2".into())]);
    }

    #[test]
    fn truncated_trailing_bytes_emit_nothing() {
        let bytes = frame(0, b"hello");
        let events = parse(&[&bytes[..bytes.len() - 1]], 1024);
        assert_eq!(events, Vec::new());
    }
}
