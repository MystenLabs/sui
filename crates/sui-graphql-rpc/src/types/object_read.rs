// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use sui_types::base_types::ObjectRef as NativeObjectRef;

use crate::context_data::db_data_provider::PgManager;

use super::{object::Object, sui_address::SuiAddress};

// A helper type representing the read of a specific version of an object. Intended to be
// "flattened" into other GraphQL types.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ObjectRead(pub NativeObjectRef);

#[Object]
impl ObjectRead {
    /// ID of the object being read.
    async fn address(&self) -> SuiAddress {
        self.address_impl()
    }

    /// Version of the object being read.
    async fn version(&self) -> u64 {
        self.version_impl()
    }

    /// 32-byte hash that identifies the object's contents at this version, encoded as a Base58
    /// string.
    async fn digest(&self) -> String {
        self.0 .2.base58_encode()
    }

    /// The object at this version.  May not be available due to pruning.
    async fn object(&self, ctx: &Context<'_>) -> Result<Option<Object>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_obj(self.address_impl(), Some(self.version_impl()))
            .await
            .extend()
    }
}

impl ObjectRead {
    fn address_impl(&self) -> SuiAddress {
        SuiAddress::from(self.0 .0)
    }

    fn version_impl(&self) -> u64 {
        self.0 .1.value()
    }
}
