// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `OwnerIndexKey` → `VersionDigest`.
//!
//! Supports owner-and-type filtering with optional balance-based
//! ordering. The leading [`OwnerKind`] byte clusters entries by
//! ownership category (address-owned, object-owned, shared,
//! immutable); within each group a prefix scan walks
//! `owner → type → inverted_balance → object_id`.

use bytes::Buf;
use bytes::BufMut;
use move_core_types::language_storage::StructTag;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SUI_ADDRESS_LENGTH;
use sui_types::base_types::SuiAddress;

use crate::schema::keys::U64Varint;

pub const NAME: &str = "owner_index";

/// The four kinds of ownership this index distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum OwnerKind {
    /// Owned by an address.
    Address = 0,
    /// Owned by another object.
    Object = 1,
    /// Shared object.
    Shared = 2,
    /// Immutable.
    Immutable = 3,
}

/// Encoded as `kind(1) || owner(32) || type(bcs) || inverted_balance(8 BE) || object_id(32)`.
///
/// `inverted_balance` is `u64::MAX - balance` so larger balances
/// sort *first*, letting the most valuable coins page out without a
/// descending scan. Non-coin objects use `u64::MAX` (which sorts
/// last); readers treat that as "no balance" rather than decoding
/// the sentinel back to zero.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key {
    pub kind: OwnerKind,
    pub owner: SuiAddress,
    pub type_: StructTag,
    pub inverted_balance: u64,
    pub object_id: ObjectID,
}

pub type Value = U64Varint;

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

impl Encode for Key {
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

impl Decode for Key {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() < 1 + SUI_ADDRESS_LENGTH + 8 + ObjectID::LENGTH {
            return Err(DecodeError::msg(format!(
                "{NAME} Key too short: {} bytes",
                buf.remaining(),
            )));
        }
        let kind = OwnerKind::from_byte(buf.get_u8())?;
        let mut owner_bytes = [0u8; SUI_ADDRESS_LENGTH];
        buf.copy_to_slice(&mut owner_bytes);
        let owner = SuiAddress::from_bytes(owner_bytes)
            .map_err(|e| DecodeError::with_source("decode SuiAddress", e))?;
        let middle = buf.copy_to_bytes(buf.remaining() - 8 - ObjectID::LENGTH);
        let type_: StructTag = bcs::from_bytes(&middle)
            .map_err(|e| DecodeError::with_source("bcs decode StructTag", e))?;
        let inverted_balance = buf.get_u64();
        let mut id_bytes = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut id_bytes);
        Ok(Key {
            kind,
            owner,
            type_,
            inverted_balance,
            object_id: ObjectID::new(id_bytes),
        })
    }
}

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
