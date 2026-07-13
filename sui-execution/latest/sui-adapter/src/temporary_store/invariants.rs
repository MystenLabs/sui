// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Post-execution system-invariant checks for transaction execution.
//!
//! All of the defense-in-depth checks that verify the finalized execution results satisfy system
//! invariants live here, behind the [`InvariantChecker`] API: that the transaction neither mints
//! nor burns SUI, that balance-accumulator withdrawals stay authorized, and that every modified
//! object traces back to an authenticated owner. [`TemporaryStore`] owns one `InvariantChecker`,
//! forwards the little bookkeeping it accumulates during execution to it (see
//! [`record_ptb_event_range`]), and defers to it for the checks. Keeping it in its own module
//! keeps the invariant accounting -- which reaches across gas, accumulator events, settlement SUI,
//! per-object storage rebates, and object ownership -- out of the main store code.
//!
//! Note these are invariant *assertions* (a failure means a bug, and aborts or panics), distinct
//! from transaction-validation guards like `TemporaryStore::check_accumulator_amounts_representable`
//! that can legitimately reject a well-formed transaction.
//!
//! [`record_ptb_event_range`]: InvariantChecker::record_ptb_event_range

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ops::Range;
use std::sync::Arc;

use move_vm_runtime::runtime::MoveRuntime;
use mysten_common::debug_fatal;

use sui_types::TypeTag;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::coin_reservation::ParsedDigest;
use sui_types::effects::{AccumulatorOperation, AccumulatorValue};
use sui_types::error::{ExecutionError, SuiResult};
use sui_types::execution::DynamicallyLoadedObjectMetadata;
use sui_types::gas::GasCostSummary;
use sui_types::is_system_package;
use sui_types::layout_resolver::LayoutResolver;
use sui_types::object::{Object, ObjectPermissions, Owner};
use sui_types::transaction::{GasData, TransactionKind};

use crate::execution_mode::ExecutionMode;
use crate::gas_charger::{GasCharger, PaymentLocation};
use crate::temporary_store::TemporaryStore;
use crate::type_layout_resolver::TypeLayoutResolver;

/// The per-transaction inputs the invariant checks need that are derived from the raw transaction
/// (rather than accumulated during execution). Built once, up front, by
/// [`InvariantChecker::set_transaction_inputs`] so the checks read them off the store instead of
/// receiving them as threaded arguments.
#[derive(Default)]
struct InvariantInputs {
    /// Per-`(address, type)` funds-accumulator reservation budget authorized by this transaction.
    /// Sources: PTB `FundsWithdrawalArg`s (sender/sponsor as owner), gas paid entirely from an
    /// address balance, and gas-data coin-reservation digests. Consumed by
    /// `check_address_balance_changes` and `check_ownership_invariants`.
    input_reservations: BTreeMap<(SuiAddress, TypeTag), u64>,
    /// For the advance-epoch transaction, `(epoch_fees minted, epoch_rebates burned)`; `None`
    /// for every other transaction. Needed by `check_sui_conserved_expensive`, which must account
    /// for the SUI the epoch change mints and burns.
    advance_epoch_gas_summary: Option<(u64, u64)>,
    /// The genesis transaction mints the initial SUI supply and so is exempt from conservation.
    is_genesis: bool,
}

/// Holds the invariant-check-only bookkeeping accumulated during execution and exposes the
/// post-execution system-invariant checks (SUI conservation, balance-accumulator authorization,
/// object ownership) as its API.
///
/// It owns the transaction-derived inputs the checks need ([`InvariantInputs`], set once up
/// front) and the PTB-emitted accumulator-event ranges accumulated during execution; the rest of
/// the data the checks need (modified objects, accumulator events, settlement SUI, gas summary,
/// ...) is read from the owning [`TemporaryStore`] passed to each check method.
pub(crate) struct InvariantChecker {
    /// Transaction-derived inputs, populated by [`Self::set_transaction_inputs`] before execution.
    inputs: InvariantInputs,
    /// Index ranges into `execution_results.accumulator_events` for events emitted from PTB
    /// (Move) execution. Each `record_ptb_event_range` call appends a contiguous range
    /// bracketing the merge. Any index outside these ranges was emitted by the runtime outside
    /// of PTB execution (currently only `gas_charger`'s `add_accumulator_event`). Consumed by
    /// `check_address_balance_changes` to gate non-PTB-emitted events behind input reservations.
    /// Cleared on `drop_writes` since the underlying events are also cleared. A `Vec` is
    /// sufficient because real transactions run the PTB at most a handful of times.
    ptb_emitted_accumulator_event_ranges: Vec<Range<usize>>,
}

