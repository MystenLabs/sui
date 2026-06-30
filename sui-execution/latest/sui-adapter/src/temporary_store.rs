// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_mode::ExecutionMode;
use crate::gas_charger::{GasCharger, PaymentLocation};
use move_vm_runtime::runtime::MoveRuntime;
use mysten_common::{ZipDebugEqIteratorExt, debug_fatal};
use mysten_metrics::monitored_scope;
use parking_lot::RwLock;
use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;
use sui_types::accumulator_event::AccumulatorEvent;
use sui_types::accumulator_root::AccumulatorObjId;
use sui_types::base_types::VersionDigest;
use sui_types::committee::EpochId;
use sui_types::deny_list_v2::check_coin_deny_list_v2_during_execution;
use sui_types::effects::{
    AccumulatorOperation, AccumulatorValue, AccumulatorWriteV1, TransactionEffects,
    TransactionEffectsV2, TransactionEvents,
};
use sui_types::execution::{
    DynamicallyLoadedObjectMetadata, ExecutionResults, ExecutionResultsV2, ExecutionRetryError,
    SharedInput,
};
use sui_types::execution_status::{ExecutionErrorKind, ExecutionStatus};
use sui_types::inner_temporary_store::InnerTemporaryStore;
use sui_types::object::Data;
use sui_types::storage::{BackingStore, DenyListResult, PackageObject};
use sui_types::sui_system_state::{AdvanceEpochParams, get_sui_system_state_wrapper};
use sui_types::transaction::{GasData, TransactionKind};
use sui_types::{
    SUI_DENY_LIST_OBJECT_ID,
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest},
    digests::ObjectDigest,
    effects::EffectsObjectChange,
    error::{ExecutionError, SuiErrorKind, SuiResult},
    gas::GasCostSummary,
    object::Object,
    object::Owner,
    storage::{BackingPackageStore, RuntimeObjectResolver, Storage},
    transaction::InputObjects,
};
use sui_types::{SUI_SYSTEM_STATE_OBJECT_ID, TypeTag, is_system_package};

pub(crate) mod invariants;
use invariants::InvariantChecker;

pub struct TemporaryStore<'backing> {
    // The backing store for retrieving Move packages onchain.
    // When executing a Move call, the dependent packages are not going to be
    // in the input objects. They will be fetched from the backing store.
    // Also used for fetching the backing parent_sync to get the last known version for wrapped
    // objects
    store: &'backing dyn BackingStore,
    tx_digest: TransactionDigest,
    input_objects: BTreeMap<ObjectID, Object>,

    /// Store the original versions of the non-exclusive write inputs, in order to detect
    /// mutations (which are illegal, but not prevented by the type system).
    non_exclusive_input_original_versions: BTreeMap<ObjectID, Object>,

    stream_ended_consensus_objects: BTreeMap<ObjectID, SequenceNumber /* start_version */>,
    /// The version to assign to all objects written by the transaction using this store.
    lamport_timestamp: SequenceNumber,
    /// Inputs that will be mutated by the transaction. Does not include NonExclusiveWrite inputs,
    /// which can be taken as `&mut T` but cannot be directly mutated.
    mutable_input_refs: BTreeMap<ObjectID, (VersionDigest, Owner)>,
    execution_results: ExecutionResultsV2,
    /// Objects that were loaded during execution (dynamic fields + received objects).
    loaded_runtime_objects: BTreeMap<ObjectID, DynamicallyLoadedObjectMetadata>,
    /// A map from wrapped object to its container. Used during expensive invariant checks.
    wrapped_object_containers: BTreeMap<ObjectID, ObjectID>,
    protocol_config: &'backing ProtocolConfig,

    /// Every package that was loaded from DB store during execution.
    /// These packages were not previously loaded into the temporary store.
    runtime_packages_loaded_from_db: RwLock<BTreeMap<ObjectID, PackageObject>>,

    /// The set of objects that we may receive during execution. Not guaranteed to receive all, or
    /// any of the objects referenced in this set.
    receiving_objects: Vec<ObjectRef>,

    /// The set of all generated object IDs from the object runtime during the transaction. This includes any
    /// created-and-then-deleted objects in addition to any `new_ids` which contains only the set
    /// of created (but not deleted) IDs in the transaction.
    generated_runtime_ids: BTreeSet<ObjectID>,

    // TODO: Now that we track epoch here, there are a few places we don't need to pass it around.
    /// The current epoch.
    cur_epoch: EpochId,

    /// The set of per-epoch config objects that were loaded during execution, and are not in the
    /// input objects. This allows us to commit them to the effects.
    loaded_per_epoch_config_objects: RwLock<BTreeSet<ObjectID>>,

    /// Transaction-derived inputs and bookkeeping for the post-execution system-invariant checks
    /// (SUI conservation, balance-accumulator authorization, object ownership). See
    /// [`invariants::InvariantChecker`].
    invariants: InvariantChecker,

    /// Versions of system objects this transaction is allowed to read, keyed by object ID. A
    /// system object is considered "available" once its latest committed version has reached the
    /// recorded version; `is_system_object_available` consults this map. Every system object read
    /// during execution must appear here — querying one that is absent is an invariant violation
    /// (the transaction was not sequenced against it), so the check errors rather than allowing it.
    system_object_versions: BTreeMap<ObjectID, SequenceNumber>,

    /// System objects read during execution that are not through input objects, keyed by object ID, with the version (and its
    /// digest) at which they were read. Recorded by `is_system_object_available` and
    /// emitted into the transaction effects as read-only consensus objects so the read can be
    /// reproduced on replay. Interior-mutable because reads happen behind `&self`
    /// (`RuntimeObjectResolver`).
    loaded_system_objects: RefCell<BTreeMap<ObjectID, (SequenceNumber, ObjectDigest)>>,

    /// Recorded when execution determines the transaction must be retried later rather than
    /// committed. Execution still runs to completion (the triggering native also raises a Move
    /// error); this signal is carried out on `InnerTemporaryStore` so the authority can discard the
    /// effects and re-enqueue. A `OnceCell` rather than a lock: execution is single-threaded, and the
    /// condition is detected behind `&self` (`RuntimeObjectResolver`), so the field must be
    /// interior-mutable; it is recorded at most once (the first detection, after which execution
    /// aborts), which `OnceCell` enforces.
    retry_request: OnceCell<ExecutionRetryError>,
}

