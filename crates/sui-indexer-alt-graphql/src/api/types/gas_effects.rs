// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::SimpleObject;
use sui_types::effects::{TransactionEffects as NativeTransactionEffects, TransactionEffectsAPI};

use crate::{api::types::gas::GasCostSummary, scope::Scope};

use super::object::Object;

/// Effects related to gas (costs incurred and the identity of the smashed gas object returned).
#[derive(SimpleObject)]
pub(crate) struct GasEffects {
    /// The gas object used to pay for this transaction. If multiple gas coins were provided, this represents the combined coin after smashing.
    gas_object: Option<Object>,
    /// Breakdown of the gas costs for this transaction.
    gas_summary: Option<GasCostSummary>,
}

impl GasEffects {
    pub(crate) fn from_effects(scope: Scope, effects: &NativeTransactionEffects) -> Self {
        let ((id, version, digest), _owner) = effects.gas_object();
        let gas_object = Some(Object::with_ref(&scope, id.into(), version, digest));
        let gas_summary = Some(effects.gas_cost_summary().clone().into());

        Self {
            gas_object,
            gas_summary,
        }
    }
}
