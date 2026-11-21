// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The signing-time checks of guardrails (see `DESIGN.md` next to this file). Consensus adds the
//! sender's policy to a transaction as described in `consensus_inputs`, and execution enforces
//! the sender's limits.
//!
//! The signing-time checks refuse what execution cannot handle: a transaction that could only
//! fail, and a transaction that reaches the funds of another account that has a policy. Most of
//! them depend only on whether a policy is active this epoch, which does not change within the
//! epoch, so every validator decides the same. The check on declared policies depends on whether
//! a declared object is a policy, which changes when a policy is created. A policy created in the
//! current epoch is not active yet, so no transaction of its owner depends on it, and a validator
//! that signs before it sees the policy only lets another account contend on it for that epoch.

use sui_types::{
    base_types::SuiAddress,
    error::{SuiResult, UserInputError},
    guardrails::{Policy, allowance_funders, policy_object_id, sender_policy_id},
    transaction::{SharedObjectMutability, TransactionData, TransactionDataAPI, TransactionKind},
};

use crate::authority::{AuthorityState, authority_per_epoch_store::AuthorityPerEpochStore};

impl AuthorityState {
    pub(crate) fn handle_guardrails_checks(
        &self,
        tx_data: &TransactionData,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult<()> {
        if !epoch_store.protocol_config().enable_guardrails() {
            return Ok(());
        }
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

        // Execution checks the sender's policy only. A transaction reaches the funds of its gas
        // owner beyond the gas it pays only through `Argument::GasCoin`, so a gas owner with a
        // policy can lose at most the gas budget of a transaction that does not use it.
        let gas_owner = tx_data.gas_owner();
        if gas_owner != sender
            && let TransactionKind::ProgrammableTransaction(ptb) = tx_data.kind()
            && ptb
                .commands
                .iter()
                .any(|command| command.is_gas_coin_used())
            && active_policy(gas_owner).is_some()
        {
            return violation(
                gas_owner,
                "a transaction whose gas owner has an active policy and is not its sender \
                 cannot use the gas coin as an argument",
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
