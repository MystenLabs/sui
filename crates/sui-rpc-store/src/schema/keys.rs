// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Key types shared across multiple `sui-rpc-store` CFs.
//!
//! Keys used by only one CF live in that CF's own module; only the
//! types reused across many CFs land here.

use bytes::Buf;
use bytes::BufMut;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;

/// A `u64` encoded big-endian, suitable for keys whose iteration
/// order should match numerical order. Shared across CFs keyed by
/// transaction sequence, checkpoint sequence, and epoch id, and
/// reused as the value type for the `tx_seq_by_digest` and
/// `checkpoint_seq_by_digest` lookup CFs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct U64Be(pub u64);

impl Encode for U64Be {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(&self.0.to_be_bytes());
        Ok(())
    }
}

impl Decode for U64Be {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() != 8 {
            return Err(DecodeError::msg(format!(
                "expected 8 bytes for U64Be, got {}",
                buf.remaining(),
            )));
        }
        Ok(U64Be(buf.get_u64()))
    }
}

/// Zero-byte key for singleton CFs. Encodes as the empty byte
/// string; decoding requires the input to be empty too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnitKey;

impl Encode for UnitKey {
    fn encode_into<B: BufMut>(&self, _buf: &mut B) -> Result<(), EncodeError> {
        Ok(())
    }
}

impl Decode for UnitKey {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.has_remaining() {
            return Err(DecodeError::msg(format!(
                "expected 0 bytes for UnitKey, got {}",
                buf.remaining(),
            )));
        }
        Ok(UnitKey)
    }
}
