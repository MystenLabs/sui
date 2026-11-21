// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The consensus objects a user transaction takes: the shared objects it declares, and the
//! mutable shared inputs consensus adds to it. Today consensus adds only the sender's guardrails
//! policy (see `guardrails/DESIGN.md`).
//!
//! Before version assignment the added inputs come from the latest state. After it they are the
//! assigned objects the transaction does not declare, and in effects they are the accessed
//! consensus objects it does not declare.

use mysten_common::debug_fatal;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::ObjectID,
    committee::EpochId,
    effects::{TransactionEffects, undeclared_consensus_inputs},
    error::SuiResult,
    guardrails::implicit_policy_input,
    object::Object,
    transaction::{
        InputObjectKind, SharedInputObject, SharedObjectMutability, TransactionData,
        TransactionDataAPI,
    },
};

use crate::authority::{
    authority_per_epoch_store::AuthorityPerEpochStore,
    shared_object_version_manager::AssignedVersions,
};
use crate::execution_cache::ObjectCacheRead;

/// The shared objects consensus adds to `tx_data` in `epoch`, given `latest`, which reads the
/// latest version of an object. For anything that executes a transaction without consensus.
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
        .map(|((id, initial_shared_version), _)| SharedInputObject {
            id: *id,
            initial_shared_version: *initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        })
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
    for id in undeclared_consensus_inputs(tx_data, effects) {
        // Effects carry no initial shared version, and the initial shared version of an object
        // never changes, so the latest version of the object has it.
        let Some(initial_shared_version) = cache_reader
            .get_object(&id)
            .and_then(|object| object.owner().start_version())
        else {
            debug_fatal!("consensus object {id} added to {tx_data:?} is missing");
            continue;
        };
        inputs.push(SharedInputObject {
            id,
            initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        });
    }
    inputs
}