impl InvariantChecker {
    pub(crate) fn new() -> Self {
        Self {
            inputs: InvariantInputs::default(),
            ptb_emitted_accumulator_event_ranges: Vec::new(),
        }
    }

    /// Derive and cache the per-transaction invariant-check inputs from the raw transaction. Must be
    /// called once, before execution, and *after* any gas-smash filtering of `gas_data`, since the
    /// reservation budget reads the final gas payment list.
    pub(crate) fn set_transaction_inputs(
        &mut self,
        transaction_kind: &TransactionKind,
        gas_data: &GasData,
        transaction_signer: SuiAddress,
    ) {
        self.inputs = InvariantInputs {
            input_reservations: compute_input_reservations(
                transaction_kind,
                gas_data,
                transaction_signer,
            ),
            advance_epoch_gas_summary: transaction_kind.get_advance_epoch_tx_gas_summary(),
            is_genesis: matches!(transaction_kind, TransactionKind::Genesis(_)),
        };
    }

    /// The funds-accumulator reservation budget for this transaction. Also consumed by
    /// `TemporaryStore::check_ownership_invariants`, which shares the same authorization model.
    pub(crate) fn input_reservations(&self) -> &BTreeMap<(SuiAddress, TypeTag), u64> {
        &self.inputs.input_reservations
    }

    /// Drop the PTB-emitted ranges. Called from `TemporaryStore::drop_writes` since the
    /// underlying accumulator events the ranges point into are cleared at the same time.
    pub(crate) fn clear(&mut self) {
        self.ptb_emitted_accumulator_event_ranges.clear();
    }

    /// Record that the accumulator events in `[start, end)` (indices into
    /// `execution_results.accumulator_events`) were emitted by PTB (Move) execution. `start`/`end`
    /// are the lengths of the accumulator-event vec before/after a `merge_results`. The
    /// address-balance change invariant uses this set to distinguish trusted PTB-emitted events
    /// from runtime-emitted ones.
    pub(crate) fn record_ptb_event_range(&mut self, start: usize, end: usize) {
        debug_assert!(
            start <= end,
            "merge_results should not shrink accumulator_events"
        );
        let (start, end) = (start.min(end), start.max(end));
        let range = start..end;
        match self.ptb_emitted_accumulator_event_ranges.last_mut() {
            // Coalesce with the previous PTB range if no runtime events were added in between.
            Some(last) if last.end == range.start => last.end = range.end,
            _ => self.ptb_emitted_accumulator_event_ranges.push(range),
        }
    }

