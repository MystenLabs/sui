// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The shared inputs consensus adds to a user transaction beyond the ones it declares. Today the
//! only one is the sender's guardrails policy (`crate::guardrails::implicit_policy_input`).

use std::collections::BTreeSet;

use crate::{
    IMPLICITLY_READ_SYSTEM_OBJECTS,
    base_types::{ObjectID, SequenceNumber},
    effects::{TransactionEffects, TransactionEffectsAPI},
    transaction::{SharedInputObject, SharedObjectMutability, TransactionData, TransactionDataAPI},
};

/// The shared input consensus adds for the object `id`. Consensus adds only mutable inputs.
pub fn added_input(id: ObjectID, initial_shared_version: SequenceNumber) -> SharedInputObject {
    SharedInputObject {
        id,
        initial_shared_version,
        mutability: SharedObjectMutability::Mutable,
    }
}

/// The objects `effects` show consensus added to `tx_data`: the consensus objects it accessed
/// without declaring them, other than the implicitly read system objects.
pub fn added_input_ids_in_effects(
    tx_data: &TransactionData,
    effects: &TransactionEffects,
) -> Vec<ObjectID> {
    let declared: BTreeSet<ObjectID> = tx_data
        .shared_input_objects()
        .iter()
        .map(|input| input.id)
        .collect();
    effects
        .accessed_consensus_objects()
        .iter()
        .map(|object| object.id_and_version().0)
        .filter(|id| !declared.contains(id) && !IMPLICITLY_READ_SYSTEM_OBJECTS.contains(id))
        .collect()
}
