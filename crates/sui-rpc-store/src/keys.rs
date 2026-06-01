// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Key types shared across `sui-rpc-store` schema modules.
//!
//! Each key is a newtype carrying an `Encode` / `Decode` impl that
//! pins the on-disk byte layout. The pinning matters: changing a
//! key's layout is a migration, so keys live here so the layout is
//! visible alongside the type rather than buried in a `DbMap`
//! declaration.
//!
//! # Encoding conventions
//!
//! - Fixed-size types (`ObjectID`, digests) encode as raw bytes;
//!   they sort lexicographically, which is the desired order.
//! - `u64`s in keys encode big-endian so prefix scans walk the
//!   natural numerical order (oldest tx_seq first, etc.).
//! - Composite keys encode their components in declaration order,
//!   with no separator: each component has a known fixed width or
//!   carries its own length in its wire format. Authors must not
//!   reorder the fields without writing a migration.

use bytes::Buf;
use bytes::BufMut;
use move_core_types::language_storage::StructTag;
use move_core_types::language_storage::TypeTag;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SUI_ADDRESS_LENGTH;
use sui_types::base_types::SuiAddress;
use sui_types::digests::CheckpointDigest;
use sui_types::digests::TransactionDigest;

// === Primitive newtypes =========================================

/// A `u64` encoded big-endian, suitable for keys whose iteration
/// order should match numerical order. Shared across CFs keyed by
/// transaction sequence, checkpoint sequence, and epoch id.
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

/// Wrapper around `ObjectID` whose encoding is the raw 32 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectIdKey(pub ObjectID);

impl Encode for ObjectIdKey {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.0.as_ref());
        Ok(())
    }
}

impl Decode for ObjectIdKey {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() != ObjectID::LENGTH {
            return Err(DecodeError::msg(format!(
                "expected {} bytes for ObjectIdKey, got {}",
                ObjectID::LENGTH,
                buf.remaining(),
            )));
        }
        let mut bytes = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut bytes);
        Ok(ObjectIdKey(ObjectID::new(bytes)))
    }
}

/// Wrapper around `TransactionDigest` whose encoding is the raw 32
/// bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TxDigestKey(pub TransactionDigest);

impl Encode for TxDigestKey {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.0.inner());
        Ok(())
    }
}

impl Decode for TxDigestKey {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() != 32 {
            return Err(DecodeError::msg(format!(
                "expected 32 bytes for TxDigestKey, got {}",
                buf.remaining(),
            )));
        }
        let mut bytes = [0u8; 32];
        buf.copy_to_slice(&mut bytes);
        Ok(TxDigestKey(TransactionDigest::new(bytes)))
    }
}

/// Wrapper around `CheckpointDigest` whose encoding is the raw 32
/// bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CkptDigestKey(pub CheckpointDigest);

impl Encode for CkptDigestKey {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.0.inner());
        Ok(())
    }
}

impl Decode for CkptDigestKey {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() != 32 {
            return Err(DecodeError::msg(format!(
                "expected 32 bytes for CkptDigestKey, got {}",
                buf.remaining(),
            )));
        }
        let mut bytes = [0u8; 32];
        buf.copy_to_slice(&mut bytes);
        Ok(CkptDigestKey(CheckpointDigest::new(bytes)))
    }
}

// === Composite keys =============================================

/// `(ObjectID, version)` — the key shape of the `objects` CF.
///
/// Fixed 40 bytes: 32 for the id followed by 8 for the version
/// (big-endian). Versions of the same object cluster in sorted
/// order, so a prefix scan on `id` walks them oldest-first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectVersionKey {
    pub id: ObjectID,
    pub version: SequenceNumber,
}

impl Encode for ObjectVersionKey {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.id.as_ref());
        buf.put_slice(&self.version.value().to_be_bytes());
        Ok(())
    }
}

impl Decode for ObjectVersionKey {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() != ObjectID::LENGTH + 8 {
            return Err(DecodeError::msg(format!(
                "expected {} bytes for ObjectVersionKey, got {}",
                ObjectID::LENGTH + 8,
                buf.remaining(),
            )));
        }
        let mut id_bytes = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut id_bytes);
        let version = SequenceNumber::from_u64(buf.get_u64());
        Ok(ObjectVersionKey {
            id: ObjectID::new(id_bytes),
            version,
        })
    }
}

/// `(parent ObjectID, field_id ObjectID)` — key for the
/// `dynamic_fields` CF. Fixed 64 bytes; a prefix scan on `parent`
/// walks all fields of that parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DynamicFieldKey {
    pub parent: ObjectID,
    pub field_id: ObjectID,
}

impl Encode for DynamicFieldKey {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.parent.as_ref());
        buf.put_slice(self.field_id.as_ref());
        Ok(())
    }
}