    /// Check that this transaction neither creates nor destroys SUI. This should hold for all txes
    /// except the epoch change tx, which mints staking rewards equal to the gas fees burned in the
    /// previous epoch.  Specifically, this checks two key invariants about storage
    /// fees and storage rebate:
    ///
    /// 1. all SUI in storage rebate fields of input objects should flow either to the transaction
    ///    storage rebate, or the transaction non-refundable storage rebate
    /// 2. all SUI charged for storage should flow into the storage rebate field of some output
    ///    object
    ///
    /// This function is intended to be called *after* we have charged for
    /// gas + applied the storage rebate to the gas object, but *before* we
    /// have updated object versions.
    pub(crate) fn check_sui_conserved(
        &self,
        store: &TemporaryStore<'_>,
        simple_conservation_checks: bool,
        gas_summary: &GasCostSummary,
    ) -> Result<(), ExecutionError> {
        if !simple_conservation_checks {
            return Ok(());
        }
        // total amount of SUI in storage rebate of input objects
        let mut total_input_rebate = 0;
        // total amount of SUI in storage rebate of output objects
        let mut total_output_rebate = 0;
        for (_id, input, output) in get_modified_objects(store) {
            if let Some(input) = input {
                total_input_rebate += input.storage_rebate;
            }
            if let Some(object) = output {
                total_output_rebate += object.storage_rebate;
            }
        }

        if gas_summary.storage_cost == 0 {
            // this condition is usually true when the transaction went OOG and no
            // gas is left for storage charges.
            // The storage cost has to be there at least for the gas coin which
            // will not be deleted even when going to 0.
            // However if the storage cost is 0 and if there is any object touched
            // or deleted the value in input must be equal to the output plus rebate and
            // non refundable.
            // Rebate and non refundable will be positive when there are object deleted
            // (gas smashing being the primary and possibly only example).
            // A more typical condition is for all storage charges in summary to be 0 and
            // then input and output must be the same value
            if total_input_rebate
                != total_output_rebate
                    + gas_summary.storage_rebate
                    + gas_summary.non_refundable_storage_fee
            {
                return Err(ExecutionError::invariant_violation(format!(
                    "SUI conservation failed -- no storage charges in gas summary \
                        and total storage input rebate {} not equal  \
                        to total storage output rebate {}",
                    total_input_rebate, total_output_rebate,
                )));
            }
        } else {
            // all SUI in storage rebate fields of input objects should flow either to
            // the transaction storage rebate, or the non-refundable storage rebate pool
            if total_input_rebate
                != gas_summary.storage_rebate + gas_summary.non_refundable_storage_fee
            {
                return Err(ExecutionError::invariant_violation(format!(
                    "SUI conservation failed -- {} SUI in storage rebate field of input objects, \
                        {} SUI in tx storage rebate or tx non-refundable storage rebate",
                    total_input_rebate, gas_summary.non_refundable_storage_fee,
                )));
            }

            // all SUI charged for storage should flow into the storage rebate field
            // of some output object
            if gas_summary.storage_cost != total_output_rebate {
                return Err(ExecutionError::invariant_violation(format!(
                    "SUI conservation failed -- {} SUI charged for storage, \
                        {} SUI in storage rebate field of output objects",
                    gas_summary.storage_cost, total_output_rebate
                )));
            }
        }
        Ok(())
    }

