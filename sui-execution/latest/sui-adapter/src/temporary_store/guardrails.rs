// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Enforcement of the sender's guardrails policy (`sui::guardrails`) after a user transaction has
//! executed.
//!
//! Consensus added the policy as a mutable shared input, so the sender's transactions execute in
//! sequence and the policy holds the spend of every earlier one. The check computes how much of
//! each coin type left the sender's coins and address balance, compares it with the policy's
//! per-epoch limits, and writes the new spend into the policy before storage is metered. Gas is
//! not counted: the check runs before gas is charged.

use std::collections::BTreeMap;

use move_core_types::language_storage::TypeTag;
use sui_types::{
    balance::Balance,
    base_types::{ObjectID, SuiAddress},
    effects::{AccumulatorOperation, AccumulatorValue},
    error::ExecutionError,
    execution_status::{ExecutionErrorKind, GuardrailsViolationKind},
    guardrails::{Policy, sender_policy_id},
    object::{Object, Owner},
    transaction::{GasData, TransactionKind},
};

use crate::temporary_store::TemporaryStore;

type Totals = BTreeMap<TypeTag, u128>;

/// What the guardrails check keeps about a user transaction.
#[derive(Clone, Copy)]
pub(super) struct GuardrailsTx {
    sender: SuiAddress,
    gas_owner: SuiAddress,
    policy_id: ObjectID,
}

impl GuardrailsTx {
    pub(super) fn new(
        transaction_kind: &TransactionKind,
        gas_data: &GasData,
        sender: SuiAddress,
    ) -> Option<Self> {
        Some(Self {
            sender,
            gas_owner: gas_data.owner,
            policy_id: sender_policy_id(transaction_kind, sender)?,
        })
    }
}

fn add(totals: &mut Totals, coin_type: TypeTag, amount: u64) {
    let total = totals.entry(coin_type).or_default();
    *total = total.saturating_add(u128::from(amount));
}

fn violation(account: SuiAddress, kind: GuardrailsViolationKind) -> ExecutionError {
    ExecutionError::from_kind(ExecutionErrorKind::GuardrailsViolation { account, kind })
}

impl TemporaryStore<'_> {
    /// Fails the transaction if more left the sender's coins and address balance than the
    /// sender's policy has left for the epoch, and otherwise records what left in the policy.
    /// Runs after the PTB and before storage is metered, so the policy write is paid for like
    /// any other.
    pub(crate) fn check_guardrails(&mut self) -> Result<(), ExecutionError> {
        let Some(GuardrailsTx {
            sender,
            gas_owner,
            policy_id,
        }) = self.guardrails
        else {
            return Ok(());
        };
        // Whether the policy binds the transaction is judged on the policy as the transaction
        // found it. A transaction the guardian sponsors stays exempt when it changes the guardian.
        let Some(policy) = self
            .input_objects
            .get(&policy_id)
            .and_then(Policy::from_object)
        else {
            return Ok(());
        };
        if !policy.is_active(self.cur_epoch) || policy.exempts(gas_owner) {
            return Ok(());
        }
        if !self.mutable_input_refs.contains_key(&policy_id) {
            return Err(violation(
                sender,
                GuardrailsViolationKind::PolicyNotWritable,
            ));
        }
        // The outflow counts coins with a single owner only.
        if self
            .input_objects
            .values()
            .any(|object| matches!(object.owner, Owner::Party { .. }) && object.is_coin())
        {
            return Err(violation(sender, GuardrailsViolationKind::PartyCoin));
        }
        let outflow = self.sender_outflow(sender);
        if outflow.is_empty() {
            return Ok(());
        }
        // The spend goes into the policy as the transaction left it, which keeps a
        // `request_disable` made in the same transaction.
        let mut object = self
            .execution_results
            .written_objects
            .get(&policy_id)
            .unwrap_or(&self.input_objects[&policy_id])
            .clone();
        let Some(mut recorded) = Policy::from_object(&object) else {
            return Err(ExecutionError::invariant_violation(
                "a guardrails policy stopped being one",
            ));
        };
        for (coin_type, amount) in outflow {
            if policy
                .spent(self.cur_epoch, &coin_type)
                .saturating_add(amount)
                > policy.limit(&coin_type)
            {
                return Err(violation(
                    sender,
                    GuardrailsViolationKind::BudgetExhausted {
                        coin_type: Box::new(coin_type),
                    },
                ));
            }
            recorded.record(self.cur_epoch, &coin_type, amount);
        }
        recorded.write_to(&mut object);
        self.mutate_input_object(object);
        Ok(())
    }

    /// How much of each coin type the sender's own `Coin` objects and address balance hold less
    /// of after the transaction than before. Value the sender moved into any other object counts
    /// as gone, including an object the sender owns. Value the sender got back in the same
    /// transaction offsets what left.
    fn sender_outflow(&self, sender: SuiAddress) -> BTreeMap<TypeTag, u64> {
        let results = &self.execution_results;
        let sender_coin = |object: &Object| -> Option<(TypeTag, u64)> {
            let owner = match &object.owner {
                Owner::AddressOwner(owner) | Owner::ConsensusAddressOwner { owner, .. } => *owner,
                Owner::ObjectOwner(_)
                | Owner::Shared { .. }
                | Owner::Immutable
                | Owner::Party { .. } => return None,
            };
            if owner != sender {
                return None;
            }
            Some((object.coin_type_maybe()?, object.get_coin_value_unsafe()))
        };

        let mut before = Totals::new();
        let mut after = Totals::new();
        for (id, object) in &self.input_objects {
            let Some((coin_type, value)) = sender_coin(object) else {
                continue;
            };
            // `modified_objects` holds every input that was written, wrapped or deleted. An
            // input outside it is unchanged, and a written one is counted below.
            if !results.modified_objects.contains(id) && !results.deleted_object_ids.contains(id) {
                add(&mut after, coin_type.clone(), value);
            }
            add(&mut before, coin_type, value);
        }
        for object in results.written_objects.values() {
            if let Some((coin_type, value)) = sender_coin(object) {
                add(&mut after, coin_type, value);
            }
        }
        // Address balance: a Split took the amount out of the address, a Merge put it in.
        for event in &results.accumulator_events {
            if event.write.address.address != sender {
                continue;
            }
            let Some(coin_type) = Balance::maybe_get_balance_type_param(&event.write.address.ty)
            else {
                continue;
            };
            let AccumulatorValue::Integer(amount) = event.write.value else {
                continue;
            };
            match event.write.operation {
                AccumulatorOperation::Split => add(&mut before, coin_type, amount),
                AccumulatorOperation::Merge => add(&mut after, coin_type, amount),
            }
        }

        before
            .into_iter()
            .filter_map(|(coin_type, before)| {
                let left = before.saturating_sub(after.get(&coin_type).copied().unwrap_or(0));
                (left > 0).then(|| (coin_type, u64::try_from(left).unwrap_or(u64::MAX)))
            })
            .collect()
    }
}
