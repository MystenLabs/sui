// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `(dimension_key, bucket)` → `BitmapBlob`.
//!
//! Same wire shape as [`super::transaction_bitmap`](super::transaction_bitmap)
//! but indexes packed-event-seq space — i.e. each set bit
//! identifies a single event by `(tx_seq << EVENT_BITS) | event_idx`.

use bytes::Buf;
use bytes::BufMut;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::Protobuf;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;

use crate::proto::BitmapBlob;

pub const NAME: &str = "event_bitmap";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key {
    pub dimension_key: Vec<u8>,
    pub bucket: u64,
}

pub type Value = Protobuf<BitmapBlob>;

impl Encode for Key {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(&self.dimension_key);
        buf.put_slice(&self.bucket.to_be_bytes());
        Ok(())
    }
}

impl Decode for Key {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() < 8 {
            return Err(DecodeError::msg(format!(
                "{NAME} Key too short: {} bytes",
                buf.remaining(),
            )));
        }
        let dim_len = buf.remaining() - 8;
        let dim_bytes = buf.copy_to_bytes(dim_len);
        let bucket = buf.get_u64();
        Ok(Key {
            dimension_key: dim_bytes.to_vec(),
            bucket,
        })
    }
}

// TODO: install bitmap-union merge operator and a per-bucket
// compaction filter that converts the pruning floor from tx_seq
// space into packed-event-seq space.
pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