    /// Defense-in-depth invariant on funds-accumulator events. Per `(address, type)`:
    /// - If the pair is in `input_reservations`, net withdrawal <= budget.
    /// - Else if the PTB emitted a Split at this key, we assume there must be an object withdrawal.
    ///   As such, any net change is acceptable.
    /// - Else if the PTB emitted only Merges at this key, we can assume there might not be an
    ///   object withdrawal. In any case, the net balance at the end of the transaction should be
    ///   non-negative, since there could be additional withdrawals from gas, but they should
    ///   not exceed the deposits.
    /// - Else, any event is unauthorized.
    ///
    /// Currently the only funds-accumulator type is `Balance<T>`, so the check is scoped to
    /// those events. As more accumulator shapes are added the filter and the integer
    /// arithmetic in this method will need to grow with them.
    ///
    /// PTB-emitted events are identified via `ptb_emitted_accumulator_event_ranges`, populated
    /// at `record_execution_results` time. They are trusted because Move enforces `&mut UID`
    /// and the native checks the actual balance.
    pub(crate) fn check_address_balance_changes(
        &self,
        store: &TemporaryStore<'_>,
    ) -> Result<(), ExecutionError> {
        use sui_types::balance::Balance;

        let input_reservations = &self.inputs.input_reservations;
        let mut actual_changes: BTreeMap<(SuiAddress, TypeTag), i128> = BTreeMap::new();
        let mut has_ptb_withdrawals: BTreeSet<(SuiAddress, TypeTag)> = BTreeSet::new();
        let mut has_ptb_deposits: BTreeSet<(SuiAddress, TypeTag)> = BTreeSet::new();
        for (idx, event) in store
            .execution_results
            .accumulator_events
            .iter()
            .enumerate()
        {
            // Filter on the value shape first: only `Integer` carries the funds-flow we care
            // about. Other shapes (e.g. `EventDigest` for event-stream heads) belong to
            // non-Balance accumulators and are out of scope here. If we ever see an `Integer`
            // value at a non-`Balance<T>` type, the accounting invariants below don't apply
            // -- debug_fatal so that case is surfaced instead of silently accepted.
            let amount = match event.write.value {
                AccumulatorValue::Integer(amount) => amount as i128,
                AccumulatorValue::IntegerTuple(_, _) | AccumulatorValue::EventDigest(_) => {
                    assert_invariant!(
                        !sui_types::balance::Balance::is_balance_type(&event.write.address.ty),
                        "Non-integer accumulator changes should not be balances"
                    );
                    continue;
                }
            };
            if !Balance::is_balance_type(&event.write.address.ty) {
                debug_fatal!(
                    "Integer accumulator value at non-Balance type: {:?}",
                    event.write.address.ty
                );
                continue;
            }
            let is_ptb_emitted = self
                .ptb_emitted_accumulator_event_ranges
                .iter()
                .any(|range| range.contains(&idx));
            let key = (event.write.address.address, event.write.address.ty.clone());
            let change = match event.write.operation {
                AccumulatorOperation::Split => {
                    if is_ptb_emitted {
                        has_ptb_withdrawals.insert(key.clone());
                    }
                    -amount
                }
                AccumulatorOperation::Merge => {
                    if is_ptb_emitted {
                        has_ptb_deposits.insert(key.clone());
                    }
                    amount
                }
            };
            *actual_changes.entry(key.clone()).or_insert(0) += change;
        }

        for (key, actual) in actual_changes {
            let (address, type_tag) = &key;
            if let Some(budget) = input_reservations.get(&key).copied() {
                let net_withdrawn = -actual.min(0) as u128;
                assert_invariant!(
                    net_withdrawn <= budget as u128,
                    "Balance accumulator withdrawal exceeds reservation budget at address \
                    {address} for type {type_tag}: net Split {net_withdrawn}, budget {budget}"
                );
            } else if has_ptb_withdrawals.contains(&key) {
                // Move authorized the PTB Split against the on-chain balance, so any
                // resulting net (including a net withdrawal beyond any PTB Merges here)
                // is trusted.
            } else if has_ptb_deposits.contains(&key) {
                // PTB only deposited at this key. As such, the final net change must be
                // non-negative, since there was no authorization for any withdrawal.
                // We cannot compare this value to the sum of the PTB deposits due to intricacies
                // with gas charging and storage rebate.
                assert_invariant!(
                    actual >= 0,
                    "PTB-emitted Balance accumulator deposits do not cover the runtime \
                    withdrawal at address {address} for type {type_tag}: net change {actual}"
                );
            } else {
                invariant_violation!(
                    "Unauthorized runtime Balance accumulator event at address {address} for \
                    type {type_tag}: net change {actual} (no input reservation, no PTB-emitted \
                    events)"
                );
            }
        }

        Ok(())
    }

    /// Check that this transaction neither creates nor destroys SUI.
    /// This more expensive check will check a third invariant on top of the 2 performed
    /// by `check_sui_conserved` above:
    ///
    /// * all SUI in input objects (including coins etc in the Move part of an object) should flow
    ///   either to an output object, or be burned as part of computation fees or non-refundable
    ///   storage rebate
    ///
    /// This function is intended to be called *after* we have charged for gas + applied the
    /// storage rebate to the gas object, but *before* we have updated object versions. The
    /// advance epoch transaction would mint `epoch_fees` amount of SUI, and burn `epoch_rebates`
    /// amount of SUI. We need these information for this check.
    pub(crate) fn check_sui_conserved_expensive(
        &self,
        store: &TemporaryStore<'_>,
        gas_summary: &GasCostSummary,
        layout_resolver: &mut impl LayoutResolver,
    ) -> Result<(), ExecutionError> {
        let advance_epoch_gas_summary = self.inputs.advance_epoch_gas_summary;
        // Accumulate in u128. The per-object SUI totals are bounded by the real supply, but the
        // accumulator-event terms below are not: an object-sourced withdrawal/deposit (backing
        // verified only at settlement) can contribute up to u64::MAX on each side, and a transaction
        // can stack several across distinct keys, so a u64 running total could overflow. These
        // amounts net out, so a u128 sum stays exact and conservation is decided correctly.
        // total amount of SUI in input objects, including both coins and storage rebates
        let mut total_input_sui: u128 = 0;
        // total amount of SUI in output objects, including both coins and storage rebates
        let mut total_output_sui: u128 = 0;

        // settlement input/output sui is used by the settlement transactions to account for
        // Sui that has been gathered from the accumulator writes of transactions which it is
        // settling.
        total_input_sui += store.execution_results.settlement_input_sui as u128;
        total_output_sui += store.execution_results.settlement_output_sui as u128;

        for (id, input, output) in get_modified_objects(store) {
            if let Some(input) = input {
                total_input_sui +=
                    get_input_sui(store, &id, input.version, layout_resolver)? as u128;
            }
            if let Some(object) = output {
                total_output_sui += object.get_total_sui(layout_resolver).map_err(|e| {
                    make_invariant_violation!(
                        "Failed looking up output SUI in SUI conservation checking for \
                         mutated type {:?}: {e:#?}",
                        object.struct_tag(),
                    )
                })? as u128;
            }
        }

        for event in &store.execution_results.accumulator_events {
            let (input, output) = event.total_sui_in_event();
            total_input_sui += input as u128;
            total_output_sui += output as u128;
        }

        // note: storage_cost flows into the storage_rebate field of the output objects, which is
        // why it is not accounted for here.
        // similarly, all of the storage_rebate *except* the storage_fund_rebate_inflow
        // gets credited to the gas coin both computation costs and storage rebate inflow are
        total_output_sui +=
            gas_summary.computation_cost as u128 + gas_summary.non_refundable_storage_fee as u128;
        if let Some((epoch_fees, epoch_rebates)) = advance_epoch_gas_summary {
            total_input_sui += epoch_fees as u128;
            total_output_sui += epoch_rebates as u128;
        }
        if total_input_sui != total_output_sui {
            return Err(ExecutionError::invariant_violation(format!(
                "SUI conservation failed: input={}, output={}, \
                    this transaction either mints or burns SUI",
                total_input_sui, total_output_sui,
            )));
        }
        Ok(())
    }
}

