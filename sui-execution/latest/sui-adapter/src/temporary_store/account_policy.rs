// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Post-execution enforcement of the sender's account policy (`sui::account_policy`).
//!
//! The policy is looked up as a dynamic field of the account policy registry, bounded by the
//! registry version consensus assigned to this transaction, so every validator sees the same
//! policy. The registry read is only recorded in effects when the sender has a policy: a
//! transaction whose sender has none behaves identically whether or not the version is pinned on
//! replay.

use std::collections::{BTreeMap, BTreeSet};

use move_core_types::language_storage::TypeTag;
use mysten_common::debug_fatal;
use sui_types::{
    SUI_ACCOUNT_POLICY_REGISTRY_OBJECT_ID,
    account_policy::{AccountPolicy, account_policy_field_id},
    base_types::{ObjectID, SuiAddress},
    effects::{AccumulatorOperation, AccumulatorValue},
    error::ExecutionError,
    execution_status::{AccountPolicyViolationKind, ExecutionErrorKind},
    gas_coin::GAS,
    governance::StakedSui,
    object::{Object, Owner},
    storage::BackingPackageStore,
    transaction::{Command, GasData, TransactionKind},
};

use crate::temporary_store::TemporaryStore;

/// What the policy check needs to know about each PTB command.
enum CommandSummary {
    MoveCall(ObjectID),
    Publish,
    Other,
}

/// Facts about the transaction that the policy check needs and that execution results do not
/// carry. Only user PTBs have them; system transactions have no sender policy.
pub(super) struct AccountPolicyTxInputs {
    sender: SuiAddress,
    co_signers: Vec<SuiAddress>,
    gas_budget: u64,
    commands: Vec<CommandSummary>,
}

