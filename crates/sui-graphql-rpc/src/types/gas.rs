// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::db_data_provider::PgManager;
use crate::types::object::Object;
use async_graphql::connection::Connection;
use async_graphql::*;
use sui_json_rpc_types::{OwnedObjectRef, SuiGasData};
use sui_types::{
    base_types::{ObjectID, SuiAddress as NativeSuiAddress},
    gas::GasCostSummary as NativeGasCostSummary,
    transaction::GasData,
};

use super::digest::Digest;
use super::object::ObjectFilter;
use super::{address::Address, big_int::BigInt, sui_address::SuiAddress};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GasInput {
    pub owner: NativeSuiAddress,
    pub price: u64,
    pub budget: u64,
    pub payment_obj_ids: Vec<ObjectID>,
}

impl From<&SuiGasData> for GasInput {
    fn from(s: &SuiGasData) -> Self {
        Self {
            owner: s.owner,
            price: s.price,
            budget: s.budget,
            payment_obj_ids: s.payment.iter().map(|o| o.object_id).collect(),
        }
    }
}

impl From<&GasData> for GasInput {
    fn from(s: &GasData) -> Self {
        Self {
            owner: s.owner,
            price: s.price,
            budget: s.budget,
            payment_obj_ids: s.payment.iter().map(|o| o.0).collect(),
        }
    }
}

#[Object]
impl GasInput {
    /// Address of the owner of the gas object(s) used
    async fn gas_sponsor(&self) -> Option<Address> {
        Some(Address::from(SuiAddress::from(self.owner)))
    }

    /// Objects used to pay for a transaction's execution and storage
    async fn gas_payment(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, Object>>> {
        let filter = ObjectFilter {
            object_ids: Some(
                self.payment_obj_ids
                    .iter()
                    .map(|id| SuiAddress::from_array(***id))
                    .collect(),
            ),
            ..Default::default()
        };

        ctx.data_unchecked::<PgManager>()
            .fetch_objs(first, after, last, before, Some(filter))
            .await
            .extend()
    }

    /// An unsigned integer specifying the number of native tokens per gas unit this transaction will pay
    async fn gas_price(&self) -> Option<BigInt> {
        Some(BigInt::from(self.price))
    }

    /// The maximum number of gas units that can be expended by executing this transaction
    async fn gas_budget(&self) -> Option<BigInt> {
        Some(BigInt::from(self.budget))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GasCostSummary {
    pub computation_cost: u64,
    pub storage_cost: u64,
    pub storage_rebate: u64,
    pub non_refundable_storage_fee: u64,
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

#[Object]
impl GasCostSummary {
    async fn computation_cost(&self) -> Option<BigInt> {
        Some(BigInt::from(self.computation_cost))
    }

    async fn storage_cost(&self) -> Option<BigInt> {
        Some(BigInt::from(self.storage_cost))
    }

    async fn storage_rebate(&self) -> Option<BigInt> {
        Some(BigInt::from(self.storage_rebate))
    }

    async fn non_refundable_storage_fee(&self) -> Option<BigInt> {
        Some(BigInt::from(self.non_refundable_storage_fee))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GasEffects {
    pub gcs: GasCostSummary,
    pub object_ref: ObjectRef,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ObjectRef {
    pub object_id: SuiAddress,
    pub version: u64,
    pub digest: Digest,
}

// From trait to convert data into GasEffects
impl From<(&NativeGasCostSummary, &OwnedObjectRef)> for GasEffects {
    fn from((gcs, gas_obj_ref): (&NativeGasCostSummary, &OwnedObjectRef)) -> Self {
        Self {
            gcs: gcs.into(),
            object_ref: ObjectRef {
                object_id: SuiAddress::from_array(**gas_obj_ref.object_id()),
                version: gas_obj_ref.version().value(),
                digest: Digest::from_array(gas_obj_ref.reference.digest.into_inner()),
            },
        }
    }
}

// impl #[Object] macro handles conversions and data fetches
#[Object]
impl GasEffects {
    async fn gas_object(&self, ctx: &Context<'_>) -> Result<Option<Object>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_obj(self.object_ref.object_id, Some(self.object_ref.version))
            .await
            .extend()
    }

    async fn gas_summary(&self) -> Option<GasCostSummary> {
        Some(self.gcs)
    }
}
