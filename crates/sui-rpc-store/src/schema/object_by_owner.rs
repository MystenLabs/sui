// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `(OwnerKind, type, inverted_balance?, ObjectID)` → latest live
//! `version`.
//!
//! Supports owner-and-type filtering with optional balance-based
//! ordering. The leading [`OwnerKind`] byte clusters entries by
//! ownership category: address-owned, object-owned, shared,
//! immutable. The address variants carry the owning address in the
//! key; shared and immutable do not. Within each
//! `(kind, owner, type)` group, coin-like objects carry the
//! ones-complement of their balance (`!balance`) so richer coins
//! sort first under a forward prefix scan; non-coin objects carry
//! no balance at all.

use bytes::Buf;
use bytes::BufMut;
use move_core_types::language_storage::StructTag;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::Iter;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SUI_ADDRESS_LENGTH;
use sui_types::base_types::SuiAddress;
use sui_types::object::Object;
use sui_types::object::Owner;

use crate::schema::keys::U64Varint;

pub const NAME: &str = "object_by_owner";

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

impl OwnerKind {
    /// Project a canonical [`Owner`] onto this index's
    /// [`OwnerKind`] shape.
    ///
    /// - `ConsensusAddressOwner` collapses into `AddressOwner` so
    ///   address-based listings return objects regardless of
    ///   whether they sit on the consensus path.
    /// - `Party` is intentionally unimplemented: the canonical
    ///   type is still in flux upstream.
    pub fn from_owner(owner: &Owner) -> Self {
        match owner {
            Owner::AddressOwner(address) => OwnerKind::AddressOwner(*address),
            Owner::ObjectOwner(address) => OwnerKind::ObjectOwner(*address),
            Owner::Shared { .. } => OwnerKind::Shared,
            Owner::Immutable => OwnerKind::Immutable,
            Owner::ConsensusAddressOwner { owner, .. } => OwnerKind::AddressOwner(*owner),
            Owner::Party { .. } => todo!("Party owner WIP"),
        }
    }
}

/// Encoded as
/// `kind_tag(1) || owner?(32) || type(bcs) || balance_tag(1) || balance?(8 BE) || object_id(32)`.
///
/// `kind_tag` distinguishes the four owner kinds (`0` =
/// AddressOwner, `1` = ObjectOwner, `2` = Shared, `3` = Immutable).
/// The 32-byte owning address follows only for the two owner-kind
/// variants. `balance_tag` is `0` for non-coin rows (no balance
/// follows) and `1` for coin-like rows (followed by 8 big-endian
/// bytes of the ones-complement of the coin's balance —
/// `!balance`, so richer coins sort first under a forward prefix
/// scan).
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

/// Build the `(Key, Value)` pair indexing a Move object by owner.
///
/// Returns `None` for objects that aren't Move objects (packages,
/// for example) — those have no `StructTag` and aren't part of
/// this index. For coin-like Move objects the balance is captured
/// as the ones-complement `!balance` so richer coins sort first
/// within their `(owner, type)` group.
pub fn store(object: &Object) -> Option<(Key, U64Varint)> {
    let type_: StructTag = object.type_()?.clone().into();
    Some((
        Key {
            kind: OwnerKind::from_owner(object.owner()),
            type_,
            inverted_balance: object.as_coin_maybe().map(|coin| !coin.balance.value()),
            object_id: object.id(),
        },
        U64Varint(object.version().value()),
    ))
}

/// Prefix encoder for "all address-owned objects of `owner`".
///
/// Encodes as `kind_tag(1) || owner(32)` — exactly the first 33
/// bytes of every `Key` whose `kind` is `AddressOwner(owner)`. The
/// schema's `Encode` impl matches this layout, so passing this
/// type to [`DbMap::iter_prefix`](sui_consistent_store::DbMap::iter_prefix)
/// walks every row owned by the given address.
pub struct AddressOwnerPrefix(pub SuiAddress);

impl Encode for AddressOwnerPrefix {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_u8(0);
        buf.put_slice(self.0.as_ref());
        Ok(())
    }
}

/// Prefix encoder for "all objects owned by parent object `owner`".
///
/// Same layout as [`AddressOwnerPrefix`] but with the
/// `ObjectOwner` discriminant — i.e. the leading 33 bytes of every
/// `Key` whose `kind` is `ObjectOwner(owner)`. Useful for
/// enumerating dynamic fields and other object-owned children of
/// a parent.
pub struct ObjectOwnerPrefix(pub SuiAddress);

impl Encode for ObjectOwnerPrefix {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_u8(1);
        buf.put_slice(self.0.as_ref());
        Ok(())
    }
}

/// Prefix encoder for "all objects owned by `owner` that match
/// `type_filter`". Composes [`AddressOwnerPrefix`] with a
/// [`TypeFilter`](super::type_filter::TypeFilter).
pub struct AddressOwnerTypePrefix<'a> {
    pub owner: SuiAddress,
    pub type_filter: &'a super::type_filter::TypeFilter,
}

