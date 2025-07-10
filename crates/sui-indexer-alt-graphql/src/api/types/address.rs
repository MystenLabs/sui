// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Interface, Object};
use sui_types::base_types::SuiAddress as NativeSuiAddress;

use crate::{api::scalars::sui_address::SuiAddress, scope::Scope};

use super::{move_package::MovePackage, object::Object};

/// Interface implemented by GraphQL types representing entities that are identified by an address.
///
/// An address uniquely represents either the public key of an account, or an object's ID, but never both. It is not possible to determine which type an address represents up-front. If an object is wrapped, its contents will not be accessible via its address, but it will still be possible to access other objects it owns.
#[derive(Interface)]
#[graphql(name = "IAddressable", field(name = "address", ty = "SuiAddress"))]
pub(crate) enum IAddressable {
    Address(Address),
    MovePackage(MovePackage),
    Object(Object),
}

#[derive(Clone)]
pub(crate) struct Address {
    pub(crate) scope: Scope,
    pub(crate) address: NativeSuiAddress,
}

pub(crate) struct AddressableImpl<'a>(&'a Address);

#[Object]
impl Address {
    /// The Address' identifier, a 32-byte number represented as a 64-character hex string, with a lead "0x".
    pub(crate) async fn address(&self) -> SuiAddress {
        AddressableImpl::from(self).address()
    }
}

impl Address {
    /// Construct an address that is represented by just its identifier (`SuiAddress`).
    /// This does not check whether the address is valid or exists in the system.
    pub(crate) fn with_address(scope: Scope, address: NativeSuiAddress) -> Self {
        Self { scope, address }
    }
}

impl AddressableImpl<'_> {
    pub(crate) fn address(&self) -> SuiAddress {
        self.0.address.into()
    }
}

impl<'a> From<&'a Address> for AddressableImpl<'a> {
    fn from(address: &'a Address) -> Self {
        AddressableImpl(address)
    }
}
