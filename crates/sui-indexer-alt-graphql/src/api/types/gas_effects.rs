// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;
use sui_types::{
    effects::{TransactionEffects as NativeTransactionEffects, TransactionEffectsAPI},
    gas::GasCostSummary as NativeGasCostSummary,
};

use crate::{
    api::{scalars::big_int::BigInt, types::address::Address},
    error::RpcError,
    scope::Scope,
};

use super::object::Object;

pub(crate) struct GasCostSummary {
    pub(crate) computation_cost: u64,
    pub(crate) storage_cost: u64,
    pub(crate) storage_rebate: u64,
    pub(crate) non_refundable_storage_fee: u64,
}

pub(crate) struct GasEffects {
    pub(crate) scope: Scope,
    pub(crate) native: NativeTransactionEffects,
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

    /// Part of storage cost that can be reclaimed by cleaning up data created by this transaction (when objects are deleted or an object is modified, which is treated as a deletion followed by a creation) (in MIST).
    async fn storage_rebate(&self) -> Option<BigInt> {
        Some(BigInt::from(self.storage_rebate))
    }

    /// Part of storage cost that is not reclaimed when data created by this transaction is cleaned up (in MIST).
    async fn non_refundable_storage_fee(&self) -> Option<BigInt> {
        Some(BigInt::from(self.non_refundable_storage_fee))
    }
}

/// Effects related to gas (costs incurred and the identity of the smashed gas object returned).
#[Object]
impl GasEffects {
    /// The gas object used to pay for this transaction, after being smashed.
    async fn gas_object(&self) -> Result<Option<Object>, RpcError> {
        let ((id, version, digest), _owner) = self.native.gas_object();
        let address = Address::with_address(self.scope.clone(), id.into());
        Ok(Some(Object::with_ref(address, version, digest)))
    }

    /// Breakdown of the gas costs for this transaction.
    async fn gas_summary(&self) -> Option<GasCostSummary> {
        Some(GasCostSummary::from(self.native.gas_cost_summary()))
    }
}

impl GasEffects {
    pub(crate) fn from(scope: Scope, effects: NativeTransactionEffects) -> Self {
        Self {
            scope,
            native: effects,
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
