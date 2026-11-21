// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Core-side support for guardrails (see `DESIGN.md` next to this file): which policy object a
//! transaction takes as an implicit mutable input, and the signing-time checks.
//!
//! Execution enforces the sender's limits. The signing-time checks refuse what execution cannot
//! handle: a transaction that could only fail, and a withdrawal from another account that has a
//! policy. They depend only on whether a policy is active this epoch, which does not change
//! within the epoch, so every validator decides the same.

use mysten_common::debug_fatal;
use sui_types::{
    base_types::{ConsensusObjectSequenceKey, ObjectID, SuiAddress},
    committee::EpochId,
    effects::{TransactionEffects, TransactionEffectsAPI},
    error::{SuiResult, UserInputError},
    guardrails::{Policy, allowance_funders, implicit_input, policy_object_id, sender_policy_id},
    storage::ObjectStore,
    transaction::{
        InputObjectKind, SenderSignedData, SharedObjectMutability, TransactionData,
        TransactionDataAPI,
    },
};

use crate::authority::{
    AuthorityState, authority_per_epoch_store::AuthorityPerEpochStore,
    shared_object_version_manager::AssignedVersions,
};
use crate::execution_cache::ObjectCacheRead;

/// The ID of the policy `tx_data` could take implicitly: its sender's, unless it declares the
/// policy itself.
fn undeclared_policy_id(tx_data: &TransactionData) -> Option<ObjectID> {
    let id = sender_policy_id(tx_data.kind(), tx_data.sender())?;
    tx_data
        .shared_input_objects()
        .iter()
        .all(|input| input.id != id)
        .then_some(id)
}

/// The policy consensus adds to `tx_data` as a mutable input when sequencing it: the sender's,
/// if it is active this epoch.
///
/// Reads the latest version of the policy, and every validator still gets the same answer:
/// whether a policy is active in the current epoch never changes within it, since enabling and
/// disabling take effect at later epochs, and its initial shared version never changes.
pub(crate) fn implicit_policy_key(
    epoch_store: &AuthorityPerEpochStore,
    cache_reader: &dyn ObjectCacheRead,
    tx_data: &TransactionData,
) -> Option<ConsensusObjectSequenceKey> {
    if !epoch_store.protocol_config().enable_guardrails() {
        return None;
    }
    let policy = cache_reader.get_object(&undeclared_policy_id(tx_data)?)?;
    Policy::active_sequence_key(&policy, epoch_store.epoch())
}

/// The policy `effects` show `tx_data` took as an implicit input. Effects carry no initial
/// shared version, so it comes from the store, which holds every active policy: a policy
/// activates no earlier than the epoch after it is created.
pub(crate) fn policy_input_from_effects(
    epoch_store: &AuthorityPerEpochStore,
    cache_reader: &dyn ObjectCacheRead,
    tx_data: &TransactionData,
    effects: &TransactionEffects,
) -> Option<ConsensusObjectSequenceKey> {
    if !epoch_store.protocol_config().enable_guardrails() {
        return None;
    }
    let id = undeclared_policy_id(tx_data)?;
    effects
        .accessed_consensus_objects()
        .iter()
        .any(|accessed| accessed.id_and_version().0 == id)
        .then_some(())?;
    let initial_shared_version = cache_reader
        .get_object(&id)
        .and_then(|object| object.owner().start_version());
    if initial_shared_version.is_none() {
        debug_fatal!("guardrails policy {id} accessed by a transaction is missing");
    }
    Some((id, initial_shared_version?))
}

/// The policy `tx_data` was assigned as an implicit input, as the key consensus or effects gave
/// it.
pub(crate) fn assigned_policy_key(
    tx_data: &TransactionData,
    assigned_versions: &AssignedVersions,
) -> Option<ConsensusObjectSequenceKey> {
    let id = undeclared_policy_id(tx_data)?;
    assigned_versions
        .iter()
        .map(|(key, _)| *key)
        .find(|(assigned, _)| *assigned == id)
}

/// The same policy as an input object to load for execution.
pub fn assigned_policy_input(
    tx_data: &TransactionData,
    assigned_versions: &AssignedVersions,
) -> Option<InputObjectKind> {
    assigned_policy_key(tx_data, assigned_versions).map(implicit_input)
}

/// The policy a simulation of `tx_data` takes as an implicit input: the one consensus would
/// add, judged from the latest state.
pub(crate) fn policy_input_for_simulation(
    store: &dyn ObjectStore,
    epoch: EpochId,
    tx_data: &TransactionData,
) -> Option<InputObjectKind> {
    let policy = store.get_object(&undeclared_policy_id(tx_data)?)?;
    Policy::active_sequence_key(&policy, epoch).map(implicit_input)
}

impl AuthorityState {
    pub(crate) fn handle_guardrails_checks(
        &self,
        transaction: &SenderSignedData,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult<()> {
        if !epoch_store.protocol_config().enable_guardrails() {
            return Ok(());
        }
        let tx_data = transaction.transaction_data();
        let sender = tx_data.sender();
        if sender_policy_id(tx_data.kind(), sender).is_none() {
            return Ok(());
        }
        let cache = self.get_object_cache_reader();
        let epoch = epoch_store.epoch();
        let violation = |account: SuiAddress, reason: &str| {
            Err(UserInputError::GuardrailsViolation {
                account,
                reason: reason.to_string(),
            }
            .into())
        };
        let active_policy = |account: SuiAddress| {
            cache
                .get_object(&policy_object_id(account))
                .and_then(|object| Policy::from_object(&object))
                .filter(|policy| policy.is_active(epoch))
        };

        // Only its owner may declare a policy, and only with write access: every transaction of
        // the owner depends on the object, so nobody else gets to contend on it, and a read-only
        // declaration would keep execution from recording the spend.
        for input in tx_data.shared_input_objects() {
            let Some(policy) = cache
                .get_object(&input.id)
                .and_then(|object| Policy::from_object(&object))
            else {
                continue;
            };
            if policy.owner != sender {
                return violation(
                    policy.owner,
                    "only the owner of a policy may declare it as an input",
                );
            }
            if input.mutability != SharedObjectMutability::Mutable {
                return violation(policy.owner, "a policy must be declared as a mutable input");
            }
        }

        // A gasless transaction may not write an object, and execution records the spend in
        // the policy, so the transaction could only fail.
        if tx_data.is_gasless_transaction() && active_policy(sender).is_some() {
            return violation(
                sender,
                "an account with an active policy cannot send gasless transactions",
            );
        }

        // Execution checks the sender's policy only. A transaction can spend its gas coins, so
        // paying the gas of someone else's transaction would move the gas owner's SUI past its
        // policy.
        let gas_owner = tx_data.gas_owner();
        if gas_owner != sender && active_policy(gas_owner).is_some() {
            return violation(
                gas_owner,
                "an account with an active policy cannot pay the gas of another account's \
                 transaction",
            );
        }

        // Likewise for a withdrawal from another account under an allowance.
        for funder in allowance_funders(tx_data.kind(), sender) {
            if active_policy(funder).is_some() {
                return violation(
                    funder,
                    "an account with an active policy cannot fund an allowance withdrawal",
                );
            }
        }
        Ok(())
    }
}