impl AccountPolicyTxInputs {
    pub(super) fn new(
        transaction_kind: &TransactionKind,
        gas_data: &GasData,
        sender: SuiAddress,
        co_signers: &[SuiAddress],
    ) -> Option<Self> {
        let TransactionKind::ProgrammableTransaction(pt) = transaction_kind else {
            return None;
        };
        let commands = pt
            .commands
            .iter()
            .map(|command| match command {
                Command::MoveCall(call) => CommandSummary::MoveCall(call.package),
                Command::Publish(..) | Command::Upgrade(..) => CommandSummary::Publish,
                _ => CommandSummary::Other,
            })
            .collect();
        Some(Self {
            sender,
            co_signers: co_signers.to_vec(),
            gas_budget: gas_data.budget,
            commands,
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
        if !policy.is_active(self.cur_epoch) || policy.is_guardian_approved(&inputs.co_signers) {
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

        // Which package each command calls, by original ID so upgrades keep working.
        let mut called_packages: Vec<Option<ObjectID>> = Vec::with_capacity(inputs.commands.len());
        for command in &inputs.commands {
            called_packages.push(match command {
                CommandSummary::MoveCall(package) => {
                    let Some(original) = self.original_package_id(*package) else {
                        return violation(AccountPolicyViolationKind::PackageNotAllowed);
                    };
                    if !policy.allows_package(original) {
                        return violation(AccountPolicyViolationKind::PackageNotAllowed);
                    }
                    Some(original)
                }
                CommandSummary::Publish => {
                    return violation(AccountPolicyViolationKind::PublishNotAllowed);
                }
                CommandSummary::Other => None,
            });
        }

        let sender = inputs.sender;
        let before = self.sender_objects_before(sender);

        // Every non-coin object the sender held must still be theirs, be with a listed recipient,
        // or have been taken by a package the policy trusts with custody of that type.
        for (id, object) in &before {
            if object.is_coin() {
                continue;
            }
            let after = self.execution_results.written_objects.get(id);
            if after.is_some_and(|after| owned_by(after, sender)) {
                continue;
            }
            if after.is_some_and(|after| {
                owner_address(after).is_some_and(|recipient| policy.is_recipient(recipient))
            }) {
                continue;
            }
            let Some(object_type) = object.type_().map(|ty| ty.clone().into()) else {
                return violation(AccountPolicyViolationKind::ObjectTransferNotAllowed);
            };
            let custodian = self
                .object_consumers
                .get(id)
                .and_then(|command| called_packages.get(usize::from(*command)))
                .copied()
                .flatten();
            if !custodian.is_some_and(|package| policy.allows_custody(package, &object_type)) {
                return violation(AccountPolicyViolationKind::ObjectTransferNotAllowed);
            }
        }

        // Net outflow per coin type: value held before, minus value still with the sender or
        // sent to a listed recipient afterwards, plus net address-balance withdrawals.
        let mut outflows: BTreeMap<TypeTag, i128> = BTreeMap::new();
        for object in before.values() {
            if let Some((coin_type, value)) = coin_value(object) {
                *outflows.entry(coin_type).or_default() += value as i128;
            }
        }
        for object in self.execution_results.written_objects.values() {
            let kept = owned_by(object, sender)
                || owner_address(object).is_some_and(|recipient| policy.is_recipient(recipient));
            if !kept {
                continue;
            }
            if let Some((coin_type, value)) = coin_value(object) {
                *outflows.entry(coin_type).or_default() -= value as i128;
            }
        }
        for event in &self.execution_results.accumulator_events {
            let AccumulatorValue::Integer(amount) = event.write.value else {
                continue;
            };
            let Some(coin_type) = balance_coin_type(&event.write.address.ty) else {
                continue;
            };
            let address = event.write.address.address;
            let delta = if address == sender {
                match event.write.operation {
                    AccumulatorOperation::Split => amount as i128,
                    AccumulatorOperation::Merge => -(amount as i128),
                }
            } else if policy.is_recipient(address)
                && event.write.operation == AccumulatorOperation::Merge
            {
                -(amount as i128)
            } else {
                continue;
            };
            *outflows.entry(coin_type).or_default() += delta;
        }
        for (coin_type, outflow) in outflows {
            if outflow > policy.coin_limit(&coin_type) as i128 {
                return violation(AccountPolicyViolationKind::CoinOutflowExceeded);
            }
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

    fn original_package_id(&self, package: ObjectID) -> Option<ObjectID> {
        self.get_package_object(&package)
            .ok()
            .flatten()
            .map(|package| package.move_package().original_package_id())
    }

    /// Objects the sender possessed when the transaction started: owned inputs, plus objects
    /// loaded at runtime that belonged to the sender or to one of the sender's objects (received
    /// objects and dynamic fields), as they were before the transaction.
    fn sender_objects_before(&self, sender: SuiAddress) -> BTreeMap<ObjectID, Object> {
        let mut objects: BTreeMap<ObjectID, Object> = self
            .input_objects
            .iter()
            .filter(|(_, object)| owned_by(object, sender))
            .map(|(id, object)| (*id, object.clone()))
            .collect();
        // Two passes pick up children of children (an object received by a dynamic field).
        for _ in 0..2 {
            let possessed: BTreeSet<SuiAddress> = objects
                .keys()
                .map(|id| SuiAddress::from(*id))
                .chain(std::iter::once(sender))
                .collect();
            for (id, metadata) in &self.loaded_runtime_objects {
                if objects.contains_key(id) {
                    continue;
                }
                let holder = match &metadata.owner {
                    Owner::AddressOwner(holder) | Owner::ObjectOwner(holder) => *holder,
                    Owner::ConsensusAddressOwner { owner, .. } => *owner,
                    Owner::Shared { .. } | Owner::Immutable | Owner::Party { .. } => continue,
                };
                if !possessed.contains(&holder) {
                    continue;
                }
                if let Some(object) = self.store.get_object_by_key(id, metadata.version) {
                    objects.insert(*id, object);
                }
            }
        }
        objects
    }
}

fn owned_by(object: &Object, address: SuiAddress) -> bool {
    owner_address(object) == Some(address)
}

/// The address an object is owned by, including object IDs as addresses for objects held by
/// other objects. Shared and immutable objects have none.
fn owner_address(object: &Object) -> Option<SuiAddress> {
    match object.owner() {
        Owner::AddressOwner(address) | Owner::ObjectOwner(address) => Some(*address),
        Owner::ConsensusAddressOwner { owner, .. } => Some(*owner),
        Owner::Shared { .. } | Owner::Immutable | Owner::Party { .. } => None,
    }
}

/// The coin type and amount an object holds: a coin's balance, or a stake's principal as SUI.
fn coin_value(object: &Object) -> Option<(TypeTag, u64)> {
    if let Some(coin_type) = object.coin_type_maybe() {
        return Some((coin_type, object.get_coin_value_unsafe()));
    }
    if object
        .data
        .try_as_move()
        .is_some_and(|move_object| move_object.is_staked_sui())
    {
        let principal = StakedSui::try_from(object).ok()?.principal();
        return Some((GAS::type_tag(), principal));
    }
    None
}

/// `T` for a `Balance<T>` accumulator type.
fn balance_coin_type(balance_type: &TypeTag) -> Option<TypeTag> {
    use sui_types::balance::Balance;
    let TypeTag::Struct(tag) = balance_type else {
        return None;
    };
    Balance::is_balance_type(balance_type)
        .then(|| tag.type_params.first().cloned())
        .flatten()
}