type ModifiedObjectInfo<'a> = (
    ObjectID,
    // old object metadata, including version, digest, owner, and storage rebate.
    Option<DynamicallyLoadedObjectMetadata>,
    Option<&'a Object>,
);

/// Return the list of all modified objects, for each object, returns
/// - Object ID,
/// - Input: If the object existed prior to this transaction, include their version and storage_rebate,
/// - Output: If a new version of the object is written, include the new object.
fn get_modified_objects<'a>(store: &'a TemporaryStore<'_>) -> Vec<ModifiedObjectInfo<'a>> {
    store
        .execution_results
        .modified_objects
        .iter()
        .map(|id| {
            let metadata = store.get_object_modified_at(id);
            let output = store.execution_results.written_objects.get(id);
            (*id, metadata, output)
        })
        .chain(
            store
                .execution_results
                .written_objects
                .iter()
                .filter_map(|(id, object)| {
                    if store.execution_results.modified_objects.contains(id) {
                        None
                    } else {
                        Some((*id, None, Some(object)))
                    }
                }),
        )
        .collect()
}

fn get_input_sui(
    store: &TemporaryStore<'_>,
    id: &ObjectID,
    expected_version: SequenceNumber,
    layout_resolver: &mut impl LayoutResolver,
) -> Result<u64, ExecutionError> {
    if let Some(obj) = store.input_objects.get(id) {
        // the assumption here is that if it is in the input objects must be the right one
        if obj.version() != expected_version {
            invariant_violation!(
                "Version mismatching when resolving input object to check conservation--\
                 expected {}, got {}",
                expected_version,
                obj.version(),
            );
        }
        obj.get_total_sui(layout_resolver).map_err(|e| {
            make_invariant_violation!(
                "Failed looking up input SUI in SUI conservation checking for input with \
                     type {:?}: {e:#?}",
                obj.struct_tag(),
            )
        })
    } else {
        // not in input objects, must be a dynamic field
        let Some(obj) = store.store.get_object_by_key(id, expected_version) else {
            invariant_violation!(
                "Failed looking up dynamic field {id} in SUI conservation checking"
            );
        };
        obj.get_total_sui(layout_resolver).map_err(|e| {
            make_invariant_violation!(
                "Failed looking up input SUI in SUI conservation checking for type \
                     {:?}: {e:#?}",
                obj.struct_tag(),
            )
        })
    }
}