impl<'backing> TemporaryStore<'backing> {
    /// Creates a new store associated with an authority store, and populates it with
    /// initial objects.
    pub fn new(
        store: &'backing dyn BackingStore,
        input_objects: InputObjects,
        receiving_objects: Vec<ObjectRef>,
        tx_digest: TransactionDigest,
        protocol_config: &'backing ProtocolConfig,
        cur_epoch: EpochId,
        system_object_versions: BTreeMap<ObjectID, SequenceNumber>,
    ) -> Self {
        let mutable_input_refs = input_objects.exclusive_mutable_inputs();
        let non_exclusive_input_original_versions = input_objects.non_exclusive_input_objects();

        let lamport_timestamp = input_objects.lamport_timestamp(&receiving_objects);
        let stream_ended_consensus_objects = input_objects.consensus_stream_ended_objects();
        let objects = input_objects.into_object_map();
        #[cfg(debug_assertions)]
        {
            // Ensure that input objects and receiving objects must not overlap.
            assert!(
                objects
                    .keys()
                    .collect::<HashSet<_>>()
                    .intersection(
                        &receiving_objects
                            .iter()
                            .map(|oref| &oref.0)
                            .collect::<HashSet<_>>()
                    )
                    .next()
                    .is_none()
            );
        }
        Self {
            store,
            tx_digest,
            input_objects: objects,
            non_exclusive_input_original_versions,
            stream_ended_consensus_objects,
            lamport_timestamp,
            mutable_input_refs,
            execution_results: ExecutionResultsV2::default(),
            protocol_config,
            loaded_runtime_objects: BTreeMap::new(),
            wrapped_object_containers: BTreeMap::new(),
            runtime_packages_loaded_from_db: RwLock::new(BTreeMap::new()),
            receiving_objects,
            generated_runtime_ids: BTreeSet::new(),
            cur_epoch,
            loaded_per_epoch_config_objects: RwLock::new(BTreeSet::new()),
            invariants: InvariantChecker::new(),
            system_object_versions,
            loaded_system_objects: RefCell::new(BTreeMap::new()),
            retry_request: OnceCell::new(),
        }
    }

    /// Reports whether the system object `object_id` is available at the version this transaction
    /// requires, i.e. its latest committed version has caught up to that version. The temporary
    /// store knows the required versions (`system_object_versions`) and errors if `object_id` has
    /// none — reading a system object the transaction was not sequenced against is an invariant
    /// violation. Called directly on the store during execution rather than through a resolver
    /// trait.
    pub fn is_system_object_available(&self, object_id: &ObjectID) -> SuiResult<bool> {
        // Every system object read during execution must have an assigned version. Its absence
        // here means the transaction is reading a system object it was not sequenced against,
        // which is an invariant violation.
        let Some(required_version) = self.system_object_versions.get(object_id).copied() else {
            debug_fatal!("system object {object_id} read without an assigned version");
            return Err(SuiErrorKind::GenericAuthorityError {
                error: format!("system object {object_id} read without an assigned version"),
            }
            .into());
        };
        // Load the object at exactly the version this transaction was sequenced against.
        // `required_version` is the freshly-assigned version at the frontier, so it is never pruned:
        // its absence means the local node has not yet committed that version.
        let Some(object_at_required) = self.store.get_object_by_key(object_id, required_version)
        else {
            // Not yet caught up to the version this transaction requires: mark the transaction for
            // retry. The retry is signaled out-of-band via this interior-mutable state (the Move VM
            // boundary can't carry it), and surfaces as `ExecutionRetryError` on the inner temporary
            // store. The authority then waits for `object_id` to reach `required_version` and
            // re-enqueues; it recovers the object's initial shared version from the epoch start
            // config, so the id and version carried here are enough.
            // First detection wins; a second would only be recorded if execution continued past the
            // abort below, which it does not.
            let _ = self
                .retry_request
                .set(ExecutionRetryError::SystemObjectUnavailable {
                    object_id: *object_id,
                    version: required_version,
                });
            return Ok(false);
        };

        // Available: record the read at `required_version` (which is what the transaction depends
        // on and reads) so it can be emitted into effects as a read-only consensus object and
        // reproduced on replay. The version and digest are taken at `required_version` — not the
        // latest — so the recorded value is deterministic across nodes regardless of how far the
        // object has since advanced.
        self.loaded_system_objects
            .borrow_mut()
            .insert(*object_id, (required_version, object_at_required.digest()));
        Ok(true)
    }

    // Helpers to access private fields
    pub fn objects(&self) -> &BTreeMap<ObjectID, Object> {
        &self.input_objects
    }

    pub fn update_object_version_and_prev_tx(&mut self) {
        self.execution_results.update_version_and_previous_tx(
            self.lamport_timestamp,
            self.tx_digest,
            &self.input_objects,
            self.protocol_config.reshare_at_same_initial_version(),
        );

        #[cfg(debug_assertions)]
        {
            self.check_invariants();
        }
    }

    fn calculate_accumulator_running_max_withdraws(&self) -> BTreeMap<AccumulatorObjId, u128> {
        let mut running_net_withdraws: BTreeMap<AccumulatorObjId, i128> = BTreeMap::new();
        let mut running_max_withdraws: BTreeMap<AccumulatorObjId, u128> = BTreeMap::new();
        for event in &self.execution_results.accumulator_events {
            match &event.write.value {
                AccumulatorValue::Integer(amount) => match event.write.operation {
                    AccumulatorOperation::Split => {
                        let entry = running_net_withdraws
                            .entry(event.accumulator_obj)
                            .or_default();
                        *entry += *amount as i128;
                        if *entry > 0 {
                            let max_entry = running_max_withdraws
                                .entry(event.accumulator_obj)
                                .or_default();
                            *max_entry = (*max_entry).max(*entry as u128);
                        }
                    }
                    AccumulatorOperation::Merge => {
                        let entry = running_net_withdraws
                            .entry(event.accumulator_obj)
                            .or_default();
                        *entry -= *amount as i128;
                    }
                },
                AccumulatorValue::IntegerTuple(_, _) | AccumulatorValue::EventDigest(_) => {}
            }
        }
        running_max_withdraws
    }

