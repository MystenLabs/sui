// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    connection::{Connection, CursorType},
    Context, Object, Result,
};
use sui_types::base_types::SuiAddress as NativeSuiAddress;
use sui_types::transaction::GasData;

use crate::{
    api::scalars::{big_int::BigInt, cursor::JsonCursor},
    error::RpcError,
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

use super::{
    addressable::Addressable,
    object::{Object, ObjectKey},
};

/// Configuration for this transaction's gas price and the coins used to pay for gas.
#[derive(Clone)]
pub(crate) struct GasInput {
    pub(crate) scope: Scope,
    pub(crate) sponsor: NativeSuiAddress,
    pub(crate) price: u64,
    pub(crate) budget: u64,
    pub(crate) payment_obj_keys: Vec<ObjectKey>,
}

type CGasPayment = JsonCursor<usize>;

#[Object]
impl GasInput {
    /// Address of the owner of the gas object(s) used.
    async fn gas_sponsor(&self) -> Addressable {
        Addressable::with_address(self.scope.clone(), self.sponsor)
    }

    /// Objects used to pay for a transaction's execution and storage.
    async fn gas_payment(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CGasPayment>,
        last: Option<u64>,
        before: Option<CGasPayment>,
    ) -> Result<Connection<String, Object>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("GasInput", "gasPayment");
        let page = Page::from_params(limits, first, after, last, before)?;

        // Return empty connection if no payment objects
        if self.payment_obj_keys.is_empty() {
            return Ok(Connection::new(false, false));
        }

        let cursors = page.paginate_indices(self.payment_obj_keys.len());

        let mut conn = Connection::new(cursors.has_previous_page, cursors.has_next_page);
        for edge in cursors.edges {
            let object_key = &self.payment_obj_keys[*edge.cursor];

            let object = Object::by_key(ctx, self.scope.clone(), object_key.clone())
                .await
                .map_err(|e| {
                    RpcError::InternalError(
                        anyhow::anyhow!("Failed to load gas payment object: {:?}", e).into(),
                    )
                })?
                .ok_or_else(|| {
                    RpcError::InternalError(anyhow::anyhow!("Gas object not found").into())
                })?;

            conn.edges.push(async_graphql::connection::Edge::new(
                edge.cursor.encode_cursor(),
                object,
            ));
        }

        Ok(conn)
    }

    /// An unsigned integer specifying the number of native tokens per gas unit this transaction will pay (in MIST).
    async fn gas_price(&self) -> BigInt {
        self.price.into()
    }

    /// The maximum number of gas units that can be expended by executing this transaction.
    async fn gas_budget(&self) -> BigInt {
        self.budget.into()
    }
}

impl GasInput {
    pub(crate) fn from_gas_data(scope: Scope, gas_data: &GasData) -> Self {
        let payment_obj_keys = match gas_data.owner {
            NativeSuiAddress::ZERO => vec![], // system transactions do not have payment objects
            _ => gas_data
                .payment
                .iter()
                .map(|o| ObjectKey {
                    address: o.0.into(),
                    version: Some(o.1.value().into()),
                    root_version: None,
                    at_checkpoint: None,
                })
                .collect(),
        };

        Self {
            scope,
            sponsor: gas_data.owner,
            price: gas_data.price,
            budget: gas_data.budget,
            payment_obj_keys,
        }
    }
}
