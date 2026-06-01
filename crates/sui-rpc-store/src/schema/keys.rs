// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Key types shared across multiple `sui-rpc-store` CFs.
//!
//! Keys used by only one CF live in that CF's own module; only the
//! types reused across many CFs land here.

use bytes::Buf;
use bytes::BufMut;
use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use move_core_types::language_storage::TypeTag;
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

/// Newtype wrapping `StructTag` with a streaming-friendly
/// `Encode` / `Decode` pair.
///
/// On the wire, the bytes are **exactly** what `bcs::to_bytes`
/// would produce for the same `StructTag` — encode delegates to
/// `bcs::to_bytes`, and decode is a hand-rolled streaming parser
/// that consumes one `StructTag`'s worth of bytes and leaves any
/// trailing bytes intact. The byte-equivalence to BCS matters
/// because the sort order of these keys is determined by the
/// on-disk bytes; layering an extra framing prefix would change
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructTagKey(pub StructTag);

impl Encode for StructTagKey {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        let bytes = bcs::to_bytes(&self.0)
            .map_err(|e| EncodeError::with_source("bcs encode StructTag", e))?;
        buf.put_slice(&bytes);
        Ok(())
    }
}

impl Decode for StructTagKey {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        let tag = read_struct_tag(buf)?;
        Ok(StructTagKey(tag))
    }
}

/// Newtype wrapping `TypeTag` with the same byte-identical, but
/// streaming-friendly `Encode` / `Decode` shape as
/// [`StructTagKey`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeTagKey(pub TypeTag);

impl Encode for TypeTagKey {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        let bytes = bcs::to_bytes(&self.0)
            .map_err(|e| EncodeError::with_source("bcs encode TypeTag", e))?;
        buf.put_slice(&bytes);
        Ok(())
    }
}

impl Decode for TypeTagKey {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        let tag = read_type_tag(buf)?;
        Ok(TypeTagKey(tag))
    }
}

/// Read one `StructTag` from the head of `buf`, advancing past
/// exactly its bytes.
pub(crate) fn read_struct_tag<B: Buf>(buf: &mut B) -> Result<StructTag, DecodeError> {
    if buf.remaining() < AccountAddress::LENGTH {
        return Err(DecodeError::msg(format!(
            "StructTag truncated at address: {} bytes left",
            buf.remaining(),
        )));
    }
    let mut addr = [0u8; AccountAddress::LENGTH];
    buf.copy_to_slice(&mut addr);
    let address = AccountAddress::new(addr);
    let module = read_identifier(buf)?;
    let name = read_identifier(buf)?;
    let n_params = read_uleb128(buf)? as usize;
    let mut type_params = Vec::with_capacity(n_params);
    for _ in 0..n_params {
        type_params.push(read_type_tag(buf)?);
    }
    Ok(StructTag {
        address,
        module,
        name,
        type_params,
    })
}

/// Read one `TypeTag` from the head of `buf`, advancing past
/// exactly its bytes.
pub(crate) fn read_type_tag<B: Buf>(buf: &mut B) -> Result<TypeTag, DecodeError> {
    let variant = read_uleb128(buf)?;
    Ok(match variant {
        0 => TypeTag::Bool,
        1 => TypeTag::U8,
        2 => TypeTag::U64,
        3 => TypeTag::U128,
        4 => TypeTag::Address,
        5 => TypeTag::Signer,
        6 => TypeTag::Vector(Box::new(read_type_tag(buf)?)),
        7 => TypeTag::Struct(Box::new(read_struct_tag(buf)?)),
        8 => TypeTag::U16,
        9 => TypeTag::U32,
        10 => TypeTag::U256,
        v => {
            return Err(DecodeError::msg(format!("unknown TypeTag variant: {v}",)));
        }
    })
}

/// Read a BCS uleb128-encoded `u32`. Matches the canonical
/// encoding bcs uses (rejects non-canonical zero-padded forms).
fn read_uleb128<B: Buf>(buf: &mut B) -> Result<u32, DecodeError> {
    let mut value: u64 = 0;
    for shift in (0..32).step_by(7) {
        if !buf.has_remaining() {
            return Err(DecodeError::msg("uleb128 truncated"));
        }
        let byte = buf.get_u8();
        let digit = byte & 0x7f;
        value |= u64::from(digit) << shift;
        if digit == byte {
            if shift > 0 && digit == 0 {
                return Err(DecodeError::msg("non-canonical uleb128"));
            }
            return u32::try_from(value).map_err(|_| DecodeError::msg("uleb128 overflow"));
        }
    }
    Err(DecodeError::msg("uleb128 overflow"))
}

