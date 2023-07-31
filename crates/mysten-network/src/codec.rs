// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bytes::{Buf, BufMut};
use std::{io::Read, marker::PhantomData};
use tonic::{
    codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder},
    Status,
};

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
        let compressed_size = buf.remaining();
        if compressed_size == 0 {
            return Ok(None);
        }
        let mut snappy_decoder = snap::read::FrameDecoder::new(buf.reader());
        let mut bytes = Vec::with_capacity(compressed_size);
        snappy_decoder.read_to_end(&mut bytes)?;
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
    use std::{io::Read, marker::PhantomData};

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
            let compressed_size = buf.len();
            let mut snappy_decoder = snap::read::FrameDecoder::new(buf.reader()).take(1 << 30);
            let mut bytes = Vec::with_capacity(compressed_size);
            snappy_decoder.read_to_end(&mut bytes)?;
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
