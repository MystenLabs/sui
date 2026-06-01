// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `TransactionDigest` → `tx_seq`.
//!
//! One half of the digest <-> sequence bijection. The inverse lives
//! in [`super::tx_meta_by_seq`](super::tx_meta_by_seq).

use bytes::Buf;
use bytes::BufMut;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_types::digests::TransactionDigest;

use crate::schema::keys::U64Varint;

pub const NAME: &str = "tx_seq_by_digest";

/// Wrapper around `TransactionDigest` whose encoding is the raw 32
/// bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key(pub TransactionDigest);

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
        Ok(Key(TransactionDigest::new(bytes)))
    }
}

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
