// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::object::Object;
use async_graphql::connection::Connection;
use async_graphql::*;
use sui_types::{
    effects::{TransactionEffects as NativeTransactionEffects, TransactionEffectsAPI},
    gas::GasCostSummary as NativeGasCostSummary,
    transaction::GasData,
};

use super::{address::Address, big_int::BigInt, sui_address::SuiAddress};
use super::{
    cursor::Page,
    object::{self, ObjectFilter, ObjectKey},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GasInput {
    pub owner: SuiAddress,
    pub price: u64,
    pub budget: u64,
    pub payment_obj_keys: Vec<ObjectKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GasCostSummary {
    pub computation_cost: u64,
    pub storage_cost: u64,
    pub storage_rebate: u64,
    pub non_refundable_storage_fee: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GasEffects {
    pub summary: GasCostSummary,
    pub object_id: SuiAddress,
    pub object_version: u64,
}

/// Configuration for this transaction's gas price and the coins used to pay for gas.
#[Object]
impl GasInput {
    /// Address of the owner of the gas object(s) used
    async fn gas_sponsor(&self) -> Option<Address> {
        Some(Address {
            address: self.owner,
        })
    }

    /// Objects used to pay for a transaction's execution and storage
    async fn gas_payment(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
    ) -> Result<Connection<String, Object>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;

        let filter = ObjectFilter {
            object_keys: Some(self.payment_obj_keys.clone()),
            ..Default::default()
        };

        Object::paginate(ctx.data_unchecked(), page, filter)
            .await
            .extend()
    }

    /// An unsigned integer specifying the number of native tokens per gas unit this transaction
    /// will pay (in MIST).
    async fn gas_price(&self) -> Option<BigInt> {
        Some(BigInt::from(self.price))
    }

    /// The maximum number of gas units that can be expended by executing this transaction
    async fn gas_budget(&self) -> Option<BigInt> {
        Some(BigInt::from(self.budget))
    }
}

/// Breakdown of gas costs in effects.
#[Object]
impl GasCostSummary {
    /// Gas paid for executing this transaction (in MIST).
    async fn computation_cost(&self) -> Option<BigInt> {
        Some(BigInt::from(self.computation_cost))
    }

    /// Gas paid for the data stored on-chain by this transaction (in MIST).
    async fn storage_cost(&self) -> Option<BigInt> {
        Some(BigInt::from(self.storage_cost))
    }

    /// Part of storage cost that can be reclaimed by cleaning up data created by this transaction
    /// (when objects are deleted or an object is modified, which is treated as a deletion followed
    /// by a creation) (in MIST).
    async fn storage_rebate(&self) -> Option<BigInt> {
        Some(BigInt::from(self.storage_rebate))
    }

    /// Part of storage cost that is not reclaimed when data created by this transaction is cleaned
    /// up (in MIST).
    async fn non_refundable_storage_fee(&self) -> Option<BigInt> {
        Some(BigInt::from(self.non_refundable_storage_fee))
    }
}

/// Effects related to gas (costs incurred and the identity of the smashed gas object returned).
#[Object]
impl GasEffects {
    async fn gas_object(&self, ctx: &Context<'_>) -> Result<Option<Object>> {
        Object::query(
            ctx.data_unchecked(),
            self.object_id,
            Some(self.object_version),
        )
        .await
        .extend()
    }

    async fn gas_summary(&self) -> Option<&GasCostSummary> {
        Some(&self.summary)
    }
}

impl GasEffects {
    pub(crate) fn from(effects: &NativeTransactionEffects) -> Self {
        let ((id, version, _digest), _owner) = effects.gas_object();
        Self {
            summary: GasCostSummary::from(effects.gas_cost_summary()),
            object_id: SuiAddress::from(id),
            object_version: version.value(),
        }
    }
}

impl From<&GasData> for GasInput {
    fn from(s: &GasData) -> Self {
        Self {
            owner: s.owner.into(),
            price: s.price,
            budget: s.budget,
            payment_obj_keys: s
                .payment
                .iter()
                .map(|o| ObjectKey {
                    object_id: o.0.into(),
                    version: o.1.value(),
                })
                .collect(),
        }
    }
}

impl From<&NativeGasCostSummary> for GasCostSummary {
    fn from(gcs: &NativeGasCostSummary) -> Self {
        Self {
            computation_cost: gcs.computation_cost,
            storage_cost: gcs.storage_cost,
            storage_rebate: gcs.storage_rebate,
            non_refundable_storage_fee: gcs.non_refundable_storage_fee,
        }
    }
}
