// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Post-execution enforcement of the sender's account policy (`sui::account_policy`).
//!
//! The policy is looked up as a dynamic field of the account policy registry, bounded by the
//! registry version consensus assigned to this transaction, so every validator sees the same
//! policy. The registry read is only recorded in effects when the sender has a policy: a
//! transaction whose sender has none behaves identically whether or not the version is pinned on
//! replay.

use std::collections::BTreeSet;

use mysten_common::debug_fatal;
use sui_types::{
    SUI_ACCOUNT_POLICY_REGISTRY_OBJECT_ID,
    account_policy::{ACCOUNT_POLICY_ALLOWED_PACKAGES, AccountPolicy, account_policy_field_id},
    base_types::{ObjectID, SuiAddress},
    effects::{AccumulatorOperation, AccumulatorValue},
    error::ExecutionError,
    execution_status::{AccountPolicyViolationKind, ExecutionErrorKind},
    gas_coin::GasCoin,
    governance::StakedSui,
    object::{Object, Owner},
    transaction::{Command, GasData, TransactionKind},
};

use crate::temporary_store::TemporaryStore;

/// Facts about the transaction that the policy check needs and that execution results do not
/// carry. Only user PTBs have them; system transactions have no sender policy.
pub(super) struct AccountPolicyTxInputs {
    sender: SuiAddress,
    gas_budget: u64,
    called_packages: BTreeSet<ObjectID>,
    publishes: bool,
}

impl AccountPolicyTxInputs {
    pub(super) fn new(
        transaction_kind: &TransactionKind,
        gas_data: &GasData,
        sender: SuiAddress,
    ) -> Option<Self> {
        let TransactionKind::ProgrammableTransaction(pt) = transaction_kind else {
            return None;
        };
        let mut called_packages = BTreeSet::new();
        let mut publishes = false;
        for command in &pt.commands {
            match command {
                Command::MoveCall(call) => {
                    called_packages.insert(call.package);
                }
                Command::Publish(..) | Command::Upgrade(..) => publishes = true,
                _ => {}
            }
        }
        Some(Self {
            sender,
            gas_budget: gas_data.budget,
            called_packages,
            publishes,
        })
    }
}

impl TemporaryStore<'_> {
    /// Rejects the transaction if the sender has an active account policy that it violates.
    /// Runs after execution so it sees final ownership and balances, and before gas is charged,
    /// which is why the gas budget cap, not the gas actually used, bounds gas spend.
    pub(crate) fn check_account_policy(&self) -> Result<(), ExecutionError> {
        let Some(inputs) = &self.post_execution_check_inputs.account_policy else {
            return Ok(());
        };
        let Some(policy) = self.load_sender_policy(inputs.sender) else {
            return Ok(());
        };
        if !policy.is_active(self.cur_epoch) || policy.is_approved(&self.tx_digest) {
            return Ok(());
        }
        let violation = |kind| {
            Err(ExecutionError::from_kind(
                ExecutionErrorKind::AccountPolicyViolation { kind },
            ))
        };
        if inputs.gas_budget > policy.gas_budget_cap {
            return violation(AccountPolicyViolationKind::GasBudgetExceeded);
        }
        if inputs.publishes {
            return violation(AccountPolicyViolationKind::PublishNotAllowed);
        }
        if inputs
            .called_packages
            .iter()
            .any(|package| !ACCOUNT_POLICY_ALLOWED_PACKAGES.contains(package))
        {
            return violation(AccountPolicyViolationKind::PackageNotAllowed);
        }
        // Deleted and wrapped objects are not checked: neither can happen from a bare PTB, so
        // they can only come from an allowed package.
        let object_left_sender = self.input_objects.iter().any(|(id, before)| {
            owned_by(before, inputs.sender)
                && !before.is_gas_coin()
                && self
                    .execution_results
                    .written_objects
                    .get(id)
                    .is_some_and(|after| !owned_by(after, inputs.sender))
        });
        if object_left_sender {
            return violation(AccountPolicyViolationKind::ObjectTransferNotAllowed);
        }
        if self.sender_sui_outflow(inputs.sender) > policy.sui_limit_per_tx as i128 {
            return violation(AccountPolicyViolationKind::SuiOutflowExceeded);
        }
        Ok(())
    }

    /// The sender's policy as of the registry version assigned to this transaction, if any.
    fn load_sender_policy(&self, sender: SuiAddress) -> Option<AccountPolicy> {
        let registry_version = self
            .system_object_versions
            .get(&SUI_ACCOUNT_POLICY_REGISTRY_OBJECT_ID)?;
        // Waiting for the registry at the assigned version guarantees the child read below
        // cannot observe a stale policy.
        let registry = self.store.load_implicitly_read_system_object(
            &SUI_ACCOUNT_POLICY_REGISTRY_OBJECT_ID,
            registry_version,
        )?;
        let field = self
            .store
            .read_child_object(
                &SUI_ACCOUNT_POLICY_REGISTRY_OBJECT_ID,
                &account_policy_field_id(sender),
                registry.version(),
            )
            .ok()
            .flatten()?;
        let Some(policy) = AccountPolicy::from_field_object(&field) else {
            debug_fatal!(
                "account policy field {} has an unexpected layout",
                field.id()
            );
            return None;
        };
        self.record_implicitly_read_system_object(&registry);
        Some(policy)
    }

    /// Net SUI that left the sender in this transaction: SUI held in the sender's coins and
    /// stake before minus after, plus net withdrawals from the sender's SUI address balance.
    /// Negative when the sender gained SUI.
    fn sender_sui_outflow(&self, sender: SuiAddress) -> i128 {
        let held = |objects: &std::collections::BTreeMap<ObjectID, Object>| -> i128 {
            objects
                .values()
                .filter(|object| owned_by(object, sender))
                .map(|object| sui_held(object) as i128)
                .sum()
        };
        let held_before = held(&self.input_objects);
        let held_after = held(&self.execution_results.written_objects);
        let balance_outflow: i128 = self
            .execution_results
            .accumulator_events
            .iter()
            .filter(|event| {
                event.write.address.address == sender
                    && GasCoin::is_gas_balance_type(&event.write.address.ty)
            })
            .map(|event| {
                let AccumulatorValue::Integer(amount) = event.write.value else {
                    return 0;
                };
                match event.write.operation {
                    AccumulatorOperation::Split => amount as i128,
                    AccumulatorOperation::Merge => -(amount as i128),
                }
            })
            .sum();
        held_before - held_after + balance_outflow
    }
}

fn owned_by(object: &Object, address: SuiAddress) -> bool {
    matches!(
        object.owner(),
        Owner::AddressOwner(owner) | Owner::ConsensusAddressOwner { owner, .. } if *owner == address
    )
}

/// SUI held by `object`: a SUI coin's balance, or a stake's principal.
fn sui_held(object: &Object) -> u64 {
    if object.is_gas_coin() {
        object.get_coin_value_unsafe()
    } else if object
        .data
        .try_as_move()
        .is_some_and(|move_object| move_object.is_staked_sui())
    {
        StakedSui::try_from(object)
            .map(|stake| stake.principal())
            .unwrap_or(0)
    } else {
        0
    }
}
