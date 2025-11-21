// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Context, Object, connection::Connection};
use sui_types::transaction::GenesisTransaction as NativeGenesisTransaction;

use crate::{
    api::{scalars::cursor::JsonCursor, types::object::Object},
    error::RpcError,
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

type CObject = JsonCursor<usize>;

#[derive(Clone)]
pub(crate) struct GenesisTransaction {
    pub(crate) native: NativeGenesisTransaction,
    pub(crate) scope: Scope,
}

/// System transaction that initializes the network and writes the initial set of objects on-chain.
#[Object]
impl GenesisTransaction {
    /// Objects to be created during genesis.
    async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CObject>,
        last: Option<u64>,
        before: Option<CObject>,
    ) -> Result<Option<Connection<String, Object>>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("GenesisTransaction", "objects");
        let page = Page::from_params(limits, first, after, last, before)?;

        let objects = &self.native.objects;
        page.paginate_indices(objects.len(), |i| {
            Ok(Object::from_genesis_object(
                self.scope.clone(),
                objects[i].clone(),
            ))
        })
        .map(Some)
    }
}