/// Compute the per-`(address, type)` funds-accumulator reservation budget authorized by the
/// transaction. Today every funds accumulator is a `Balance<T>`, but the `(address, TypeTag)`
/// keying lets this generalize as more accumulator types are added. Sources:
/// - PTB `FundsWithdrawalArg`s for any supported accumulator type (sender or sponsor as owner).
/// - Gas paid entirely from address balance (credits `(gas_owner, Balance<SUI>)`).
/// - Gas-data entries with coin-reservation digests (also credit `(gas_owner, Balance<SUI>)`).
fn compute_input_reservations(
    transaction_kind: &TransactionKind,
    gas_data: &GasData,
    transaction_signer: SuiAddress,
) -> BTreeMap<(SuiAddress, TypeTag), u64> {
    use sui_types::balance::Balance;
    use sui_types::gas_coin::GAS;
    use sui_types::transaction::{Reservation, WithdrawFrom, is_gas_paid_from_address_balance};

    let mut reservations: BTreeMap<(SuiAddress, TypeTag), u64> = BTreeMap::new();
    let sui_balance_type = Balance::type_tag(GAS::type_tag());

    for arg in transaction_kind.get_funds_withdrawals() {
        let owner = match arg.withdraw_from {
            WithdrawFrom::Sender => transaction_signer,
            WithdrawFrom::Sponsor => gas_data.owner,
        };
        let Reservation::MaxAmountU64(reservation) = arg.reservation;
        *reservations
            .entry((owner, arg.type_arg.to_type_tag()))
            .or_insert(0) += reservation;
    }

    if is_gas_paid_from_address_balance(gas_data, transaction_kind) {
        *reservations
            .entry((gas_data.owner, sui_balance_type.clone()))
            .or_insert(0) += gas_data.budget;
    }

    for entry in &gas_data.payment {
        if let Ok(parsed) = ParsedDigest::try_from(entry.2) {
            *reservations
                .entry((gas_data.owner, sui_balance_type.clone()))
                .or_insert(0) += parsed.reservation_amount();
        }
    }

    reservations
}

impl InvariantChecker {
    /// Run the SUI-conservation and balance-accumulator invariant checks against the
    /// (already-finalized, gas-charged) `store`. Read-only: the caller (the execution engine's
    /// `run_conservation_checks`) owns any recovery that mutates state.
    ///
    /// Returns `Ok(())` when the checks are not applicable: the genesis transaction mints the SUI
    /// supply, and dev-inspect mode is allowed to violate conservation. The transaction-derived
    /// inputs the checks need (reservation budget, advance-epoch mint/burn, genesis flag) were
    /// cached up front by [`Self::set_transaction_inputs`].
    pub(crate) fn check_conservation_invariants<Mode: ExecutionMode>(
        &self,
        store: &TemporaryStore<'_>,
        move_vm: &Arc<MoveRuntime>,
        enable_expensive_checks: bool,
        cost_summary: &GasCostSummary,
    ) -> Result<(), ExecutionError> {
        if self.inputs.is_genesis || Mode::skip_conservation_checks() {
            return Ok(());
        }
        let simple_conservation_checks = store.protocol_config().simple_conservation_checks();
        self.check_sui_conserved(store, simple_conservation_checks, cost_summary)
            .and_then(|()| {
                if enable_expensive_checks {
                    let mut layout_resolver =
                        TypeLayoutResolver::new(move_vm, store.protocol_config(), Box::new(store));
                    self.check_sui_conserved_expensive(store, cost_summary, &mut layout_resolver)
                } else {
                    Ok(())
                }
            })
            .and_then(|()| self.check_address_balance_changes(store))
    }

