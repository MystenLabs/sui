// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Post-execution enforcement of the sender's account policy (`sui::account_policy`).
//!
//! The policy is looked up as a dynamic field of the account policy registry, bounded by the
//! registry version consensus assigned to this transaction, so every validator sees the same
//! policy. The registry read is only recorded in effects when the sender has a policy: a
//! transaction whose sender has none behaves identically whether or not the version is pinned on
//! replay.
//!
//! Per-epoch budgets are tracked in accumulator counters keyed by owner and epoch. The check reads
//! the counter settled at the assigned accumulator root version plus the merges of earlier
//! transactions in the same commit, and the transaction's own spend is merged after gas is
//! charged, whether or not it succeeded, so failing transactions still consume gas budget.

use std::collections::{BTreeMap, BTreeSet};

use move_core_types::language_storage::TypeTag;
use mysten_common::debug_fatal;
use sui_types::{
    SUI_ACCOUNT_POLICY_REGISTRY_OBJECT_ID, SUI_ACCUMULATOR_ROOT_OBJECT_ID,
    account_policy::{
        AccountPolicy, account_policy_field_id, counter_address, custody_type_tag, spent_type_tag,
    },
    accumulator_event::AccumulatorEvent,
    accumulator_root::AccumulatorValue as AccumulatorRootValue,
    base_types::{ObjectID, SuiAddress},
    effects::{AccumulatorAddress, AccumulatorOperation, AccumulatorValue, AccumulatorWriteV1},
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

/// What a transaction charges against its sender's per-epoch budgets.
pub(super) struct AccountPolicySpend {
    owner: SuiAddress,
    epoch: u64,
    /// Net coin outflow by type; only charged if the transaction succeeds.
    coin_outflows: BTreeMap<TypeTag, u64>,
    /// Objects taken by each package (original ID); only charged if the transaction succeeds.
    custody_counts: BTreeMap<ObjectID, u64>,
}

impl TemporaryStore<'_> {
    /// Rejects the transaction if the sender has an active account policy that it violates.
    /// Runs after execution so it sees final ownership and balances, and before gas is charged,
    /// so gas is checked against the budget cap and the gas budget rather than what was used.
    pub(crate) fn check_account_policy(&mut self) -> Result<(), ExecutionError> {
        let Some(inputs) = &self.post_execution_check_inputs.account_policy else {
            return Ok(());
        };
        let Some(policy) = self.load_sender_policy(inputs.sender) else {
            return Ok(());
        };
        if !policy.is_active(self.cur_epoch) || policy.is_guardian_approved(&inputs.co_signers) {
            return Ok(());
        }
        // From here on the policy applies, so the transaction's gas counts against the budget
        // even if a rule below rejects it.
        let mut spend = AccountPolicySpend {
            owner: inputs.sender,
            epoch: self.cur_epoch,
            coin_outflows: BTreeMap::new(),
            custody_counts: BTreeMap::new(),
        };
        let result = self.check_rules(&policy, &mut spend);
        self.account_policy_spend = Some(spend);
        result
    }

    fn check_rules(
        &self,
        policy: &AccountPolicy,
        spend: &mut AccountPolicySpend,
    ) -> Result<(), ExecutionError> {
        let inputs = self
            .post_execution_check_inputs
            .account_policy
            .as_ref()
            .expect("checked by caller");
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
            let Some(package) =
                custodian.filter(|package| policy.allows_custody(*package, &object_type))
            else {
                return violation(AccountPolicyViolationKind::ObjectTransferNotAllowed);
            };
            *spend.custody_counts.entry(package).or_default() += 1;
        }
        for (package, count) in &spend.custody_counts {
            if let Some(limit) = policy.custody_limit(*package) {
                let taken = self.counter_total(
                    counter_address(sender, self.cur_epoch, Some(*package)),
                    spent_type_tag(custody_type_tag()),
                );
                if taken + u128::from(*count) > u128::from(limit) {
                    return violation(AccountPolicyViolationKind::CustodyLimitExceeded);
                }
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
        // Gas is SUI outflow too; the budget is the bound known before gas is charged.
        outflows.entry(GAS::type_tag()).or_default();
        for (coin_type, outflow) in outflows {
            let outflow = u64::try_from(outflow).unwrap_or(0);
            let gas = if coin_type == GAS::type_tag() {
                inputs.gas_budget
            } else {
                0
            };
            let spent = self.counter_total(
                counter_address(sender, self.cur_epoch, None),
                spent_type_tag(coin_type.clone()),
            );
            if spent + u128::from(outflow) + u128::from(gas)
                > u128::from(policy.coin_limit(&coin_type))
            {
                return violation(AccountPolicyViolationKind::CoinOutflowExceeded);
            }
            if outflow > 0 {
                spend.coin_outflows.insert(coin_type, outflow);
            }
        }
        Ok(())
    }

    /// Merges this transaction's spend into the sender's per-epoch counters. Coin outflow and
    /// custody only count for a successful transaction, whose writes stand; gas counts always.
    pub(crate) fn record_account_policy_spend(&mut self, gas_used: u64, succeeded: bool) {
        let Some(spend) = self.account_policy_spend.take() else {
            return;
        };
        let mut merges: Vec<(SuiAddress, TypeTag, u64)> = Vec::new();
        let coin_address = counter_address(spend.owner, spend.epoch, None);
        let mut sui_spent = gas_used;
        if succeeded {
            for (coin_type, amount) in spend.coin_outflows {
                if coin_type == GAS::type_tag() {
                    sui_spent = sui_spent.saturating_add(amount);
                } else {
                    merges.push((coin_address, spent_type_tag(coin_type), amount));
                }
            }
            for (package, count) in spend.custody_counts {
                merges.push((
                    counter_address(spend.owner, spend.epoch, Some(package)),
                    spent_type_tag(custody_type_tag()),
                    count,
                ));
            }
        }
        if sui_spent > 0 {
            merges.push((coin_address, spent_type_tag(GAS::type_tag()), sui_spent));
        }
        for (address, type_, amount) in merges {
            let Ok(field_id) = AccumulatorRootValue::get_field_id(address, &type_) else {
                debug_fatal!("account policy counter type {type_} is not an accumulator type");
                continue;
            };
            self.add_accumulator_event(AccumulatorEvent::new(
                field_id,
                AccumulatorWriteV1 {
                    address: AccumulatorAddress::new(address, type_),
                    operation: AccumulatorOperation::Merge,
                    value: AccumulatorValue::Integer(amount),
                },
            ));
        }
    }

    /// Settled value of a spend counter at the assigned accumulator root version, plus merges from
    /// earlier transactions in the same commit. Zero if accumulators are not enabled.
    fn counter_total(&self, address: SuiAddress, type_: TypeTag) -> u128 {
        let Some(root) = self.load_implicitly_read_system_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID)
        else {
            return 0;
        };
        let version = root.version();
        let settled = AccumulatorRootValue::load(self, Some(version), address, &type_)
            .ok()
            .flatten()
            .and_then(|value| value.as_u128())
            .unwrap_or(0);
        let unsettled = AccumulatorRootValue::get_field_id(address, &type_)
            .map(|field_id| {
                self.unsettled_object_funds
                    .get_unsettled_counter_merge(&field_id, version)
            })
            .unwrap_or(0);
        settled.saturating_add(unsettled)
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
