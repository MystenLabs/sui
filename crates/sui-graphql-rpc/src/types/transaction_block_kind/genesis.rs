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

        let total = self.0.objects.len();
        let mut lo = page.after().map_or(0, |a| *a + 1);
        let mut hi = page.before().map_or(total, |b| *b);

        let mut connection = Connection::new(false, false);
        if hi <= lo {
            return Ok(connection);
        } else if (hi - lo) > page.limit() {
            if page.is_from_front() {
                hi = lo + page.limit();
            } else {
                lo = hi - page.limit();
            }
        }

        connection.has_previous_page = 0 < lo;
        connection.has_next_page = hi < total;

        for idx in lo..hi {
            let GenesisObject::RawObject { data, owner } = self.0.objects[idx].clone();
            let native =
                NativeObject::new_from_genesis(data, owner, TransactionDigest::genesis_marker());

            let cursor = Cursor::new(idx).encode_cursor();
            let object = Object::from_native(SuiAddress::from(native.id()), native);
            connection.edges.push(Edge::new(cursor, object));
        }

        Ok(connection)
    }
}