impl Decode for DynamicFieldKey {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        let expected = ObjectID::LENGTH * 2;
        if buf.remaining() != expected {
            return Err(DecodeError::msg(format!(
                "expected {expected} bytes for DynamicFieldKey, got {}",
                buf.remaining(),
            )));
        }
        let mut parent = [0u8; ObjectID::LENGTH];
        let mut field = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut parent);
        buf.copy_to_slice(&mut field);
        Ok(DynamicFieldKey {
            parent: ObjectID::new(parent),
            field_id: ObjectID::new(field),
        })
    }
}

/// `(original_package_id, version)` — key for the
/// `package_versions` CF. Fixed 40 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageVersionKey {
    pub original_id: ObjectID,
    pub version: u64,
}

impl Encode for PackageVersionKey {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.original_id.as_ref());
        buf.put_slice(&self.version.to_be_bytes());
        Ok(())
    }
}

impl Decode for PackageVersionKey {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() != ObjectID::LENGTH + 8 {
            return Err(DecodeError::msg(format!(
                "expected {} bytes for PackageVersionKey, got {}",
                ObjectID::LENGTH + 8,
                buf.remaining(),
            )));
        }
        let mut id = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut id);
        let version = buf.get_u64();
        Ok(PackageVersionKey {
            original_id: ObjectID::new(id),
            version,
        })
    }
}

/// `(SuiAddress, TypeTag)` — key for the `balance` and
/// `address_balance` CFs. A prefix scan on `owner` walks all of an
/// address's balances.
///
/// The `TypeTag` component is BCS-encoded. BCS isn't generally
/// lexicographically order-preserving for arbitrary types, but
/// `(owner, type)` keys are read via point lookup or
/// `owner`-prefix iteration; intra-owner sort order doesn't
/// matter for the current read patterns.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BalanceKey {
    pub owner: SuiAddress,
    pub coin_type: TypeTag,
}

impl Encode for BalanceKey {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.owner.as_ref());
        let type_bytes = bcs::to_bytes(&self.coin_type)
            .map_err(|e| EncodeError::with_source("bcs encode TypeTag", e))?;
        buf.put_slice(&type_bytes);
        Ok(())
    }
}

impl Decode for BalanceKey {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() < ObjectID::LENGTH {
            return Err(DecodeError::msg(format!(
                "expected at least {} bytes for BalanceKey owner, got {}",
                ObjectID::LENGTH,
                buf.remaining(),
            )));
        }
        let mut owner_bytes = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut owner_bytes);
        let owner = SuiAddress::from_bytes(owner_bytes)
            .map_err(|e| DecodeError::with_source("decode SuiAddress", e))?;
        let remaining = buf.copy_to_bytes(buf.remaining());
        let coin_type: TypeTag = bcs::from_bytes(&remaining)
            .map_err(|e| DecodeError::with_source("bcs decode TypeTag", e))?;
        Ok(BalanceKey { owner, coin_type })
    }
}

/// `StructTag` BCS-encoded — key for the `coin_index` CF. Lookups
/// are point reads, so BCS's lack of sort-preservation does not
/// matter here.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CoinTypeKey(pub StructTag);

impl Encode for CoinTypeKey {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        let bytes = bcs::to_bytes(&self.0)
            .map_err(|e| EncodeError::with_source("bcs encode StructTag", e))?;
        buf.put_slice(&bytes);
        Ok(())
    }
}

impl Decode for CoinTypeKey {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        let bytes = buf.copy_to_bytes(buf.remaining());
        let tag: StructTag = bcs::from_bytes(&bytes)
            .map_err(|e| DecodeError::with_source("bcs decode StructTag", e))?;
        Ok(CoinTypeKey(tag))
    }
}

/// Distinguishes the four kinds of ownership the `owner_index`
/// supports.
///
/// Encoded as a single byte at the front of `OwnerIndexKey`, so the
/// CF's iteration order groups all entries of one kind together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum OwnerKind {
    /// Owned by an address.
    Address = 0,
    /// Owned by another object (transferred via the parent's UID).
    Object = 1,
    /// Shared object.
    Shared = 2,
    /// Immutable.
    Immutable = 3,
}

impl OwnerKind {
    fn from_byte(b: u8) -> Result<Self, DecodeError> {
        match b {
            0 => Ok(OwnerKind::Address),
            1 => Ok(OwnerKind::Object),
            2 => Ok(OwnerKind::Shared),
            3 => Ok(OwnerKind::Immutable),
            other => Err(DecodeError::msg(format!(
                "unknown OwnerKind discriminant: {other}",
            ))),
        }
    }
}