/// Read a BCS-encoded `Identifier`: uleb128 length followed by
/// UTF-8 bytes.
fn read_identifier<B: Buf>(buf: &mut B) -> Result<Identifier, DecodeError> {
    let len = read_uleb128(buf)? as usize;
    if buf.remaining() < len {
        return Err(DecodeError::msg(format!(
            "Identifier truncated: need {len} bytes, have {}",
            buf.remaining(),
        )));
    }
    let bytes = buf.copy_to_bytes(len);
    let s =
        std::str::from_utf8(&bytes).map_err(|e| DecodeError::with_source("Identifier utf8", e))?;
    Identifier::new(s).map_err(|e| DecodeError::with_source("Identifier", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident(s: &str) -> Identifier {
        Identifier::new(s).unwrap()
    }

    fn struct_tag(addr: u8, module: &str, name: &str, params: Vec<TypeTag>) -> StructTag {
        StructTag {
            address: AccountAddress::new([addr; 32]),
            module: ident(module),
            name: ident(name),
            type_params: params,
        }
    }

    /// Encode through both `StructTagKey::encode` and
    /// `bcs::to_bytes` and assert the bytes match — the
    /// byte-identity contract is what preserves on-disk sort order
    /// against any consumer that historically wrote the same
    /// `StructTag` via raw BCS.
    fn assert_struct_tag_byte_identical(tag: &StructTag) {
        let via_key = StructTagKey(tag.clone()).encode().unwrap();
        let via_bcs = bcs::to_bytes(tag).unwrap();
        assert_eq!(via_key, via_bcs);
    }

    fn round_trip_struct(tag: StructTag) {
        assert_struct_tag_byte_identical(&tag);
        let bytes = StructTagKey(tag.clone()).encode().unwrap();
        let decoded = StructTagKey::decode(&mut &bytes[..]).unwrap();
        assert_eq!(decoded.0, tag);
    }

    fn round_trip_type(tag: TypeTag) {
        let bytes = TypeTagKey(tag.clone()).encode().unwrap();
        assert_eq!(bytes, bcs::to_bytes(&tag).unwrap());
        let decoded = TypeTagKey::decode(&mut &bytes[..]).unwrap();
        assert_eq!(decoded.0, tag);
    }

    /// Streaming-decode contract: the decoder must consume exactly
    /// one tag's bytes and leave anything after untouched.
    #[test]
    fn struct_tag_leaves_trailing_bytes_intact() {
        let tag = struct_tag(2, "sui", "SUI", vec![]);
        let mut bytes = bcs::to_bytes(&tag).unwrap();
        let trailer = b"trailing payload";
        bytes.extend_from_slice(trailer);

        let mut cursor: &[u8] = &bytes;
        let decoded = StructTagKey::decode(&mut cursor).unwrap();
        assert_eq!(decoded.0, tag);
        assert_eq!(cursor, trailer);
    }

    #[test]
    fn struct_tag_no_type_params() {
        round_trip_struct(struct_tag(2, "sui", "SUI", vec![]));
    }

    #[test]
    fn struct_tag_with_primitive_param() {
        round_trip_struct(struct_tag(2, "balance", "Balance", vec![TypeTag::U64]));
    }

    #[test]
    fn struct_tag_with_nested_struct_param() {
        // `Coin<0x2::sui::SUI>`.
        round_trip_struct(struct_tag(
            2,
            "coin",
            "Coin",
            vec![TypeTag::Struct(Box::new(struct_tag(
                2,
                "sui",
                "SUI",
                vec![],
            )))],
        ));
    }

    #[test]
    fn struct_tag_with_vector_of_struct_param() {
        // `Bag<vector<0x2::sui::SUI>>`.
        round_trip_struct(struct_tag(
            2,
            "bag",
            "Bag",
            vec![TypeTag::Vector(Box::new(TypeTag::Struct(Box::new(
                struct_tag(2, "sui", "SUI", vec![]),
            ))))],
        ));
    }

    #[test]
    fn type_tag_primitive_variants_round_trip() {
        for tag in [
            TypeTag::Bool,
            TypeTag::U8,
            TypeTag::U16,
            TypeTag::U32,
            TypeTag::U64,
            TypeTag::U128,
            TypeTag::U256,
            TypeTag::Address,
            TypeTag::Signer,
        ] {
            round_trip_type(tag);
        }
    }

    #[test]
    fn type_tag_nested_vectors_round_trip() {
        round_trip_type(TypeTag::Vector(Box::new(TypeTag::Vector(Box::new(
            TypeTag::U64,
        )))));
    }

    #[test]
    fn type_tag_decode_rejects_unknown_variant() {
        // BCS uleb128 11 is one byte (`0x0b`) — past the highest
        // defined TypeTag variant (U256 at index 10).
        let err = TypeTagKey::decode(&mut &[0x0bu8][..]).unwrap_err();
        assert!(
            err.to_string().contains("unknown TypeTag variant"),
            "unexpected error: {err}",
        );
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