    // check that every object read is owned directly or indirectly by sender, sponsor,
    // or a shared object input
    pub(crate) fn check_ownership_invariants(
        &self,
        store: &TemporaryStore<'_>,
        sender: &SuiAddress,
        sponsor: &Option<SuiAddress>,
        gas_charger: &GasCharger,
        mutable_inputs: &HashSet<ObjectID>,
        is_epoch_change: bool,
    ) -> SuiResult<()> {
        // The funds-accumulator reservation budget is shared with the conservation checks; see
        // `Self::check_address_balance_changes`.
        let input_reservations = self.input_reservations();
        let gas_objs: HashSet<&ObjectID> = gas_charger.used_coins().map(|g| &g.0).collect();
        let gas_owner = sponsor.as_ref().unwrap_or(sender);

        // mark input objects as authenticated
        let objects_authenticated_for_mutation: HashSet<SuiAddress> = store
            .input_objects
            .iter()
            .filter_map(|(id, obj)| {
                match &obj.owner {
                    Owner::AddressOwner(a) => {
                        if gas_objs.contains(id) {
                            // gas object must be owned by sender or sponsor
                            assert!(
                                a == gas_owner,
                                "Gas object must be owned by sender or sponsor"
                            );
                        } else {
                            assert!(sender == a, "Input object must be owned by sender");
                        }
                        Some(id)
                    }
                    Owner::Shared { .. } | Owner::ConsensusAddressOwner { .. } => Some(id),
                    Owner::Immutable => {
                        // object is authenticated, but it cannot own other objects,
                        // so we should not add it to `authenticated_objs`
                        // However, we would definitely want to add immutable objects
                        // to the set of authenticated roots if we were doing runtime
                        // checks inside the VM instead of after-the-fact in the temporary
                        // store. Here, we choose not to add them because this will catch a
                        // bug where we mutate or delete an object that belongs to an immutable
                        // object (though it will show up somewhat opaquely as an authentication
                        // failure), whereas adding the immutable object to the roots will prevent
                        // us from catching this.
                        None
                    }
                    Owner::Party { permissions, .. } => {
                        let sender_permissions = permissions.permissions_for(sender);
                        let sponsor_permissions = sponsor
                            .as_ref()
                            .map(|s| permissions.permissions_for(s))
                            .unwrap_or(ObjectPermissions::NONE);
                        (sender_permissions | sponsor_permissions)
                            .can_use_mutably()
                            .then_some(id)
                    }
                    Owner::ObjectOwner(_parent) => {
                        unreachable!(
                            "Input objects must be address owned, shared, consensus, or immutable"
                        )
                    }
                }
            })
            .filter(|id| {
                // remove any non-mutable inputs. This will remove deleted or readonly shared
                // objects
                mutable_inputs.contains(id)
            })
            .copied()
            // Add any object IDs generated in the object runtime during execution to the
            // authenticated set (i.e., new (non-package) objects, and possibly ephemeral UIDs).
            .chain(store.generated_runtime_ids.iter().copied())
            .map(SuiAddress::from)
            .collect();

        // Add sender and sponsor (if present) to authenticated set
        let mut authenticated_for_mutation = {
            assert!(
                !objects_authenticated_for_mutation.contains(sender),
                "Sender cannot be an object"
            );
            assert!(
                sponsor
                    .is_none_or(|sponsor| !objects_authenticated_for_mutation.contains(&sponsor)),
                "Sponsor cannot be an object"
            );
            let mut s = objects_authenticated_for_mutation.clone();
            s.insert(*sender);
            if let Some(sponsor) = sponsor {
                s.insert(*sponsor);
            }
            s
        };

        // check all modified objects are authenticated
        let mut objects_to_authenticate = store
            .execution_results
            .modified_objects
            .iter()
            .copied()
            .collect::<Vec<_>>();

        while let Some(to_authenticate) = objects_to_authenticate.pop() {
            if authenticated_for_mutation.contains(&to_authenticate.into()) {
                // object has already been authenticated
                continue;
            }

            let parent = if let Some(container_id) =
                store.wrapped_object_containers.get(&to_authenticate)
            {
                // It's a wrapped object, so check that the container is authenticated
                *container_id
            } else {
                // It's non-wrapped, so check the owner -- we can load the object from the
                // store.
                let Some(old_obj) = store.store.get_object(&to_authenticate) else {
                    panic!(
                        "Failed to load object {to_authenticate:?}.\n \
                         If it cannot be loaded, we would expect it to be in the wrapped object map: {:#?}",
                        &store.wrapped_object_containers
                    )
                };

                match &old_obj.owner {
                    // We mutated a dynamic field, we can continue to trace this back to verify
                    // proper ownership.
                    Owner::ObjectOwner(parent) => ObjectID::from(*parent),
                    // We mutated an address owned or sequenced address owned object -- one of two cases apply:
                    // 1) the object is owned by an object or address in the authenticated set,
                    // 2) the object is owned by some other address, in which case we should
                    //    continue to trace this back.
                    Owner::AddressOwner(parent)
                    | Owner::ConsensusAddressOwner { owner: parent, .. } => {
                        // For Receiving<_> objects, the address owner is actually an object.
                        // If it was actually an address, we should have caught it as an input and
                        // it would already have been in authenticated_for_mutation
                        ObjectID::from(*parent)
                    }
                    // We mutated a shared object -- we checked if this object was in the
                    // authenticated set at the top of this loop and it wasn't so this is a failure.
                    owner @ Owner::Shared { .. } | owner @ Owner::Party { .. } => {
                        panic!(
                            "Unauthenticated root at {to_authenticate:?} with owner {owner:?}\n\
                             Potentially covering objects in: {authenticated_for_mutation:#?}"
                        );
                    }

                    Owner::Immutable => {
                        assert!(
                            is_epoch_change,
                            "Immutable objects cannot be written, except for \
                             Sui Framework/Move stdlib upgrades at epoch change boundaries"
                        );
                        // Note: this assumes that the only immutable objects an epoch change
                        // tx can update are system packages,
                        // but in principle we could allow others.
                        assert!(
                            is_system_package(to_authenticate),
                            "Only system packages can be upgraded"
                        );
                        continue;
                    }
                }
            };

            // we now assume the object is authenticated and check the parent
            authenticated_for_mutation.insert(to_authenticate.into());
            objects_to_authenticate.push(parent);
        }

        // Check that all funds accumulator splits are authorized
        let sui_balance_type =
            sui_types::balance::Balance::type_tag(sui_types::gas_coin::GAS::type_tag());
        let gas_payment_address_balance =
            gas_charger
                .gas_payment_location()
                .and_then(|location| match location {
                    PaymentLocation::Coin(_) => None,
                    PaymentLocation::AddressBalance(address) => Some(address),
                });
        let mut funds_net_changes: BTreeMap<(SuiAddress, TypeTag), i128> = BTreeMap::new();
        for event in store.execution_results.accumulator_events.iter() {
            let amount = match event.write.value {
                AccumulatorValue::Integer(a) => a as i128,
                AccumulatorValue::IntegerTuple(_, _) | AccumulatorValue::EventDigest(_) => {
                    assert!(
                        !sui_types::balance::Balance::is_balance_type(&event.write.address.ty),
                        "Non-integer accumulator changes should not be balances"
                    );
                    continue;
                }
            };
            let signed = match event.write.operation {
                AccumulatorOperation::Split => -amount,
                AccumulatorOperation::Merge => amount,
            };
            let address = event.write.address.address;
            let type_tag = &event.write.address.ty;
            let key = (address, type_tag.clone());
            *funds_net_changes.entry(key.clone()).or_insert(0) += signed;
            // Authorized if it is:
            // - A merge/deposit (anyone can deposit)
            // - A withdrawal
            //   - with a corresponding input reservation
            //   - from an object authenticated for mutation
            //   - for the gas payment (potentially from a GasCoin send_funds transfer)
            let authorized = match event.write.operation {
                AccumulatorOperation::Merge => true,
                AccumulatorOperation::Split => {
                    input_reservations.contains_key(&key)
                        || objects_authenticated_for_mutation.contains(&address)
                        || (*type_tag == sui_balance_type
                            && gas_payment_address_balance
                                .is_some_and(|gas_addr| gas_addr == address))
                }
            };
            assert!(
                authorized,
                "Unauthenticated funds-accumulator Split at address {address} for type \
                 {type_tag}: no input reservation, address is not an authenticated object, and \
                 it is not the final gas payment address balance"
            );
        }

        // For all net negative changes (net withdrawals), the net changes _must_ be less than the
        // reservation amount, or it must be from an object. This excludes the final gas payment
        // address since the case where that withdrawal is allowed should be a net positive (or
        // zero) since it occurs only in the case where the gas coin is transferred via send_funds
        for (key, change) in funds_net_changes {
            // skip if deposit or for an object
            if change >= 0 || objects_authenticated_for_mutation.contains(&key.0) {
                continue;
            }
            let reservation = input_reservations.get(&key).copied().unwrap_or(0) as u128;
            let withdrawn = change.unsigned_abs();
            assert!(
                withdrawn <= reservation,
                "Net withdrawal of {withdrawn} for {key:?} exceeds input reservation of \
                 {reservation}"
            );
        }

        Ok(())
    }
}