    /// Ensure that, per accumulator object, the gross Merge total and gross Split total are
    /// representable: bounded by the total SUI supply for `Balance<SUI>` keys, and by `u64::MAX`
    /// otherwise.
    ///
    /// `AccumulatorWriteV1::merge` folds all writes for a key by summing Merge amounts and Split
    /// amounts separately into `u64`s. The object runtime caps Move-native merges per key at
    /// `u64::MAX`, but the gas charger emits additional, uncapped SUI deposit/withdraw events during
    /// gas smashing and gas charging (e.g. a refund Merge to an address balance), so a per-key SUI
    /// total could be pushed past `u64::MAX`, overflowing that fold (and the SUI-conservation sum).
    /// Reaching such a total requires SUI from an object-sourced withdrawal whose backing is only
    /// verified at settlement.
    ///
    /// Bounding SUI to `TOTAL_SUPPLY_MIST` rejects any such amount here, *before* gas is charged, so
    /// the rejected PTB-emitted writes are dropped on gas reset and only the (bounded) gas events
    /// remain. Crucially, `TOTAL_SUPPLY_MIST` is ~8.4B SUI below `u64::MAX`, so the gas events emitted
    /// after this check (which move only real SUI) cannot push any per-key total past `u64::MAX` —
    /// hence they need not be re-checked. Non-SUI balances have no uncapped gas path, so the
    /// object-runtime per-key `u64::MAX` cap is the binding guard there and we only backstop u64
    /// representability.
    ///
    /// The per-key limits are not sufficient on their own: withdrawn SUI can be spread across several
    /// object keys (each withdrawal `<= TOTAL_SUPPLY_MIST`) and then recombined *outside* the
    /// accumulator — e.g. each withdrawal redeemed to a `Coin<SUI>` and merged into the PTB gas coin
    /// via `MergeCoins`, which is an object mutation, not an accumulator event. The recombined coin
    /// can then reach `u64::MAX` and overflow `deduct_gas` on a refund. So we also bound the
    /// *cross-key* total SUI withdrawn (gross Split) to the supply, capping the total SUI a single
    /// transaction can withdraw regardless of how it is later recombined.
    pub(crate) fn check_accumulator_amounts_representable(&self) -> Result<(), ExecutionError> {
        let supply = sui_types::gas_coin::TOTAL_SUPPLY_MIST as u128;
        let mut merge_totals: BTreeMap<AccumulatorObjId, u128> = BTreeMap::new();
        let mut split_totals: BTreeMap<AccumulatorObjId, u128> = BTreeMap::new();
        // Cross-key total of SUI withdrawn (gross Split), bounded to the supply (see above).
        let mut total_sui_split: u128 = 0;
        for event in &self.execution_results.accumulator_events {
            let AccumulatorValue::Integer(amount) = event.write.value else {
                continue;
            };
            let amount = amount as u128;
            // SUI cannot exceed its total supply through any single balance. Bounding to the supply
            // (rather than u64::MAX) leaves headroom for the not-yet-emitted gas events.
            let is_sui = sui_types::gas_coin::GasCoin::is_gas_balance_type(&event.write.address.ty);
            let limit = if is_sui { supply } else { u64::MAX as u128 };
            let total = match event.write.operation {
                AccumulatorOperation::Merge => {
                    merge_totals.entry(event.accumulator_obj).or_default()
                }
                AccumulatorOperation::Split => {
                    split_totals.entry(event.accumulator_obj).or_default()
                }
            };
            *total += amount;
            if *total > limit {
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::CoinBalanceOverflow,
                    format!(
                        "accumulator balance change for {:?} exceeds the representable limit \
                         (gross total {}, limit {})",
                        event.accumulator_obj, *total, limit
                    ),
                ));
            }
            if is_sui && matches!(event.write.operation, AccumulatorOperation::Split) {
                total_sui_split += amount;
                if total_sui_split > supply {
                    return Err(ExecutionError::new_with_source(
                        ExecutionErrorKind::CoinBalanceOverflow,
                        format!(
                            "total SUI withdrawn across all accumulators ({total_sui_split}) \
                             exceeds the total supply ({supply})"
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Ensure that there is one entry for each accumulator object in the accumulator events.
    fn merge_accumulator_events(&mut self) {
        self.execution_results.accumulator_events = self
            .execution_results
            .accumulator_events
            .iter()
            .fold(
                BTreeMap::<AccumulatorObjId, Vec<AccumulatorWriteV1>>::new(),
                |mut map, event| {
                    map.entry(event.accumulator_obj)
                        .or_default()
                        .push(event.write.clone());
                    map
                },
            )
            .into_iter()
            .map(|(obj_id, writes)| {
                AccumulatorEvent::new(obj_id, AccumulatorWriteV1::merge(writes))
            })
            .collect();
    }

    /// Break up the structure and return its internal stores (objects, active_inputs, written, deleted)
    pub fn into_inner(
        self,
        accumulator_running_max_withdraws: BTreeMap<AccumulatorObjId, u128>,
    ) -> InnerTemporaryStore {
        let results = self.execution_results;
        InnerTemporaryStore {
            input_objects: self.input_objects,
            stream_ended_consensus_objects: self.stream_ended_consensus_objects,
            mutable_inputs: self.mutable_input_refs,
            written: results.written_objects,
            events: TransactionEvents {
                data: results.user_events,
            },
            accumulator_events: results.accumulator_events,
            loaded_runtime_objects: self.loaded_runtime_objects,
            runtime_packages_loaded_from_db: self.runtime_packages_loaded_from_db.into_inner(),
            lamport_version: self.lamport_timestamp,
            binary_config: self.protocol_config.binary_config(None),
            accumulator_running_max_withdraws,
            retry_request: self.retry_request.into_inner(),
        }
    }

    /// For every object from active_inputs (i.e. all mutable objects), if they are not
    /// mutated during the transaction execution, force mutating them by incrementing the
    /// sequence number. This is required to achieve safety.
    pub(crate) fn ensure_active_inputs_mutated(&mut self) {
        let mut to_be_updated = vec![];
        // Note: we do not mutate input objects if they are non-exclusive write
        for id in self.mutable_input_refs.keys() {
            if !self.execution_results.modified_objects.contains(id) {
                // We cannot update here but have to push to `to_be_updated` and update later
                // because the for loop is holding a reference to `self`, and calling
                // `self.mutate_input_object` requires a mutable reference to `self`.
                to_be_updated.push(self.input_objects[id].clone());
            }
        }
        for object in to_be_updated {
            // The object must be mutated as it was present in the input objects
            self.mutate_input_object(object.clone());
        }
    }

    fn get_object_changes(&self) -> BTreeMap<ObjectID, EffectsObjectChange> {
        let results = &self.execution_results;
        let all_ids = results
            .created_object_ids
            .iter()
            .chain(&results.deleted_object_ids)
            .chain(&results.modified_objects)
            .chain(results.written_objects.keys())
            .collect::<BTreeSet<_>>();
        all_ids
            .into_iter()
            .map(|id| {
                (
                    *id,
                    EffectsObjectChange::new(
                        self.get_object_modified_at(id)
                            .map(|metadata| ((metadata.version, metadata.digest), metadata.owner)),
                        results.written_objects.get(id),
                        results.created_object_ids.contains(id),
                        results.deleted_object_ids.contains(id),
                    ),
                )
            })
            .chain(results.accumulator_events.iter().cloned().map(
                |AccumulatorEvent {
                     accumulator_obj,
                     write,
                 }| {
                    (
                        *accumulator_obj.inner(),
                        EffectsObjectChange::new_from_accumulator_write(write),
                    )
                },
            ))
            .collect()
    }

    pub fn into_effects(
        mut self,
        shared_object_refs: Vec<SharedInput>,
        transaction_digest: &TransactionDigest,
        mut transaction_dependencies: BTreeSet<TransactionDigest>,
        gas_cost_summary: GasCostSummary,
        status: ExecutionStatus,
        gas_charger: &mut GasCharger,
        epoch: EpochId,
    ) -> (InnerTemporaryStore, TransactionEffects) {
        // Defense-in-depth: Owner::Party is not yet supported as an effect output. There are
        // no constructions of `Owner::Party` yet so a hard assert should be safe.
        for (id, obj) in &self.execution_results.written_objects {
            assert!(
                !matches!(obj.owner, Owner::Party { .. }),
                "Party-owned objects are not yet supported (object {id})"
            );
        }

        self.update_object_version_and_prev_tx();
        // This must happens before merge_accumulator_events.
        let accumulator_running_max_withdraws = self.calculate_accumulator_running_max_withdraws();
        self.merge_accumulator_events();

        // Regardless of execution status (including aborts), we insert the previous transaction
        // for any successfully received objects during the transaction.
        for (id, expected_version, expected_digest) in &self.receiving_objects {
            // If the receiving object is in the loaded runtime objects, then that means that it
            // was actually successfully loaded (so existed, and there was authenticated mutable
            // access to it). So we insert the previous transaction as a dependency.
            if let Some(obj_meta) = self.loaded_runtime_objects.get(id) {
                // Check that the expected version, digest, and owner match the loaded version,
                // digest, and owner. If they don't then don't register a dependency.
                // This is because this could be "spoofed" by loading a dynamic object field.
                let loaded_via_receive = obj_meta.version == *expected_version
                    && obj_meta.digest == *expected_digest
                    && obj_meta.owner.is_address_owned();
                if loaded_via_receive {
                    transaction_dependencies.insert(obj_meta.previous_transaction);
                }
            }
        }

        assert!(self.protocol_config.enable_effects_v2());

        // In the case of special transactions that don't require a gas object,
        // we don't really care about the effects to gas, just use the input for it.
        // Gas coins are guaranteed to be at least size 1 and if more than 1
        // the first coin is where all the others are merged.
        let gas_coin = gas_charger
            .gas_payment_amount()
            .and_then(|gp| match gp.location {
                PaymentLocation::Coin(coin_id) => Some(coin_id),
                PaymentLocation::AddressBalance(_) => None,
            });

        let object_changes = self.get_object_changes();

        let lamport_version = self.lamport_timestamp;
        // TODO: Cleanup this clone. Potentially add unchanged_shraed_objects directly to InnerTempStore.
        let loaded_per_epoch_config_objects = self.loaded_per_epoch_config_objects.read().clone();
        let unchanged_consensus_objects = TransactionEffectsV2::compute_unchanged_consensus_objects(
            shared_object_refs,
            loaded_per_epoch_config_objects,
            &object_changes,
        );
        let inner = self.into_inner(accumulator_running_max_withdraws);

        let effects = TransactionEffects::new_from_execution_v2(
            status,
            epoch,
            gas_cost_summary,
            unchanged_consensus_objects,
            *transaction_digest,
            lamport_version,
            object_changes,
            gas_coin,
            if inner.events.data.is_empty() {
                None
            } else {
                Some(inner.events.digest())
            },
            transaction_dependencies.into_iter().collect(),
        );

        (inner, effects)
    }

    /// An internal check of the invariants (will only fire in debug)
    #[cfg(debug_assertions)]
    fn check_invariants(&self) {
        // Check not both deleted and written
        debug_assert!(
            {
                self.execution_results
                    .written_objects
                    .keys()
                    .all(|id| !self.execution_results.deleted_object_ids.contains(id))
            },
            "Object both written and deleted."
        );

        // Check all mutable inputs are modified
        debug_assert!(
            {
                self.mutable_input_refs
                    .keys()
                    .all(|id| self.execution_results.modified_objects.contains(id))
            },
            "Mutable input not modified."
        );

        debug_assert!(
            {
                self.execution_results
                    .written_objects
                    .values()
                    .all(|obj| obj.previous_transaction == self.tx_digest)
            },
            "Object previous transaction not properly set",
        );
    }

    /// Mutate a mutable input object. This is used to mutate input objects outside of PT execution.
    pub fn mutate_input_object(&mut self, object: Object) {
        let id = object.id();
        debug_assert!(self.input_objects.contains_key(&id));
        debug_assert!(!object.is_immutable());
        self.execution_results.modified_objects.insert(id);
        self.execution_results.written_objects.insert(id, object);
    }

    pub fn mutate_new_or_input_object(&mut self, object: Object) {
        let id = object.id();
        debug_assert!(!object.is_immutable());
        if self.input_objects.contains_key(&id) {
            self.execution_results.modified_objects.insert(id);
        }
        self.execution_results.written_objects.insert(id, object);
    }

    /// Mutate a child object outside of PT. This should be used extremely rarely.
    /// Currently it's only used by advance_epoch_safe_mode because it's all native
    /// without PT. This should almost never be used otherwise.
    pub fn mutate_child_object(&mut self, old_object: Object, new_object: Object) {
        let id = new_object.id();
        let old_ref = old_object.compute_object_reference();
        debug_assert_eq!(old_ref.0, id);
        self.loaded_runtime_objects.insert(
            id,
            DynamicallyLoadedObjectMetadata {
                version: old_ref.1,
                digest: old_ref.2,
                owner: old_object.owner.clone(),
                storage_rebate: old_object.storage_rebate,
                previous_transaction: old_object.previous_transaction,
            },
        );
        self.execution_results.modified_objects.insert(id);
        self.execution_results
            .written_objects
            .insert(id, new_object);
    }

    /// Upgrade system package during epoch change. This requires special treatment
    /// since the system package to be upgraded is not in the input objects.
    /// We could probably fix above to make it less special.
    pub fn upgrade_system_package(&mut self, package: Object) {
        let id = package.id();
        assert!(package.is_package() && is_system_package(id));
        self.execution_results.modified_objects.insert(id);
        self.execution_results.written_objects.insert(id, package);
    }

    /// Crate a new objcet. This is used to create objects outside of PT execution.
    pub fn create_object(&mut self, object: Object) {
        // Created mutable objects' versions are set to the store's lamport timestamp when it is
        // committed to effects. Creating an object at a non-zero version risks violating the
        // lamport timestamp invariant (that a transaction's lamport timestamp is strictly greater
        // than all versions witnessed by the transaction).
        debug_assert!(
            object.is_immutable() || object.version() == SequenceNumber::MIN,
            "Created mutable objects should not have a version set",
        );
        let id = object.id();
        self.execution_results.created_object_ids.insert(id);
        self.execution_results.written_objects.insert(id, object);
    }

    /// Delete a mutable input object. This is used to delete input objects outside of PT execution.
    pub fn delete_input_object(&mut self, id: &ObjectID) {
        // there should be no deletion after write
        debug_assert!(!self.execution_results.written_objects.contains_key(id));
        debug_assert!(self.input_objects.contains_key(id));
        self.execution_results.modified_objects.insert(*id);
        self.execution_results.deleted_object_ids.insert(*id);
    }

    pub fn drop_writes(&mut self) {
        self.execution_results.drop_writes();
        // The PTB-emitted ranges pointed into the now-cleared accumulator_events vec.
        self.invariants.clear();
    }

    pub fn read_object(&self, id: &ObjectID) -> Option<&Object> {
        // there should be no read after delete
        debug_assert!(!self.execution_results.deleted_object_ids.contains(id));
        self.execution_results
            .written_objects
            .get(id)
            .or_else(|| self.input_objects.get(id))
    }

    pub fn save_loaded_runtime_objects(
        &mut self,
        loaded_runtime_objects: BTreeMap<ObjectID, DynamicallyLoadedObjectMetadata>,
    ) {
        #[cfg(debug_assertions)]
        {
            for (id, v1) in &loaded_runtime_objects {
                if let Some(v2) = self.loaded_runtime_objects.get(id) {
                    assert_eq!(v1, v2);
                }
            }
            for (id, v1) in &self.loaded_runtime_objects {
                if let Some(v2) = loaded_runtime_objects.get(id) {
                    assert_eq!(v1, v2);
                }
            }
        }
        // Merge the two maps because we may be calling the execution engine more than once
        // (e.g. in advance epoch transaction, where we may be publishing a new system package).
        self.loaded_runtime_objects.extend(loaded_runtime_objects);
    }

    pub fn save_wrapped_object_containers(
        &mut self,
        wrapped_object_containers: BTreeMap<ObjectID, ObjectID>,
    ) {
        #[cfg(debug_assertions)]
        {
            for (id, container1) in &wrapped_object_containers {
                if let Some(container2) = self.wrapped_object_containers.get(id) {
                    assert_eq!(container1, container2);
                }
            }
            for (id, container1) in &self.wrapped_object_containers {
                if let Some(container2) = wrapped_object_containers.get(id) {
                    assert_eq!(container1, container2);
                }
            }
        }
        // Merge the two maps because we may be calling the execution engine more than once
        // (e.g. in advance epoch transaction, where we may be publishing a new system package).
        self.wrapped_object_containers
            .extend(wrapped_object_containers);
    }

    pub fn save_generated_object_ids(&mut self, generated_ids: BTreeSet<ObjectID>) {
        #[cfg(debug_assertions)]
        {
            for id in &self.generated_runtime_ids {
                assert!(!generated_ids.contains(id))
            }
            for id in &generated_ids {
                assert!(!self.generated_runtime_ids.contains(id));
            }
        }
        self.generated_runtime_ids.extend(generated_ids);
    }

    pub fn estimate_effects_size_upperbound(&self) -> usize {
        TransactionEffects::estimate_effects_size_upperbound_v2(
            self.execution_results.written_objects.len(),
            self.execution_results.modified_objects.len(),
            self.input_objects.len(),
        )
    }

    pub fn written_objects_size(&self) -> usize {
        self.execution_results
            .written_objects
            .values()
            .fold(0, |sum, obj| sum + obj.object_size_for_gas_metering())
    }

    /// Validates gasless post-execution invariants:
    /// - No new objects were created or existing objects mutated (written_objects is empty)
    /// - The set of deleted objects exactly equals the set of input Coin objects
    /// - Each recipient receives at least the minimum transfer amount per token type
    /// - Unused withdrawal reservation (reservation - actual split) is 0 or >= min_amount
    pub fn check_gasless_execution_requirements(
        &self,
        withdrawal_reservations: Option<&BTreeMap<(SuiAddress, TypeTag), u64>>,
    ) -> Result<(), String> {
        if !self.execution_results.written_objects.is_empty() {
            return Err("Gasless transactions cannot create or mutate objects".to_string());
        }

        let input_coin_ids: BTreeSet<ObjectID> = self
            .input_objects
            .iter()
            .filter(|(_, obj)| obj.coin_type_maybe().is_some())
            .map(|(id, _)| *id)
            .collect();
        if self.execution_results.deleted_object_ids != input_coin_ids {
            return Err(format!(
                "Gasless transaction must destroy exactly its input Coins. \
                 Expected: {input_coin_ids:?}, deleted: {:?}",
                self.execution_results.deleted_object_ids
            ));
        }

        let allowed_types =
            sui_types::transaction::get_gasless_allowed_token_types(self.protocol_config);

        // Aggregate signed balance changes per (address, token_type).
        // Positive nets are recipient deposits that must meet the minimum transfer amount.
        let net_totals = sui_types::balance_change::signed_balance_changes_from_events(
            &self.execution_results.accumulator_events,
        )
        .fold(
            BTreeMap::<(SuiAddress, TypeTag), i128>::new(),
            |mut totals, (address, token_type, signed_amount)| {
                *totals.entry((address, token_type)).or_default() += signed_amount;
                totals
            },
        );

        for ((recipient, token_type), net_amount) in &net_totals {
            if *net_amount <= 0 {
                continue;
            }
            if let Some(&min_amount) = allowed_types.get(token_type)
                && *net_amount < i128::from(min_amount)
            {
                return Err(format!(
                    "Gasless transfer of {net_amount} to {recipient} is below \
                     minimum {min_amount} for token type {token_type}"
                ));
            }
        }

        if let Some(reservations) = withdrawal_reservations {
            for ((owner, token_type), &reserved) in reservations {
                let net = net_totals
                    .get(&(*owner, token_type.clone()))
                    .copied()
                    .unwrap_or(0);
                let remaining = (reserved as i128).saturating_add(net);
                if remaining > 0
                    && let Some(&min_balance_remaining) = allowed_types.get(token_type)
                    && min_balance_remaining > 0
                    && remaining < min_balance_remaining as i128
                {
                    return Err(format!(
                        "Gasless withdrawal leaves {remaining} unused for {owner}, \
                         below minimum {min_balance_remaining} for token type {token_type}"
                    ));
                }
            }
        }

        Ok(())
    }

    /// If there are unmetered storage rebate (due to system transaction), we put them into
    /// the storage rebate of 0x5 object.
    /// TODO: This will not work for potential future new system transactions if 0x5 is not in the input.
    /// We should fix this.
    pub fn conserve_unmetered_storage_rebate(&mut self, unmetered_storage_rebate: u64) {
        if unmetered_storage_rebate == 0 {
            // If unmetered_storage_rebate is 0, we are most likely executing the genesis transaction.
            // And in that case we cannot mutate the 0x5 object because it's newly created.
            // And there is no storage rebate that needs distribution anyway.
            return;
        }
        tracing::debug!(
            "Amount of unmetered storage rebate from system tx: {:?}",
            unmetered_storage_rebate
        );
        let mut system_state_wrapper = self
            .read_object(&SUI_SYSTEM_STATE_OBJECT_ID)
            .expect("0x5 object must be mutated in system tx with unmetered storage rebate")
            .clone();
        // In unmetered execution, storage_rebate field of mutated object must be 0.
        // If not, we would be dropping SUI on the floor by overriding it.
        assert_eq!(system_state_wrapper.storage_rebate, 0);
        system_state_wrapper.storage_rebate = unmetered_storage_rebate;
        self.mutate_input_object(system_state_wrapper);
    }

    /// Add an accumulator event to the execution results.
    pub fn add_accumulator_event(&mut self, event: AccumulatorEvent) {
        self.execution_results.accumulator_events.push(event);
    }

    /// Given an object ID, if it's not modified, returns None.
    /// Otherwise returns its metadata, including version, digest, owner and storage rebate.
    /// A modified object must be either a mutable input, or a loaded child object.
    /// The only exception is when we upgrade system packages, in which case the upgraded
    /// system packages are not part of input, but are modified.
    fn get_object_modified_at(
        &self,
        object_id: &ObjectID,
    ) -> Option<DynamicallyLoadedObjectMetadata> {
        if self.execution_results.modified_objects.contains(object_id) {
            Some(
                self.mutable_input_refs
                    .get(object_id)
                    .map(
                        |((version, digest), owner)| DynamicallyLoadedObjectMetadata {
                            version: *version,
                            digest: *digest,
                            owner: owner.clone(),
                            // It's guaranteed that a mutable input object is an input object.
                            storage_rebate: self.input_objects[object_id].storage_rebate,
                            previous_transaction: self.input_objects[object_id]
                                .previous_transaction,
                        },
                    )
                    .or_else(|| self.loaded_runtime_objects.get(object_id).cloned())
                    .unwrap_or_else(|| {
                        debug_assert!(is_system_package(*object_id));
                        let package_obj =
                            self.store.get_package_object(object_id).unwrap().unwrap();
                        let obj = package_obj.object();
                        DynamicallyLoadedObjectMetadata {
                            version: obj.version(),
                            digest: obj.digest(),
                            owner: obj.owner.clone(),
                            storage_rebate: obj.storage_rebate,
                            previous_transaction: obj.previous_transaction,
                        }
                    }),
            )
        } else {
            None
        }
    }

    pub fn protocol_config(&self) -> &'backing ProtocolConfig {
        self.protocol_config
    }

    /// Cache the transaction-derived inputs the system-invariant checks need (consumed by both the
    /// conservation checks and the ownership-invariant check). Must be called once, before
    /// execution, after any gas-smash filtering of `gas_data`.
    /// See [`invariants::InvariantChecker::set_transaction_inputs`].
    pub(crate) fn set_invariant_inputs(
        &mut self,
        transaction_kind: &TransactionKind,
        gas_data: &GasData,
        transaction_signer: SuiAddress,
    ) {
        self.invariants
            .set_transaction_inputs(transaction_kind, gas_data, transaction_signer);
    }

    /// Run the (read-only) SUI-conservation and balance-accumulator invariant checks.
    /// See [`invariants::InvariantChecker::check_conservation_invariants`].
    pub(crate) fn check_conservation_invariants<Mode: ExecutionMode>(
        &self,
        move_vm: &Arc<MoveRuntime>,
        enable_expensive_checks: bool,
        cost_summary: &GasCostSummary,
    ) -> Result<(), ExecutionError> {
        self.invariants.check_conservation_invariants::<Mode>(
            self,
            move_vm,
            enable_expensive_checks,
            cost_summary,
        )
    }

    /// Check that every modified object traces back to an authenticated owner.
    /// See [`invariants::InvariantChecker::check_ownership_invariants`].
    pub(crate) fn check_ownership_invariants(
        &self,
        sender: &SuiAddress,
        sponsor: &Option<SuiAddress>,
        gas_charger: &GasCharger,
        mutable_inputs: &HashSet<ObjectID>,
        is_epoch_change: bool,
    ) -> SuiResult<()> {
        self.invariants.check_ownership_invariants(
            self,
            sender,
            sponsor,
            gas_charger,
            mutable_inputs,
            is_epoch_change,
        )
    }
}

impl TemporaryStore<'_> {
    /// Track storage gas for each mutable input object (including the gas coin)
    /// and each created object. Compute storage refunds for each deleted object.
    /// Will *not* charge anything, gas status keeps track of storage cost and rebate.
    /// All objects will be updated with their new (current) storage rebate/cost.
    /// `SuiGasStatus` `storage_rebate` and `storage_gas_units` track the transaction
    /// overall storage rebate and cost.
    pub(crate) fn collect_storage_and_rebate(&mut self, gas_charger: &mut GasCharger) {
        // Use two loops because we cannot mut iterate written while calling get_object_modified_at.
        let old_storage_rebates: Vec<_> = self
            .execution_results
            .written_objects
            .keys()
            .map(|object_id| {
                self.get_object_modified_at(object_id)
                    .map(|metadata| metadata.storage_rebate)
                    .unwrap_or_default()
            })
            .collect();
        for (object, old_storage_rebate) in self
            .execution_results
            .written_objects
            .values_mut()
            .zip_debug_eq(old_storage_rebates)
        {
            // new object size
            let new_object_size = object.object_size_for_gas_metering();
            // track changes and compute the new object `storage_rebate`
            let new_storage_rebate = gas_charger.track_storage_mutation(
                object.id(),
                new_object_size,
                old_storage_rebate,
            );
            object.storage_rebate = new_storage_rebate;
        }

        self.collect_rebate(gas_charger);
    }

    pub(crate) fn collect_rebate(&self, gas_charger: &mut GasCharger) {
        for object_id in &self.execution_results.modified_objects {
            if self
                .execution_results
                .written_objects
                .contains_key(object_id)
            {
                continue;
            }
            // get and track the deleted object `storage_rebate`
            let storage_rebate = self
                .get_object_modified_at(object_id)
                // Unwrap is safe because this loop iterates through all modified objects.
                .unwrap()
                .storage_rebate;
            gas_charger.track_storage_mutation(*object_id, 0, storage_rebate);
        }
    }

    pub fn check_execution_results_consistency<Mode: ExecutionMode>(
        &self,
    ) -> Result<(), Mode::Error> {
        assert_invariant!(
            self.execution_results
                .created_object_ids
                .iter()
                .all(|id| !self.execution_results.deleted_object_ids.contains(id)
                    && !self.execution_results.modified_objects.contains(id)),
            "Created object IDs cannot also be deleted or modified"
        );
        assert_invariant!(
            self.execution_results.modified_objects.iter().all(|id| {
                self.mutable_input_refs.contains_key(id)
                    || self.loaded_runtime_objects.contains_key(id)
                    || is_system_package(*id)
            }),
            "A modified object must be either a mutable input, a loaded child object, or a system package"
        );
        Ok(())
    }
}
//==============================================================================
// Charge gas current - end
//==============================================================================

impl TemporaryStore<'_> {
    pub fn advance_epoch_safe_mode(
        &mut self,
        params: &AdvanceEpochParams,
        protocol_config: &ProtocolConfig,
    ) {
        let wrapper = get_sui_system_state_wrapper(self.store.as_object_store())
            .expect("System state wrapper object must exist");
        let (old_object, new_object) =
            wrapper.advance_epoch_safe_mode(params, self.store.as_object_store(), protocol_config);
        self.mutate_child_object(old_object, new_object);
    }
}

impl RuntimeObjectResolver for TemporaryStore<'_> {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        let obj_opt = self.execution_results.written_objects.get(child);
        if obj_opt.is_some() {
            Ok(obj_opt.cloned())
        } else {
            let _scope = monitored_scope("Execution::read_child_object");
            self.store
                .read_child_object(parent, child, child_version_upper_bound)
        }
    }

    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        // You should never be able to try and receive an object after deleting it or writing it in the same
        // transaction since `Receiving` doesn't have copy.
        debug_assert!(
            !self
                .execution_results
                .written_objects
                .contains_key(receiving_object_id)
        );
        debug_assert!(
            !self
                .execution_results
                .deleted_object_ids
                .contains(receiving_object_id)
        );
        self.store.get_object_received_at_version(
            owner,
            receiving_object_id,
            receive_object_at_version,
            epoch_id,
        )
    }
}

