// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The consensus objects a user transaction takes: the shared objects it declares, and the
//! mutable shared inputs consensus adds to it. Today consensus adds only the sender's guardrails
//! policy (see `guardrails/DESIGN.md`).
//!
//! Before version assignment the added inputs come from the latest state. After it they are the
//! assigned objects the transaction does not declare, and in effects they are the sender's
//! policy when the transaction accessed it without declaring it.

use mysten_common::fatal;
use sui_types::{
    consensus_inputs::{added_input, added_input_ids_in_effects, added_shared_inputs_at_latest},
    effects::TransactionEffects,
    error::SuiResult,
    transaction::{InputObjectKind, SharedInputObject, TransactionData, TransactionDataAPI},
};

use crate::authority::{
    authority_per_epoch_store::AuthorityPerEpochStore,
    shared_object_version_manager::AssignedVersions,
};
use crate::execution_cache::ObjectCacheRead;

/// The shared objects consensus sequences `tx_data` on.
pub(crate) fn sequenced_shared_inputs(
    epoch_store: &AuthorityPerEpochStore,
    cache_reader: &dyn ObjectCacheRead,
    tx_data: &TransactionData,
) -> Vec<SharedInputObject> {
    let mut inputs = tx_data.shared_input_objects();
    inputs.extend(added_shared_inputs_at_latest(
        epoch_store.protocol_config(),
        epoch_store.epoch(),
        tx_data,
        |id| cache_reader.get_object(id),
    ));
    inputs
}

/// The shared objects consensus added to `tx_data`, as the versions it assigned show them.
fn added_shared_inputs(
    tx_data: &TransactionData,
    assigned_versions: &AssignedVersions,
) -> Vec<SharedInputObject> {
    let declared = tx_data.shared_input_objects();
    assigned_versions
        .iter()
        .filter(|((id, _), _)| declared.iter().all(|input| input.id != *id))
        .map(|((id, initial_shared_version), _)| added_input(*id, *initial_shared_version))
        .collect()
}

/// The shared objects `tx_data` takes, given the versions consensus assigned it.
pub(crate) fn assigned_shared_inputs(
    tx_data: &TransactionData,
    assigned_versions: &AssignedVersions,
) -> Vec<SharedInputObject> {
    let mut inputs = tx_data.shared_input_objects();
    inputs.extend(added_shared_inputs(tx_data, assigned_versions));
    inputs
}

/// The objects to load to execute `tx_data` with the versions consensus assigned it.
pub(crate) fn input_objects_for_execution(
    tx_data: &TransactionData,
    assigned_versions: &AssignedVersions,
) -> SuiResult<Vec<InputObjectKind>> {
    let mut kinds = tx_data.input_objects()?;
    kinds.extend(
        added_shared_inputs(tx_data, assigned_versions)
            .into_iter()
            .map(InputObjectKind::from),
    );
    Ok(kinds)
}

/// The shared objects `effects` show `tx_data` took.
pub(crate) fn shared_inputs_from_effects(
    cache_reader: &dyn ObjectCacheRead,
    tx_data: &TransactionData,
    effects: &TransactionEffects,
) -> Vec<SharedInputObject> {
    let mut inputs = tx_data.shared_input_objects();
    for id in added_input_ids_in_effects(tx_data, effects) {
        // Effects carry no initial shared version, and the initial shared version of an object
        // never changes, so the latest version of the object has it. Without it the transaction
        // would execute without the input and produce effects other than the certified ones.
        let Some(initial_shared_version) = cache_reader
            .get_object(&id)
            .and_then(|object| object.owner().start_version())
        else {
            fatal!("consensus object {id} added to {tx_data:?} is missing");
        };
        inputs.push(added_input(id, initial_shared_version));
    }
    inputs
}
