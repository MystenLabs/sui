// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;
use sui_types::{
    effects::{TransactionEffects as NativeTransactionEffects, TransactionEffectsAPI},
};

use crate::{
    api::{types::{address::Address, gas::GasCostSummary}},
    error::RpcError,
    scope::Scope,
};

use super::object::Object;

pub(crate) struct GasEffects {
    pub(crate) scope: Scope,
    pub(crate) native: NativeTransactionEffects,
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
        Some(self.native.gas_cost_summary().clone().into())
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
