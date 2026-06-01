// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `CheckpointDigest` → `checkpoint_seq`.
//!
//! Resolves a checkpoint digest to its sequence number, which then
//! keys every checkpoint-keyed CF.

use bytes::Buf;
use bytes::BufMut;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_types::digests::CheckpointDigest;

use crate::schema::keys::U64Varint;

pub const NAME: &str = "checkpoint_seq_by_digest";

/// Wrapper around `CheckpointDigest` whose encoding is the raw 32
/// bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key(pub CheckpointDigest);

pub type Value = U64Varint;

impl Encode for Key {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.0.inner());
        Ok(())
    }
}

impl Decode for Key {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() != 32 {
            return Err(DecodeError::msg(format!(
                "expected 32 bytes for {NAME} Key, got {}",
                buf.remaining(),
            )));
        }
        let mut bytes = [0u8; 32];
        buf.copy_to_slice(&mut bytes);
        Ok(Key(CheckpointDigest::new(bytes)))
    }
}

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
