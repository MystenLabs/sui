// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{server::context_ext::DataProviderContextExt, types::object::Object};
use async_graphql::*;
use sui_json_rpc_types::{OwnedObjectRef, SuiGasData, SuiObjectDataOptions};
use sui_sdk::types::{
    base_types::{ObjectID, SuiAddress as NativeSuiAddress},
    gas::GasCostSummary as NativeGasCostSummary,
};

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

#[Object]
impl GasInput {
    async fn gas_sponsor(&self) -> Option<Address> {
        Some(Address::from(SuiAddress::from(self.owner)))
    }

    async fn gas_payment(&self, ctx: &Context<'_>) -> Result<Option<Vec<Object>>> {
        let payment_objs = ctx
            .data_provider()
            .multi_get_object_with_options(
                self.payment_obj_ids.to_vec(),
                SuiObjectDataOptions::full_content(),
            )
            .await?;
        Ok(Some(payment_objs))
    }

    async fn gas_price(&self) -> Option<BigInt> {
        Some(BigInt::from(self.price))
    }

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

// Struct mirroring GraphQL object contains fields needed to produce GraphQL object
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GasEffects {
    pub gcs: GasCostSummary,
    pub object_id: ObjectID,
}

// From trait to convert data into GasEffects
impl From<(&NativeGasCostSummary, &OwnedObjectRef)> for GasEffects {
    fn from((gcs, gas_obj_ref): (&NativeGasCostSummary, &OwnedObjectRef)) -> Self {
        Self {
            gcs: gcs.into(),
            object_id: gas_obj_ref.object_id(),
        }
    }
}

// impl #[Object] macro handles conversions and data fetches
#[Object]
impl GasEffects {
    async fn gas_object(&self, ctx: &Context<'_>) -> Result<Option<Object>> {
        let gas_obj = ctx
            .data_provider()
            .get_object_with_options(self.object_id, SuiObjectDataOptions::full_content())
            .await?;
        Ok(gas_obj)
    }

    async fn gas_summary(&self) -> Option<GasCostSummary> {
        Some(self.gcs)
    }
}