/// Compares the owner and payload of an object.
/// This is used to detect illegal writes to non-exclusive write objects.
fn was_object_mutated(object: &Object, original: &Object) -> bool {
    let data_equal = match (&object.data, &original.data) {
        (Data::Move(a), Data::Move(b)) => a.contents_and_type_equal(b),
        // We don't have a use for package content-equality, so we remain as strict as
        // possible for now.
        (Data::Package(a), Data::Package(b)) => a == b,
        _ => false,
    };

    let owner_equal = match (&object.owner, &original.owner) {
        // We don't compare initial shared versions, because re-shared objects do not have the
        // correct initial shared version at this point in time, and this field is not something
        // that can be modified by a single transaction anyway.
        (Owner::Shared { .. }, Owner::Shared { .. }) => true,
        (
            Owner::ConsensusAddressOwner { owner: a, .. },
            Owner::ConsensusAddressOwner { owner: b, .. },
        ) => a == b,
        (Owner::AddressOwner(a), Owner::AddressOwner(b)) => a == b,
        (Owner::Immutable, Owner::Immutable) => true,
        (Owner::ObjectOwner(a), Owner::ObjectOwner(b)) => a == b,
        (
            Owner::Party {
                permissions: a,
                start_version: _,
            },
            Owner::Party {
                permissions: b,
                start_version: _,
            },
        ) => a == b,

        // Keep the left hand side of the match exhaustive to catch future
        // changes to Owner
        (Owner::AddressOwner(_), _)
        | (Owner::Immutable, _)
        | (Owner::ObjectOwner(_), _)
        | (Owner::Shared { .. }, _)
        | (Owner::ConsensusAddressOwner { .. }, _)
        | (Owner::Party { .. }, _) => false,
    };

    !data_equal || !owner_equal
}