/// Key for the `owner_index` CF.
///
/// Encoded as: `kind(1) || owner(32) || type(bcs) || inverted_balance(8 BE) || object_id(32)`.
///
/// The inverted balance is `u64::MAX - balance` so larger balances
/// sort *first* on disk — letting the most valuable coins page out
/// without a descending scan. Objects that have no balance (non-coin
/// objects) carry `inverted_balance = u64::MAX` (so they appear
/// last) — readers should treat that sentinel as "no balance" and
/// not subtract it back.
///
/// The `type` component is BCS-encoded; it sits between two
/// fixed-width fields so a prefix scan on
/// `kind || owner || type` is well-defined despite BCS's lack of
/// sort-preservation across distinct types — within a single
/// `(kind, owner, type)` group, the rest of the key sorts as
/// designed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OwnerIndexKey {
    pub kind: OwnerKind,
    pub owner: SuiAddress,
    pub type_: StructTag,
    pub inverted_balance: u64,
    pub object_id: ObjectID,
}

impl Encode for OwnerIndexKey {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_u8(self.kind as u8);
        buf.put_slice(self.owner.as_ref());
        let type_bytes = bcs::to_bytes(&self.type_)
            .map_err(|e| EncodeError::with_source("bcs encode StructTag", e))?;
        buf.put_slice(&type_bytes);
        buf.put_slice(&self.inverted_balance.to_be_bytes());
        buf.put_slice(self.object_id.as_ref());
        Ok(())
    }
}

impl Decode for OwnerIndexKey {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() < 1 + SUI_ADDRESS_LENGTH + 8 + ObjectID::LENGTH {
            return Err(DecodeError::msg(format!(
                "OwnerIndexKey too short: {} bytes",
                buf.remaining(),
            )));
        }
        let kind = OwnerKind::from_byte(buf.get_u8())?;
        let mut owner_bytes = [0u8; SUI_ADDRESS_LENGTH];
        buf.copy_to_slice(&mut owner_bytes);
        let owner = SuiAddress::from_bytes(owner_bytes)
            .map_err(|e| DecodeError::with_source("decode SuiAddress", e))?;
        // Read the BCS-encoded StructTag from the variable middle
        // section. We don't know its length up front; decode by
        // consuming from a slice and counting bytes used.
        let middle = buf.copy_to_bytes(buf.remaining() - 8 - ObjectID::LENGTH);
        let type_: StructTag = bcs::from_bytes(&middle)
            .map_err(|e| DecodeError::with_source("bcs decode StructTag", e))?;
        let inverted_balance = buf.get_u64();
        let mut id_bytes = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut id_bytes);
        Ok(OwnerIndexKey {
            kind,
            owner,
            type_,
            inverted_balance,
            object_id: ObjectID::new(id_bytes),
        })
    }
}

/// `(StructTag, ObjectID)` — key for the `type_index` CF. The
/// `StructTag` component is BCS-encoded; readers iterate by
/// (BCS-encoded) type prefix.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeIndexKey {
    pub type_: StructTag,
    pub object_id: ObjectID,
}

impl Encode for TypeIndexKey {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        let type_bytes = bcs::to_bytes(&self.type_)
            .map_err(|e| EncodeError::with_source("bcs encode StructTag", e))?;
        buf.put_slice(&type_bytes);
        buf.put_slice(self.object_id.as_ref());
        Ok(())
    }
}

impl Decode for TypeIndexKey {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() < ObjectID::LENGTH {
            return Err(DecodeError::msg(format!(
                "TypeIndexKey too short: {} bytes",
                buf.remaining(),
            )));
        }
        let prefix = buf.copy_to_bytes(buf.remaining() - ObjectID::LENGTH);
        let type_: StructTag = bcs::from_bytes(&prefix)
            .map_err(|e| DecodeError::with_source("bcs decode StructTag", e))?;
        let mut id = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut id);
        Ok(TypeIndexKey {
            type_,
            object_id: ObjectID::new(id),
        })
    }
}

/// Bitmap inverted-index key: `(dimension_key, bucket)`.
///
/// `dimension_key` is the indexed-field token (e.g. `[tag][sender]`,
/// `[tag][module][function]`); buckets group fixed-size ranges of
/// the tx_seq or packed-event-seq space. The dimension key is a
/// variable-length opaque byte string assembled by the caller; the
/// bucket is a u64 big-endian appended at the end so bucket scans
/// within one dimension walk in numerical order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BitmapIndexKey {
    pub dimension_key: Vec<u8>,
    pub bucket: u64,
}

impl Encode for BitmapIndexKey {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(&self.dimension_key);
        buf.put_slice(&self.bucket.to_be_bytes());
        Ok(())
    }
}

impl Decode for BitmapIndexKey {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() < 8 {
            return Err(DecodeError::msg(format!(
                "BitmapIndexKey too short: {} bytes",
                buf.remaining(),
            )));
        }
        let dim_len = buf.remaining() - 8;
        let dim_bytes = buf.copy_to_bytes(dim_len);
        let bucket = buf.get_u64();
        Ok(BitmapIndexKey {
            dimension_key: dim_bytes.to_vec(),
            bucket,
        })
    }
}

/// Singleton key for the `pruning_watermark` CF. Encodes as zero
/// bytes — there's only ever one row.
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
