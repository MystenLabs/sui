// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    connection::{Connection, CursorType, Edge},
    *,
};
use sui_types::{
    digests::TransactionDigest,
    object::Object as NativeObject,
    transaction::{GenesisObject, GenesisTransaction as NativeGenesisTransaction},
};

use crate::types::{
    cursor::{Cursor, Page},
    object::Object,
    sui_address::SuiAddress,
};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct GenesisTransaction(pub NativeGenesisTransaction);

pub(crate) type CObject = Cursor<usize>;

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
    ) -> Result<Connection<String, Object>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;

        let mut connection = Connection::new(false, false);
        let Some((prev, next, cs)) = page.paginate_indices(self.0.objects.len()) else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in cs {
            let GenesisObject::RawObject { data, owner } = self.0.objects[*c].clone();
            let native =
                NativeObject::new_from_genesis(data, owner, TransactionDigest::genesis_marker());

            let object = Object::from_native(SuiAddress::from(native.id()), native);
            connection.edges.push(Edge::new(c.encode_cursor(), object));
        }

        Ok(connection)
    }
}
