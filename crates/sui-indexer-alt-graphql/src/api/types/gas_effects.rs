// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::SimpleObject;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::digests::ObjectDigest;
use sui_types::effects::TransactionEffects as NativeTransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Owner;

use crate::api::types::gas::GasCostSummary;
use crate::api::types::object::Object;
use crate::scope::Scope;

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
        // This is the value return if there is no real gas object.
        const SENTINEL: (ObjectID, SequenceNumber, ObjectDigest, Owner) = (
            ObjectID::ZERO,
            SequenceNumber::MIN,
            ObjectDigest::MIN,
            Owner::AddressOwner(SuiAddress::ZERO),
        );

        let ((id, version, digest), owner) = effects.gas_object();
        let gas_object = ((id, version, digest, owner) != SENTINEL)
            .then(|| Object::with_ref(&scope, id.into(), version, digest));

        let gas_summary = Some(effects.gas_cost_summary().clone().into());

        Self {
            gas_object,
            gas_summary,
        }
    }
}
