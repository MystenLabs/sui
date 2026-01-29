// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Context;
use async_graphql::Object;
use async_graphql::connection::Connection;
use sui_types::base_types::SuiAddress;
use sui_types::transaction::GasData;

use crate::api::scalars::big_int::BigInt;
use crate::api::scalars::cursor::JsonCursor;
use crate::api::types::address::Address;
use crate::api::types::object::Object;
use crate::error::RpcError;
use crate::pagination::Page;
use crate::pagination::PaginationConfig;
use crate::scope::Scope;

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
        (self.native.owner != SuiAddress::ZERO)
            .then(|| Address::with_address(self.scope.clone(), self.native.owner))
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
    ) -> Option<Result<Connection<String, Object>, RpcError>> {
        Some(
            async {
                // Return empty connection for system transactions.
                if self.native.owner == SuiAddress::ZERO {
                    return Ok(Connection::new(false, false));
                }

                let pagination: &PaginationConfig = ctx.data()?;
                let limits = pagination.limits("GasInput", "gasPayment");
                let page = Page::from_params(limits, first, after, last, before)?;

                page.paginate_indices(self.native.payment.len(), |i| {
                    let (id, version, digest) = self.native.payment[i];
                    Ok(Object::with_ref(&self.scope, id.into(), version, digest))
                })
            }
            .await,
        )
    }
}