impl Encode for AddressOwnerTypePrefix<'_> {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_u8(0);
        buf.put_slice(self.owner.as_ref());
        self.type_filter.encode_into(buf)
    }
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Iterate over every live object owned (in the address-owner
    /// sense) by `owner`, in the natural sort order of the index:
    /// by Move type, then within each type by descending balance
    /// for coin-like objects, then by object id.
    pub fn iter_objects_owned_by_address(
        &self,
        owner: SuiAddress,
    ) -> Result<Iter<'_, Key, U64Varint>, Error> {
        self.object_by_owner.iter_prefix(&AddressOwnerPrefix(owner))
    }

    /// Iterate over every live object owned (in the address-owner
    /// sense) by `owner` whose Move type matches `type_filter`.
    /// See [`type_filter::TypeFilter`](super::type_filter::TypeFilter)
    /// for the matching contract.
    pub fn iter_objects_owned_by_address_of_type<'a>(
        &'a self,
        owner: SuiAddress,
        type_filter: &'a super::type_filter::TypeFilter,
    ) -> Result<Iter<'a, Key, U64Varint>, Error> {
        self.object_by_owner.iter_prefix(&AddressOwnerTypePrefix {
            owner,
            type_filter,
        })
    }

    /// Iterate over every live object owned (in the object-owner
    /// sense) by the parent object at the given id.
    pub fn iter_objects_owned_by_object(
        &self,
        parent: SuiAddress,
    ) -> Result<Iter<'_, Key, U64Varint>, Error> {
        self.object_by_owner.iter_prefix(&ObjectOwnerPrefix(parent))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;

    use super::*;
    use crate::RpcStoreSchema;

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

    fn fresh_db() -> (tempfile::TempDir, sui_consistent_store::Db, RpcStoreSchema) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    fn dummy_object(id: ObjectID, owner: SuiAddress) -> Object {
        Object::with_id_owner_for_testing(id, owner)
    }

    #[test]
    fn store_derives_key_from_object() {
        let owner = SuiAddress::from_bytes([5u8; 32]).unwrap();
        let id = ObjectID::random();
        let object = dummy_object(id, owner);
        let (key, value) = store(&object).expect("Move object");

        assert_eq!(key.kind, OwnerKind::AddressOwner(owner));
        assert_eq!(key.object_id, id);
        assert_eq!(value.0, object.version().value());
        // Gas-coin test objects carry a balance, so the inverted
        // balance should be populated.
        assert!(key.inverted_balance.is_some());
    }

    #[test]
    fn iter_returns_empty_for_owner_with_no_objects() {
        let (_dir, _db, schema) = fresh_db();
        let owner = SuiAddress::from_bytes([1u8; 32]).unwrap();
        let count = schema.iter_objects_owned_by_address(owner).unwrap().count();
        assert_eq!(count, 0);
    }

    #[test]
    fn iter_with_type_filter_narrows_to_matching_objects() {
        // All gas-coin test objects share the same Move type
        // (`0x2::coin::Coin<0x2::sui::SUI>`). A `TypeFilter::Type`
        // pointing at that type should return every one of them;
        // a `TypeFilter::Type` at a different type should return
        // none.
        let (_dir, db, schema) = fresh_db();
        let owner = SuiAddress::from_bytes([1u8; 32]).unwrap();

        let mut expected_ids = BTreeSet::new();
        let mut batch = db.batch();
        let mut shared_type = None;
        for _ in 0..3 {
            let id = ObjectID::random();
            expected_ids.insert(id);
            let (k, v) = store(&dummy_object(id, owner)).unwrap();
            shared_type.get_or_insert(k.type_.clone());
            batch.put(&schema.object_by_owner, &k, &v).unwrap();
        }
        batch.commit().unwrap();

        let shared_type = shared_type.unwrap();
        let matching_filter = super::super::type_filter::TypeFilter::Type(shared_type.clone());
        let found: BTreeSet<ObjectID> = schema
            .iter_objects_owned_by_address_of_type(owner, &matching_filter)
            .unwrap()
            .map(|res| res.unwrap().0.object_id)
            .collect();
        assert_eq!(found, expected_ids);

        let mismatched_filter = super::super::type_filter::TypeFilter::Type(StructTag {
            name: move_core_types::identifier::Identifier::new("Other").unwrap(),
            ..shared_type
        });
        let mismatched_count = schema
            .iter_objects_owned_by_address_of_type(owner, &mismatched_filter)
            .unwrap()
            .count();
        assert_eq!(mismatched_count, 0);
    }

    #[test]
    fn iter_finds_only_objects_for_target_owner() {
        let (_dir, db, schema) = fresh_db();
        let target = SuiAddress::from_bytes([1u8; 32]).unwrap();
        let other = SuiAddress::from_bytes([2u8; 32]).unwrap();

        let mut target_ids = BTreeSet::new();
        let mut batch = db.batch();
        for _ in 0..3 {
            let id = ObjectID::random();
            target_ids.insert(id);
            let (k, v) = store(&dummy_object(id, target)).unwrap();
            batch.put(&schema.object_by_owner, &k, &v).unwrap();
        }
        for _ in 0..2 {
            let id = ObjectID::random();
            let (k, v) = store(&dummy_object(id, other)).unwrap();
            batch.put(&schema.object_by_owner, &k, &v).unwrap();
        }
        batch.commit().unwrap();

        let found: BTreeSet<ObjectID> = schema
            .iter_objects_owned_by_address(target)
            .unwrap()
            .map(|res| res.unwrap().0.object_id)
            .collect();
        assert_eq!(found, target_ids);
    }
}
