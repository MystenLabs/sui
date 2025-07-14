// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    connection::{Connection, CursorType, Edge},
    *,
};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    transaction::GasData,
};

use crate::{
    api::scalars::{big_int::BigInt, cursor::JsonCursor},
    error::RpcError,
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

use super::{address::Address, object::Object};

#[derive(Clone)]
pub(crate) struct GasInput {
    pub(crate) scope: Scope,
    pub(crate) native: GasData,
}

impl GasInput {
    pub(crate) fn from_gas_data(scope: Scope, gas_data: GasData) -> Self {
        Self {
            scope,
            native: gas_data,
        }
    }
}

type CGasPayment = JsonCursor<usize>;

#[Object]
impl GasInput {
    /// Address of the owner of the gas object(s) used.
    async fn gas_sponsor(&self) -> Option<Address> {
        if self.native.owner == SuiAddress::ZERO {
            None
        } else {
            Some(Address::with_address(self.scope.clone(), self.native.owner))
        }
    }

    /// An unsigned integer specifying the number of native tokens per gas unit this transaction will pay (in MIST).
    async fn gas_price(&self) -> Option<BigInt> {
        Some(BigInt::from(self.native.price))
    }

    /// The maximum SUI that can be expended by executing this transaction
    async fn gas_budget(&self) -> Option<BigInt> {
        Some(BigInt::from(self.native.budget))
    }

    /// Objects used to pay for a transaction's execution and storage
    async fn gas_payment(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CGasPayment>,
        last: Option<u64>,
        before: Option<CGasPayment>,
    ) -> Result<Option<Connection<String, Object>>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("GasInput", "gasPayment");
        let page = Page::from_params(limits, first, after, last, before)?;

        // Filter out payments with zero object ID (sentinel values for system transactions)
        let filtered_payments: Vec<_> = self
            .native
            .payment
            .iter()
            .filter(|(id, _, _)| *id != ObjectID::ZERO)
            .collect();

        let cursors = page.paginate_indices(filtered_payments.len());
        let mut conn = Connection::new(cursors.has_previous_page, cursors.has_next_page);
        for edge in cursors.edges {
            let (id, version, digest) = filtered_payments[*edge.cursor];
            let address = Address::with_address(self.scope.clone(), (*id).into());
            let object = Object::with_ref(address, *version, *digest);

            conn.edges
                .push(Edge::new(edge.cursor.encode_cursor(), object));
        }

        Ok(Some(conn))
    }
}
