// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;

use crate::api::scalars::sui_address::SuiAddress;

use super::addressable::{Addressable, AddressableImpl};

pub(crate) struct Address {
    pub(crate) super_: Addressable,
}

#[Object]
impl Address {
    /// The Address' identifier.
    pub(crate) async fn address(&self) -> SuiAddress {
        AddressableImpl::from(&self.super_).address()
    }
}

impl Address {
    /// Construct an address that is represented by just its identifier (`SuiAddress`).
    /// This does not check whether the address is valid or exists in the system.
    pub(crate) fn new(addressable: Addressable) -> Self {
        Self {
            super_: addressable,
        }
    }
}
