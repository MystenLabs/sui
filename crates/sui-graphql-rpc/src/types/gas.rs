// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::object::Object;
use async_graphql::*;

use super::{address::Address, big_int::BigInt};

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct GasInput {
    pub gas_sponsor: Option<Address>,
    pub gas_payment: Option<Vec<Object>>,
    pub gas_price: Option<BigInt>,
    pub gas_budget: Option<BigInt>,
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct GasCostSummary {
    pub computation_cost: Option<BigInt>,
    pub storage_cost: Option<BigInt>,
    pub storage_rebate: Option<BigInt>,
    pub non_refundable_storage_fee: Option<BigInt>,
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct GasEffects {
    pub gas_object: Option<Object>,
    pub gas_summary: Option<GasCostSummary>,
}
