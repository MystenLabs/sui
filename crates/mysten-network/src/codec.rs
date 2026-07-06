// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bytes::{Buf, BufMut};
use std::{io::Read, marker::PhantomData};
use tonic::{
    Status,
    codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder},
};

/// Default upper bound on total decompressed payload size.
const MAX_DECOMPRESSED_SIZE: u64 = 128 << 20;

/// Decompress a snappy-framed stream, bounding output at
/// [`MAX_DECOMPRESSED_SIZE`].
fn decompress_snappy<R: Read>(src: R) -> std::io::Result<Vec<u8>> {
    decompress_snappy_bounded(src, MAX_DECOMPRESSED_SIZE)
}

/// Decompress a snappy-framed stream, bounding total output at
/// `max_allowed` bytes. Exposed separately from [`decompress_snappy`] so
/// tests can exercise the bound directly.
fn decompress_snappy_bounded<R: Read>(src: R, max_allowed: u64) -> std::io::Result<Vec<u8>> {
    let mut snappy_decoder = snap::read::FrameDecoder::new(src).take(max_allowed);
    let mut bytes = Vec::new();
    snappy_decoder.read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[derive(Debug)]
pub struct BcsEncoder<T>(PhantomData<T>);

impl<T: serde::Serialize> Encoder for BcsEncoder<T> {
    type Item = T;
    type Error = Status;

    fn encode(&mut self, item: Self::Item, buf: &mut EncodeBuf<'_>) -> Result<(), Self::Error> {
        bcs::serialize_into(&mut buf.writer(), &item).map_err(|e| Status::internal(e.to_string()))
    }
}

#[derive(Debug)]
pub struct BcsDecoder<U>(PhantomData<U>);

impl<U: serde::de::DeserializeOwned> Decoder for BcsDecoder<U> {
    type Item = U;
    type Error = Status;

    fn decode(&mut self, buf: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Self::Error> {
        if !buf.has_remaining() {
            return Ok(None);
        }

        let chunk = buf.chunk();

        let item: Self::Item =
            bcs::from_bytes(chunk).map_err(|e| Status::internal(e.to_string()))?;
        buf.advance(chunk.len());

        Ok(Some(item))
    }
}

/// A [`Codec`] that implements `application/grpc+bcs` via the serde library.
#[derive(Debug, Clone)]
pub struct BcsCodec<T, U>(PhantomData<(T, U)>);

impl<T, U> Default for BcsCodec<T, U> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T, U> Codec for BcsCodec<T, U>
where
    T: serde::Serialize + Send + 'static,
    U: serde::de::DeserializeOwned + Send + 'static,
{
    type Encode = T;
    type Decode = U;
    type Encoder = BcsEncoder<T>;
    type Decoder = BcsDecoder<U>;

    fn encoder(&mut self) -> Self::Encoder {
        BcsEncoder(PhantomData)
    }

    fn decoder(&mut self) -> Self::Decoder {
        BcsDecoder(PhantomData)
    }
}

#[derive(Debug)]
pub struct BcsSnappyEncoder<T>(PhantomData<T>);

impl<T: serde::Serialize> Encoder for BcsSnappyEncoder<T> {
    type Item = T;
    type Error = Status;

    fn encode(&mut self, item: Self::Item, buf: &mut EncodeBuf<'_>) -> Result<(), Self::Error> {
        let mut snappy_encoder = snap::write::FrameEncoder::new(buf.writer());
        bcs::serialize_into(&mut snappy_encoder, &item).map_err(|e| Status::internal(e.to_string()))
    }
}

#[derive(Debug)]
pub struct BcsSnappyDecoder<U>(PhantomData<U>);

impl<U: serde::de::DeserializeOwned> Decoder for BcsSnappyDecoder<U> {
    type Item = U;
    type Error = Status;

    fn decode(&mut self, buf: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Self::Error> {
        if !buf.has_remaining() {
            return Ok(None);
        }
        let bytes = decompress_snappy(buf.reader()).map_err(|e| Status::internal(e.to_string()))?;
        let item =
            bcs::from_bytes(bytes.as_slice()).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Some(item))
    }
}

/// A [`Codec`] that implements `bcs` encoding/decoding and snappy compression/decompression
/// via the serde library.
#[derive(Debug, Clone)]
pub struct BcsSnappyCodec<T, U>(PhantomData<(T, U)>);

impl<T, U> Default for BcsSnappyCodec<T, U> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T, U> Codec for BcsSnappyCodec<T, U>
where
    T: serde::Serialize + Send + 'static,
    U: serde::de::DeserializeOwned + Send + 'static,
{
    type Encode = T;
    type Decode = U;
    type Encoder = BcsSnappyEncoder<T>;
    type Decoder = BcsSnappyDecoder<U>;

    fn encoder(&mut self) -> Self::Encoder {
        BcsSnappyEncoder(PhantomData)
    }

    fn decoder(&mut self) -> Self::Decoder {
        BcsSnappyDecoder(PhantomData)
    }
}

// Anemo variant of BCS codec using Snappy for compression.
pub mod anemo {
    use ::anemo::rpc::codec::{Codec, Decoder, Encoder};
    use bytes::Buf;
    use std::marker::PhantomData;

    #[derive(Debug)]
    pub struct BcsSnappyEncoder<T>(PhantomData<T>);

    impl<T: serde::Serialize> Encoder for BcsSnappyEncoder<T> {
        type Item = T;
        type Error = bcs::Error;

        fn encode(&mut self, item: Self::Item) -> Result<bytes::Bytes, Self::Error> {
            let mut buf = Vec::<u8>::new();
            let mut snappy_encoder = snap::write::FrameEncoder::new(&mut buf);
            bcs::serialize_into(&mut snappy_encoder, &item)?;
            drop(snappy_encoder);
            Ok(buf.into())
        }
    }

    #[derive(Debug)]
    pub struct BcsSnappyDecoder<U>(PhantomData<U>);

    impl<U: serde::de::DeserializeOwned> Decoder for BcsSnappyDecoder<U> {
        type Item = U;
        type Error = bcs::Error;

        fn decode(&mut self, buf: bytes::Bytes) -> Result<Self::Item, Self::Error> {
            let bytes = super::decompress_snappy(buf.reader())?;
            bcs::from_bytes(bytes.as_slice())
        }
    }

    /// A [`Codec`] that implements `bcs` encoding/decoding via the serde library.
    #[derive(Debug, Clone)]
    pub struct BcsSnappyCodec<T, U>(PhantomData<(T, U)>);

    impl<T, U> Default for BcsSnappyCodec<T, U> {
        fn default() -> Self {
            Self(PhantomData)
        }
    }

    impl<T, U> Codec for BcsSnappyCodec<T, U>
    where
        T: serde::Serialize + Send + 'static,
        U: serde::de::DeserializeOwned + Send + 'static,
    {
        type Encode = T;
        type Decode = U;
        type Encoder = BcsSnappyEncoder<T>;
        type Decoder = BcsSnappyDecoder<U>;

        fn encoder(&mut self) -> Self::Encoder {
            BcsSnappyEncoder(PhantomData)
        }

        fn decoder(&mut self) -> Self::Decoder {
            BcsSnappyDecoder(PhantomData)
        }

        fn format_name(&self) -> &'static str {
            "bcs"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::anemo::rpc::codec::{
        Codec as AnemoCodec, Decoder as AnemoDecoder, Encoder as AnemoEncoder,
    };

    fn snappy_compress(raw: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = snap::write::FrameEncoder::new(&mut out);
        std::io::Write::write_all(&mut encoder, raw).unwrap();
        drop(encoder);
        out
    }

    #[test]
    fn anemo_roundtrip() {
        let mut codec: anemo::BcsSnappyCodec<Vec<u64>, Vec<u64>> = anemo::BcsSnappyCodec::default();
        let value = vec![1u64, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let encoded = codec.encoder().encode(value.clone()).unwrap();
        let decoded = codec.decoder().decode(encoded).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn bounded_helper_respects_output_limit() {
        // With `max_allowed` set below the stream's decompressed size,
        // `decompress_snappy_bounded` returns exactly `max_allowed` bytes.
        let raw = vec![0u8; 2 * 1024 * 1024];
        let compressed = snappy_compress(&raw);
        let limit = 1024u64;
        let out = decompress_snappy_bounded(&compressed[..], limit).unwrap();
        assert_eq!(out.len() as u64, limit);
        assert!((out.len() as u64) < raw.len() as u64);
    }

    // Tonic-variant round trip via a bespoke HttpBody. Building a real
    // `DecodeBuf` requires tonic's internal API, so we drive the decoder
    // through `tonic::codec::Streaming::new_request`, which is the path used
    // by the gRPC server. This exercises the real `BcsSnappyDecoder::decode`.
    mod tonic_via_streaming {
        use super::super::*;
        use super::snappy_compress;
        use bytes::{BufMut, Bytes, BytesMut};
        use futures::StreamExt;
        use http_body::{Body as HttpBody, Frame};
        use std::pin::Pin;
        use std::task::{Context, Poll};

        /// Minimal HttpBody yielding a single gRPC-framed payload. gRPC length-
        /// prefixed frames are `[compression:u8][length:u32 BE][payload]`.
        struct OneFrameBody(Option<Bytes>);

        impl OneFrameBody {
            fn new(payload: Bytes) -> Self {
                let mut framed = BytesMut::with_capacity(5 + payload.len());
                framed.put_u8(0);
                framed.put_u32(payload.len() as u32);
                framed.put_slice(&payload);
                Self(Some(framed.freeze()))
            }
        }

        impl HttpBody for OneFrameBody {
            type Data = Bytes;
            type Error = std::convert::Infallible;

            fn poll_frame(
                mut self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
            ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
                Poll::Ready(self.0.take().map(|b| Ok(Frame::data(b))))
            }

            fn is_end_stream(&self) -> bool {
                self.0.is_none()
            }
        }

        #[tokio::test]
        async fn tonic_roundtrip() {
            let mut codec: BcsSnappyCodec<Vec<u64>, Vec<u64>> = BcsSnappyCodec::default();
            let value = vec![1u64, 2, 3, 4, 5, 6, 7, 8, 9, 10];
            let raw = bcs::to_bytes(&value).unwrap();
            let compressed = snappy_compress(&raw);
            let body = OneFrameBody::new(Bytes::from(compressed));
            let mut stream =
                tonic::codec::Streaming::new_request(codec.decoder(), body, None, None);
            let decoded = stream.next().await.unwrap().unwrap();
            assert_eq!(decoded, value);
        }
    }
}
