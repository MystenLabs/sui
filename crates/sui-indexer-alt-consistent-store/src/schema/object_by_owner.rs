// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bincode::{
    de::{read::Reader, Decoder},
    enc::{write::Writer, Encoder},
    error::{AllowedEnumVariants, DecodeError, EncodeError},
    serde::{BorrowCompat, Compat},
    Decode, Encode,
};
use move_core_types::language_storage::StructTag;
use sui_indexer_alt_framework::types::{
    base_types::{ObjectID, SuiAddress},
    object::{Object, Owner},
};

/// Key for the index that supports fetching an owner's objects, optionally filtering by object
/// type.
#[derive(Encode, Decode, PartialEq, Eq)]
pub(crate) struct Key {
    pub(crate) kind: OwnerKind,

    /// `None` if the object is a MovePackage, `Some` and the object's type if it is a MoveObject.
    #[bincode(with_serde)]
    pub(crate) type_: Option<StructTag>,

    /// If the object is coin-like (has a balance), this field stores the bitwise negation (one's
    /// complement) of the balance. This ensures coin-like objects are ordered in descending order
    /// of balance.
    pub(crate) balance: Option<u64>,

    /// The ID of the object.
    #[bincode(with_serde)]
    pub(crate) object_id: ObjectID,
}

#[derive(PartialEq, Eq, Debug)]
pub(crate) enum OwnerKind {
    /// Both AddressOwner and ConsensusAddressOwner map to this OwnerKind.
    AddressOwner(SuiAddress),
    ObjectOwner(SuiAddress),
    Shared,
    Immutable,
}

impl Key {
    pub(crate) fn from_object(obj: &Object) -> Key {
        Key {
            kind: OwnerKind::from_owner(obj.owner()),
            type_: obj.type_().map(|t| t.clone().into()),
            balance: obj.as_coin_maybe().map(|coin| !coin.balance.value()),
            object_id: obj.id(),
        }
    }
}

impl OwnerKind {
    pub(crate) fn from_owner(owner: &Owner) -> Self {
        match owner {
            Owner::AddressOwner(address) => OwnerKind::AddressOwner(*address),
            Owner::ObjectOwner(address) => OwnerKind::ObjectOwner(*address),
            Owner::Shared { .. } => OwnerKind::Shared,
            Owner::Immutable => OwnerKind::Immutable,
            Owner::ConsensusAddressOwner { owner, .. } => OwnerKind::AddressOwner(*owner),
        }
    }
}

bincode::impl_borrow_decode!(OwnerKind);

impl Encode for OwnerKind {
    fn encode<E: Encoder>(&self, e: &mut E) -> Result<(), EncodeError> {
        let w = e.writer();
        match self {
            OwnerKind::AddressOwner(address) => {
                w.write(&[0])?;
                BorrowCompat(address).encode(e)
            }
            OwnerKind::ObjectOwner(address) => {
                w.write(&[1])?;
                BorrowCompat(address).encode(e)
            }
            OwnerKind::Shared => w.write(&[2]),
            OwnerKind::Immutable => w.write(&[3]),
        }
    }
}

impl<C> Decode<C> for OwnerKind {
    fn decode<D: Decoder>(d: &mut D) -> Result<Self, DecodeError> {
        let r = d.reader();

        let mut kind = [0u8; 1];
        r.read(&mut kind)?;
        match kind[0] {
            0 => {
                let address = Compat::<SuiAddress>::decode(d)?.0;
                Ok(OwnerKind::AddressOwner(address))
            }
            1 => {
                let address = Compat::<SuiAddress>::decode(d)?.0;
                Ok(OwnerKind::ObjectOwner(address))
            }
            2 => Ok(OwnerKind::Shared),
            3 => Ok(OwnerKind::Immutable),
            v => Err(DecodeError::UnexpectedVariant {
                type_name: "OwnerKind",
                allowed: &AllowedEnumVariants::Range { min: 0, max: 3 },
                found: v as u32,
            }),
        }
    }
}

/// Options for creating this index's column family in RocksDB.
pub(crate) fn options() -> rocksdb::Options {
    rocksdb::Options::default()
}

#[cfg(test)]
mod tests {
    use crate::db::key;

    use super::*;

    #[test]
    fn test_owner_kind_roundtrip() {
        let address = OwnerKind::AddressOwner(SuiAddress::random_for_testing_only());
        assert_eq!(address, key::decode(&key::encode(&address)).unwrap());

        let object = OwnerKind::ObjectOwner(SuiAddress::random_for_testing_only());
        assert_eq!(object, key::decode(&key::encode(&object)).unwrap());

        let shared = OwnerKind::Shared;
        assert_eq!(shared, key::decode(&key::encode(&shared)).unwrap());

        let immutable = OwnerKind::Immutable;
        assert_eq!(immutable, key::decode(&key::encode(&immutable)).unwrap());
    }
}
