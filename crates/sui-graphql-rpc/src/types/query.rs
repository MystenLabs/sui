// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use super::{address::Address, object::Object, owner::Owner, sui_address::SuiAddress};

pub(crate) struct Query;

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl Query {
    async fn chain_identifier(&self) -> String {
        unimplemented!()
    }

    async fn owner(&self, address: SuiAddress) -> Option<Owner> {
        unimplemented!()
    }

    async fn object(&self, address: SuiAddress, version: Option<u64>) -> Option<Object> {
        unimplemented!()
    }

    async fn address(&self, address: SuiAddress) -> Option<Address> {
        unimplemented!()
    }
}
