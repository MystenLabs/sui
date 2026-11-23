// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The shared inputs consensus adds to a user transaction beyond the ones it declares. Today the
//! only one is the sender's guardrails policy (`crate::guardrails::implicit_policy_input`).

use sui_protocol_config::ProtocolConfig;

use crate::{
    base_types::{ObjectID, SequenceNumber},
    committee::EpochId,
    effects::{TransactionEffects, TransactionEffectsAPI},
    error::UserInputResult,
    guardrails::{implicit_policy_input, sender_policy_id},
    object::Object,
    transaction::{
        InputObjectKind, SharedInputObject, SharedObjectMutability, TransactionData,
        TransactionDataAPI,
    },
};

/// The shared input consensus adds for the object `id`. Consensus adds only mutable inputs.
pub fn added_input(id: ObjectID, initial_shared_version: SequenceNumber) -> SharedInputObject {
    SharedInputObject {
        id,
        initial_shared_version,
        mutability: SharedObjectMutability::Mutable,
    }
}

/// The shared objects consensus adds to `tx_data` in `epoch`, given `latest`, which reads the
/// latest version of an object. For anything that executes or checks a transaction without
/// consensus.
pub fn added_shared_inputs_at_latest(
    protocol_config: &ProtocolConfig,
    epoch: EpochId,
    tx_data: &TransactionData,
    latest: impl FnOnce(&ObjectID) -> Option<Object>,
) -> Vec<SharedInputObject> {
    implicit_policy_input(protocol_config, epoch, tx_data, latest)
        .into_iter()
        .collect()
}

/// The objects to load to check or execute `tx_data` in `epoch` without consensus: the declared
/// inputs and the ones consensus adds, given `latest`, which reads the latest version of an object.
pub fn input_objects_at_latest(
    protocol_config: &ProtocolConfig,
    epoch: EpochId,
    tx_data: &TransactionData,
    latest: impl FnOnce(&ObjectID) -> Option<Object>,
) -> UserInputResult<Vec<InputObjectKind>> {
    let mut kinds = tx_data.input_objects()?;
    kinds.extend(
        added_shared_inputs_at_latest(protocol_config, epoch, tx_data, latest)
            .into_iter()
            .map(InputObjectKind::from),
    );
    Ok(kinds)
}

/// The only object consensus can add to `tx_data`: the sender's policy, unless the transaction
/// declares it.
pub fn addable_input_id(tx_data: &TransactionData) -> Option<ObjectID> {
    let policy_id = sender_policy_id(tx_data.kind(), tx_data.sender())?;
    let declared = tx_data
        .shared_input_objects()
        .iter()
        .any(|input| input.id == policy_id);
    (!declared).then_some(policy_id)
}

/// The objects `effects` show consensus added to `tx_data`: the sender's policy, when the
/// transaction accessed it without declaring it.
pub fn added_input_ids_in_effects(
    tx_data: &TransactionData,
    effects: &TransactionEffects,
) -> Vec<ObjectID> {
    addable_input_id(tx_data)
        .filter(|id| {
            effects
                .accessed_consensus_objects()
                .iter()
                .any(|object| object.id_and_version().0 == *id)
        })
        .into_iter()
        .collect()
}