impl Storage for TemporaryStore<'_> {
    fn reset(&mut self) {
        self.drop_writes();
    }

    fn read_object(&self, id: &ObjectID) -> Option<&Object> {
        TemporaryStore::read_object(self, id)
    }

    /// Take execution results v2, and translate it back to be compatible with effects v1.
    fn record_execution_results(
        &mut self,
        results: ExecutionResults,
    ) -> Result<(), ExecutionError> {
        let ExecutionResults::V2(mut results) = results else {
            panic!("ExecutionResults::V2 expected in sui-execution v1 and above");
        };

        // for all non-exclusive write inputs, remove them from written objects
        let mut to_remove = Vec::new();
        for (id, original) in &self.non_exclusive_input_original_versions {
            // Object must be present in `written_objects` and identical
            if results
                .written_objects
                .get(id)
                .map(|obj| was_object_mutated(obj, original))
                .unwrap_or(true)
            {
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::NonExclusiveWriteInputObjectModified { id: *id },
                    "Non-exclusive write input object has been modified or deleted",
                ));
            }
            to_remove.push(*id);
        }

        for id in to_remove {
            results.written_objects.remove(&id);
            results.modified_objects.remove(&id);
        }

        // It's important to merge instead of override results because it's
        // possible to execute PT more than once during tx execution.
        // Track the index range of accumulator events brought in here as PTB-emitted; the
        // address-balance change invariant (run inside `run_conservation_checks`) uses this
        // set to distinguish trusted PTB-emitted events from runtime-emitted ones.
        let event_start = self.execution_results.accumulator_events.len();
        self.execution_results.merge_results(
            results, /* consistent_merge */ true, /* invariant_checks */ true,
        )?;
        let event_end = self.execution_results.accumulator_events.len();
        self.invariants
            .record_ptb_event_range(event_start, event_end);

        Ok(())
    }

    fn save_loaded_runtime_objects(
        &mut self,
        loaded_runtime_objects: BTreeMap<ObjectID, DynamicallyLoadedObjectMetadata>,
    ) {
        TemporaryStore::save_loaded_runtime_objects(self, loaded_runtime_objects)
    }

    fn save_wrapped_object_containers(
        &mut self,
        wrapped_object_containers: BTreeMap<ObjectID, ObjectID>,
    ) {
        TemporaryStore::save_wrapped_object_containers(self, wrapped_object_containers)
    }

    fn check_coin_deny_list(
        &self,
        receiving_funds_type_and_owners: BTreeMap<TypeTag, BTreeSet<SuiAddress>>,
    ) -> DenyListResult {
        let result = check_coin_deny_list_v2_during_execution(
            receiving_funds_type_and_owners,
            self.cur_epoch,
            self.store.as_object_store(),
        );
        // The denylist object is only loaded if there are regulated transfers.
        // And also if we already have it in the input there is no need to commit it again in the effects.
        if result.num_non_gas_coin_owners > 0
            && !self.input_objects.contains_key(&SUI_DENY_LIST_OBJECT_ID)
        {
            self.loaded_per_epoch_config_objects
                .write()
                .insert(SUI_DENY_LIST_OBJECT_ID);
        }
        result
    }

    fn record_generated_object_ids(&mut self, generated_ids: BTreeSet<ObjectID>) {
        TemporaryStore::save_generated_object_ids(self, generated_ids)
    }
}

impl BackingPackageStore for TemporaryStore<'_> {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        // We first check the objects in the temporary store because in non-production code path,
        // it is possible to read packages that are just written in the same transaction.
        // This can happen for example when we run the expensive conservation checks, where we may
        // look into the types of each written object in the output, and some of them need the
        // newly written packages for type checking.
        // In production path though, this should never happen.
        if let Some(obj) = self.execution_results.written_objects.get(package_id) {
            Ok(Some(PackageObject::new(obj.clone())))
        } else {
            self.store.get_package_object(package_id).inspect(|obj| {
                // Track object but leave unchanged
                if let Some(v) = obj
                    && !self
                        .runtime_packages_loaded_from_db
                        .read()
                        .contains_key(package_id)
                {
                    // TODO: Can this lock ever block execution?
                    // TODO: Another way to avoid the cost of maintaining this map is to not
                    // enable it in normal runs, and if a fork is detected, rerun it with a flag
                    // turned on and start populating this field.
                    self.runtime_packages_loaded_from_db
                        .write()
                        .insert(*package_id, v.clone());
                }
            })
        }
    }
}
