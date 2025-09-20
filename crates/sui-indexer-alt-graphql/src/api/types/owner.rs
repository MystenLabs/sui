// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{SimpleObject, Union};
use sui_types::object::Owner as NativeOwner;

use crate::{api::scalars::uint53::UInt53, scope::Scope};

use super::address::Address;

/// The object's owner kind.
#[derive(Union, Clone)]
pub(crate) enum Owner {
    Address(AddressOwner),
    Object(ObjectOwner),
    Shared(Shared),
    Immutable(Immutable),
    ConsensusAddress(ConsensusAddressOwner),
}

/// Object is exclusively owned by a single address, and is mutable.
#[derive(SimpleObject, Clone)]
pub(crate) struct AddressOwner {
    /// The owner's address.
    address: Option<Address>,
}

/// Object is exclusively owned by a single object, and is mutable. Note that the owning object may be inaccessible because it is wrapped.
#[derive(SimpleObject, Clone)]
pub(crate) struct ObjectOwner {
    /// The owner's address.
    address: Option<Address>,
}

/// Object is shared, can be used by any address, and is mutable.
#[derive(SimpleObject, Clone)]
pub(crate) struct Shared {
    /// The version at which the object became shared.
    initial_shared_version: Option<UInt53>,
}

/// Object is accessible to all addresses, and is immutable.
#[derive(SimpleObject, Clone)]
pub(crate) struct Immutable {
    #[graphql(name = "_")]
    dummy: Option<bool>,
}

/// Object is exclusively owned by a single adderss and sequenced via consensus.
#[derive(SimpleObject, Clone)]
pub(crate) struct ConsensusAddressOwner {
    /// The version at which the object most recently bcame a consensus object. This serves the same function as `Shared.initialSharedVersion`, except it may change if the object's `owner` type changes.
    start_version: Option<UInt53>,

    /// The owner's address.
    address: Option<Address>,
}

impl Owner {
    pub(crate) fn from_native(scope: Scope, native: NativeOwner) -> Self {
        use NativeOwner as NO;
        use Owner as O;

        match native {
            NO::AddressOwner(a) => O::Address(AddressOwner {
                address: Some(Address::with_address(scope.without_root_version(), a)),
            }),

            NO::ObjectOwner(a) => O::Object(ObjectOwner {
                address: Some(Address::with_address(scope, a)),
            }),

            NO::Shared {
                initial_shared_version,
            } => O::Shared(Shared {
                initial_shared_version: Some(initial_shared_version.into()),
            }),

            NO::Immutable => O::Immutable(Immutable { dummy: None }),

            NO::ConsensusAddressOwner {
                start_version,
                owner,
            } => O::ConsensusAddress(ConsensusAddressOwner {
                start_version: Some(start_version.into()),
                address: Some(Address::with_address(scope.without_root_version(), owner)),
            }),
        }
    }
}
