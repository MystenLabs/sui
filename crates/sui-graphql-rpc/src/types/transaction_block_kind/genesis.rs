// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    connection::{Connection, Edge},
    *,
};
use sui_types::{
    digests::TransactionDigest,
    object::Object as NativeObject,
    transaction::{GenesisObject, GenesisTransaction as NativeGenesisTransaction},
};

use crate::{
    context_data::db_data_provider::validate_cursor_pagination,
    error::Error,
    types::{object::Object, sui_address::SuiAddress},
};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct GenesisTransaction(pub NativeGenesisTransaction);

#[Object]
impl GenesisTransaction {
    /// Objects to be created during genesis.
    async fn object_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Connection<String, Object>> {
        // TODO: make cursor opaque (currently just an offset).
        validate_cursor_pagination(&first, &after, &last, &before).extend()?;

        let total = self.0.objects.len();

        let mut lo = if let Some(after) = after {
            1 + after
                .parse::<usize>()
                .map_err(|_| Error::InvalidCursor("Failed to parse 'after' cursor.".to_string()))
                .extend()?
        } else {
            0
        };

        let mut hi = if let Some(before) = before {
            before
                .parse::<usize>()
                .map_err(|_| Error::InvalidCursor("Failed to parse 'before' cursor.".to_string()))
                .extend()?
        } else {
            total
        };

        let mut connection = Connection::new(false, false);
        if hi <= lo {
            return Ok(connection);
        }

        // If there's a `first` limit, bound the upperbound to be at most `first` away from the
        // lowerbound.
        if let Some(first) = first {
            let first = first as usize;
            if hi - lo > first {
                hi = lo + first;
            }
        }

        // If there's a `last` limit, bound the lowerbound to be at most `last` away from the
        // upperbound.  NB. This applies after we bounded the upperbound, using `first`.
        if let Some(last) = last {
            let last = last as usize;
            if hi - lo > last {
                lo = hi - last;
            }
        }

        connection.has_previous_page = 0 < lo;
        connection.has_next_page = hi < total;

        for (idx, object) in self.0.objects.iter().enumerate().skip(lo).take(hi - lo) {
            let GenesisObject::RawObject { data, owner } = object.clone();
            let native = NativeObject::new_from_genesis(data, owner, TransactionDigest::genesis());

            let storage_id = native.id();
            let object = Object::from_native(SuiAddress::from(storage_id), native);
            connection.edges.push(Edge::new(idx.to_string(), object));
        }

        Ok(connection)
    }
}
