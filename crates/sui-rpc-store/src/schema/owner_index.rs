// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `OwnerIndexKey` → latest live `version`.
//!
//! Supports owner-and-type filtering with optional balance-based
//! ordering. The leading [`OwnerKind`] byte clusters entries by
//! ownership category: address-owned, object-owned, shared,
//! immutable. The address variants carry the owning address in the
//! key; shared and immutable do not. Within each
//! `(kind, owner, type)` group, coin-like objects carry an
//! inverted balance that sorts richest-first, and non-coin objects
//! carry no balance at all.

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

/// The four kinds of ownership this index distinguishes. The
/// address-owner and object-owner variants carry the owning
/// `SuiAddress` inline; shared and immutable owners have no
/// owning address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OwnerKind {
    AddressOwner(SuiAddress),
    ObjectOwner(SuiAddress),
    Shared,
    Immutable,
}

/// Encoded as
/// `kind_tag(1) || owner?(32) || type(bcs) || balance_tag(1) || balance?(8 BE) || object_id(32)`.
///
/// `kind_tag` distinguishes the four owner kinds (`0` =
/// AddressOwner, `1` = ObjectOwner, `2` = Shared, `3` = Immutable).
/// The 32-byte owning address follows only for the two owner-kind
/// variants. `balance_tag` is `0` for non-coin rows (no balance
/// follows) and `1` for coin-like rows (followed by an 8-byte
/// big-endian inverted balance — i.e. `u64::MAX - balance`, so
/// richer coins sort first).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key {
    pub kind: OwnerKind,
    pub type_: StructTag,
    pub inverted_balance: Option<u64>,
    pub object_id: ObjectID,
}

pub type Value = U64Varint;

impl Encode for Key {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        match &self.kind {
            OwnerKind::AddressOwner(addr) => {
                buf.put_u8(0);
                buf.put_slice(addr.as_ref());
            }
            OwnerKind::ObjectOwner(addr) => {
                buf.put_u8(1);
                buf.put_slice(addr.as_ref());
            }
            OwnerKind::Shared => {
                buf.put_u8(2);
            }
            OwnerKind::Immutable => {
                buf.put_u8(3);
            }
        }
        let type_bytes = bcs::to_bytes(&self.type_)
            .map_err(|e| EncodeError::with_source("bcs encode StructTag", e))?;
        buf.put_slice(&type_bytes);
        match self.inverted_balance {
            None => buf.put_u8(0),
            Some(b) => {
                buf.put_u8(1);
                buf.put_slice(&b.to_be_bytes());
            }
        }
        buf.put_slice(self.object_id.as_ref());
        Ok(())
    }
}

impl Decode for Key {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if !buf.has_remaining() {
            return Err(DecodeError::msg(format!("{NAME} Key empty")));
        }
        let kind = match buf.get_u8() {
            0 => OwnerKind::AddressOwner(read_address(buf)?),
            1 => OwnerKind::ObjectOwner(read_address(buf)?),
            2 => OwnerKind::Shared,
            3 => OwnerKind::Immutable,
            v => {
                return Err(DecodeError::msg(format!(
                    "{NAME} unknown OwnerKind tag: {v}",
                )));
            }
        };

        // Consume one `StructTag`'s worth of bytes via the
        // streaming BCS parser. The parser stops at the StructTag's
        // natural end and leaves the rest of the buffer (balance
        // tag, balance payload, object id) intact.
        let type_ = crate::schema::keys::read_struct_tag(buf)?;

        if !buf.has_remaining() {
            return Err(DecodeError::msg(format!("{NAME} missing balance tag")));
        }
        let inverted_balance = match buf.get_u8() {
            0 => None,
            1 => {
                if buf.remaining() < 8 {
                    return Err(DecodeError::msg(format!("{NAME} missing balance payload",)));
                }
                Some(buf.get_u64())
            }
            v => {
                return Err(DecodeError::msg(
                    format!("{NAME} invalid balance tag: {v}",),
                ));
            }
        };

        if buf.remaining() != ObjectID::LENGTH {
            return Err(DecodeError::msg(format!(
                "{NAME} expected {} trailing bytes for object_id, got {}",
                ObjectID::LENGTH,
                buf.remaining(),
            )));
        }
        let mut id = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut id);

        Ok(Key {
            kind,
            type_,
            inverted_balance,
            object_id: ObjectID::new(id),
        })
    }
}

fn read_address<B: Buf>(buf: &mut B) -> Result<SuiAddress, DecodeError> {
    if buf.remaining() < SUI_ADDRESS_LENGTH {
        return Err(DecodeError::msg(format!(
            "{NAME} missing owner address: {} bytes left",
            buf.remaining(),
        )));
    }
    let mut bytes = [0u8; SUI_ADDRESS_LENGTH];
    buf.copy_to_slice(&mut bytes);
    SuiAddress::from_bytes(bytes).map_err(|e| DecodeError::with_source("decode SuiAddress", e))
}

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sui_tag() -> StructTag {
        StructTag {
            address: move_core_types::account_address::AccountAddress::new([2u8; 32]),
            module: move_core_types::identifier::Identifier::new("sui").unwrap(),
            name: move_core_types::identifier::Identifier::new("SUI").unwrap(),
            type_params: vec![],
        }
    }

    fn round_trip(key: Key) {
        let bytes = key.encode().unwrap();
        let decoded = Key::decode(&mut &bytes[..]).unwrap();
        assert_eq!(key, decoded);
    }

    #[test]
    fn address_owner_with_balance_round_trips() {
        round_trip(Key {
            kind: OwnerKind::AddressOwner(SuiAddress::from_bytes([1u8; 32]).unwrap()),
            type_: sui_tag(),
            inverted_balance: Some(!1_000_000u64),
            object_id: ObjectID::new([7u8; 32]),
        });
    }

    #[test]
    fn object_owner_without_balance_round_trips() {
        round_trip(Key {
            kind: OwnerKind::ObjectOwner(SuiAddress::from_bytes([3u8; 32]).unwrap()),
            type_: sui_tag(),
            inverted_balance: None,
            object_id: ObjectID::new([8u8; 32]),
        });
    }

    #[test]
    fn shared_round_trips() {
        round_trip(Key {
            kind: OwnerKind::Shared,
            type_: sui_tag(),
            inverted_balance: None,
            object_id: ObjectID::new([9u8; 32]),
        });
    }

    #[test]
    fn immutable_round_trips() {
        round_trip(Key {
            kind: OwnerKind::Immutable,
            type_: sui_tag(),
            inverted_balance: None,
            object_id: ObjectID::new([0xAAu8; 32]),
        });
    }
}
