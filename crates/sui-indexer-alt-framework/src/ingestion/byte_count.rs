// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bytes::Buf;
use mysten_network::callback::{MakeCallbackHandler, ResponseHandler};
use prometheus::IntCounter;

/// A [`MakeCallbackHandler`] that increments an [`IntCounter`] by the number of bytes observed
/// in the response body of each request.
#[derive(Clone)]
pub(crate) struct ByteCountMakeCallbackHandler {
    counter: IntCounter,
}

impl ByteCountMakeCallbackHandler {
    pub(crate) fn new(counter: IntCounter) -> Self {
        Self { counter }
    }
}

impl MakeCallbackHandler for ByteCountMakeCallbackHandler {
    type Handler = ByteCountResponseHandler;

    fn make_handler(&self, _request: &http::request::Parts) -> Self::Handler {
        ByteCountResponseHandler {
            counter: self.counter.clone(),
        }
    }
}

pub(crate) struct ByteCountResponseHandler {
    counter: IntCounter,
}

impl ResponseHandler for ByteCountResponseHandler {
    fn on_response(&mut self, _response: &http::response::Parts) {}

    fn on_error<E>(&mut self, _error: &E)
    where
        E: std::fmt::Display + 'static,
    {
    }

    fn on_body_chunk<B>(&mut self, chunk: &B)
    where
        B: Buf,
    {
        self.counter.inc_by(chunk.remaining() as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use bytes::Bytes;

    fn run_byte_count(frames: &[&'static str]) -> u64 {
        let counter = IntCounter::new("test_bytes", "test").unwrap();
        let make = ByteCountMakeCallbackHandler::new(counter.clone());

        let request = http::Request::new(());
        let (parts, _) = request.into_parts();
        let mut handler = make.make_handler(&parts);

        for frame in frames {
            handler.on_body_chunk(&Bytes::from_static(frame.as_bytes()));
        }

        counter.get()
    }

    #[test]
    fn test_byte_count_0_frames() {
        assert_eq!(run_byte_count(&[]), 0);
    }

    #[test]
    fn test_byte_count_1_frame() {
        assert_eq!(run_byte_count(&["a"]), 1);
    }

    #[test]
    fn test_byte_count_2_frames() {
        assert_eq!(run_byte_count(&["a", "bb"]), 3);
    }
}
