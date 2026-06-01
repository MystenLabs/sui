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

/// A `u64` encoded as a protobuf-style varint, suitable for
/// *value* positions where on-disk size matters more than sort
/// order. Compared to [`U64Be`]: smaller for typical values (1–5
/// bytes for anything below `2^35`), slightly larger only near the
/// `u64::MAX` corner (up to 10 bytes), and never sort-stable —
/// hence values only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct U64Varint(pub u64);

impl Encode for U64Varint {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        prost::encoding::encode_varint(self.0, buf);
        Ok(())
    }
}

impl Decode for U64Varint {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        let value = prost::encoding::decode_varint(buf)
            .map_err(|e| DecodeError::with_source("decode varint", e))?;
        if buf.has_remaining() {
            return Err(DecodeError::msg(format!(
                "expected exact varint length, {} bytes remain",
                buf.remaining(),
            )));
        }
        Ok(U64Varint(value))
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
