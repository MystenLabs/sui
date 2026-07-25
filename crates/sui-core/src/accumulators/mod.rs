// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};

use itertools::Itertools;
use move_core_types::ident_str;
use move_core_types::u256::U256;
use mysten_common::fatal;
use sui_protocol_config::ProtocolConfig;
use sui_types::accumulator_event::AccumulatorEvent;
use sui_types::accumulator_root::{
    ACCUMULATOR_ROOT_SETTLE_U128_FUNC, ACCUMULATOR_ROOT_SETTLEMENT_PROLOGUE_FUNC,
    ACCUMULATOR_SETTLEMENT_MODULE, AccumulatorObjId, EventCommitment, build_event_merkle_root,
};
use sui_types::balance::{BALANCE_MODULE_NAME, BALANCE_STRUCT_NAME};
use sui_types::base_types::SequenceNumber;

use sui_types::accumulator_root::ACCUMULATOR_METADATA_MODULE;
use sui_types::digests::Digest;
use sui_types::effects::{
    AccumulatorAddress, AccumulatorOperation, AccumulatorValue, AccumulatorWriteV1, IDOperation,
    TransactionEffects, TransactionEffectsAPI,
};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{
    Argument, CallArg, ObjectArg, SharedObjectMutability, TransactionKind,
};
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_FRAMEWORK_ADDRESS, SUI_FRAMEWORK_PACKAGE_ID, TypeTag,
};

use crate::execution_cache::TransactionCacheRead;

// provides balance read functionality for the scheduler
pub mod funds_read;
// provides balance read functionality for RPC
pub mod balances;
pub mod coin_reservations;
pub mod object_funds_checker;
pub(crate) mod transaction_rewriting;

/// Merged value is the value stored inside accumulator objects.
/// Each mergeable Move type will map to a single variant as its representation.
///
/// For instance, Balance<T> stores a single u64 value, so it will map to SumU128.
/// A clawback Balance<T> will map to SumU128U128 since it also needs to represent
/// the amount of the balance that has been frozen.
#[derive(Debug, Copy, Clone)]
enum MergedValue {
    SumU128(u128),
    SumU128U128(u128, u128),
    /// Merkle root of events in this checkpoint and event count.
    EventDigest(/* merkle root */ Digest, /* event count */ u64),
}

enum ClassifiedType {
    Balance,
    Unknown,
}

impl ClassifiedType {
    fn classify(ty: &TypeTag) -> Self {
        let TypeTag::Struct(struct_tag) = ty else {
            return Self::Unknown;
        };

        if struct_tag.address == SUI_FRAMEWORK_ADDRESS
            && struct_tag.module.as_ident_str() == BALANCE_MODULE_NAME
            && struct_tag.name.as_ident_str() == BALANCE_STRUCT_NAME
        {
            return Self::Balance;
        }

        Self::Unknown
    }
}

impl MergedValue {
    fn add_move_call(
        merge: Self,
        split: Self,
        root: Argument,
        address: &AccumulatorAddress,
        checkpoint_seq: u64,
        builder: &mut ProgrammableTransactionBuilder,
    ) {
        let ty = ClassifiedType::classify(&address.ty);
        let address_arg = builder.pure(address.address).unwrap();

        match (ty, merge, split) {
            (
                ClassifiedType::Balance,
                MergedValue::SumU128(merge_amount),
                MergedValue::SumU128(split_amount),
            ) => {
                // Net out the merge and split amounts.
                let (merge_amount, split_amount) = if merge_amount >= split_amount {
                    (merge_amount - split_amount, 0)
                } else {
                    (0, split_amount - merge_amount)
                };

                if merge_amount != 0 || split_amount != 0 {
                    let merge_amount = builder.pure(merge_amount).unwrap();
                    let split_amount = builder.pure(split_amount).unwrap();
                    builder.programmable_move_call(
                        SUI_FRAMEWORK_PACKAGE_ID,
                        ACCUMULATOR_SETTLEMENT_MODULE.into(),
                        ACCUMULATOR_ROOT_SETTLE_U128_FUNC.into(),
                        vec![address.ty.clone()],
                        vec![root, address_arg, merge_amount, split_amount],
                    );
                }
            }
            (_, MergedValue::SumU128U128(_v1, _v2), MergedValue::SumU128U128(_w1, _w2)) => todo!(),
            (_, MergedValue::EventDigest(digest, event_count), MergedValue::EventDigest(_, _)) => {
                let args = vec![
                    root,
                    builder.pure(address.address).unwrap(),
                    builder
                        .pure(U256::from_le_bytes(&digest.into_inner()))
                        .unwrap(),
                    builder.pure(event_count).unwrap(),
                    builder.pure(checkpoint_seq).unwrap(),
                ];
                builder.programmable_move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    ACCUMULATOR_SETTLEMENT_MODULE.into(),
                    sui_types::accumulator_root::ACCUMULATOR_ROOT_SETTLEMENT_SETTLE_EVENTS_FUNC
                        .into(),
                    vec![],
                    args,
                );
            }
            _ => fatal!("invalid merge {:?} {:?}", merge, split),
        }
    }
}

impl From<MergedValueIntermediate> for MergedValue {
    fn from(value: MergedValueIntermediate) -> Self {
        match value {
            MergedValueIntermediate::SumU128(v) => MergedValue::SumU128(v),
            MergedValueIntermediate::SumU128U128(v1, v2) => MergedValue::SumU128U128(v1, v2),
            MergedValueIntermediate::Events(events) => {
                let event_count = events.len() as u64;
                MergedValue::EventDigest(build_event_merkle_root(&events), event_count)
            }
        }
    }
}

/// MergedValueIntermediate is an intermediate / in-memory representation of the for
/// accumulators. It is used to store the merged result of all accumulator writes in a single
/// checkpoint.
///
/// This pattern is not necessary for fully commutative operations, since those could use MergedValue directly.
///
/// However, this supports the commutative-merge + non-commutative-update pattern, which will be used by event
/// streams. In this pattern, everything within a checkpoint is merged commutatively, and then a single
/// non-commutative update is applied to the accumulator at the end of the checkpoint.
#[derive(Debug, Clone)]
enum MergedValueIntermediate {
    SumU128(u128),
    SumU128U128(u128, u128),
    Events(Vec<EventCommitment>),
}

impl MergedValueIntermediate {
    // Create a zero value with the appropriate type for the accumulator value.
    fn zero(value: &AccumulatorValue) -> Self {
        match value {
            AccumulatorValue::Integer(_) => Self::SumU128(0),
            AccumulatorValue::IntegerTuple(_, _) => Self::SumU128U128(0, 0),
            AccumulatorValue::EventDigest(_) => Self::Events(vec![]),
        }
    }

    fn accumulate_into(
        &mut self,
        value: AccumulatorValue,
        checkpoint_seq: u64,
        transaction_idx: u64,
    ) {
        match (self, value) {
            (Self::SumU128(v1), AccumulatorValue::Integer(v2)) => *v1 += v2 as u128,
            (Self::SumU128U128(v1, v2), AccumulatorValue::IntegerTuple(w1, w2)) => {
                *v1 += w1 as u128;
                *v2 += w2 as u128;
            }
            (Self::Events(commitments), AccumulatorValue::EventDigest(event_digests)) => {
                for (event_idx, digest) in event_digests {
                    commitments.push(EventCommitment::new(
                        checkpoint_seq,
                        transaction_idx,
                        event_idx,
                        digest,
                    ));
                }
            }
            _ => {
                fatal!("invalid merge");
            }
        }
    }
}

struct Update {
    merge: MergedValueIntermediate,
    split: MergedValueIntermediate,
    // Track input and output SUI for each update. Necessary so that when we construct
    // a settlement transaction from a collection of Updates, they can accurately
    // track the net SUI flows.
    input_sui: u64,
    output_sui: u64,
}

pub(crate) struct AccumulatorSettlementTxBuilder {
    // updates is iterated over, must be a BTreeMap
    updates: BTreeMap<AccumulatorObjId, Update>,
    // addresses is only used for lookups.
    addresses: HashMap<AccumulatorObjId, AccumulatorAddress>,
    num_deposits: u64,
    num_withdrawals: u64,
}

impl AccumulatorSettlementTxBuilder {
    pub fn new(
        cache: Option<&dyn TransactionCacheRead>,
        ckpt_effects: &[TransactionEffects],
        checkpoint_seq: u64,
        tx_index_offset: u64,
    ) -> Self {
        let mut updates = BTreeMap::<_, _>::new();
        let mut addresses = HashMap::<_, _>::new();
        let mut num_deposits = 0u64;
        let mut num_withdrawals = 0u64;

        for (tx_index, effect) in ckpt_effects.iter().enumerate() {
            let tx = effect.transaction_digest();
            // TransactionEffectsAPI::accumulator_events() uses a linear scan of all
            // object changes and allocates a new vector. In the common case (on validators),
            // we still have still have the original vector in the writeback cache, so
            // we can avoid the unnecessary work by just taking it from the cache.
            let events = match cache.and_then(|c| c.take_accumulator_events(tx)) {
                Some(events) => events,
                None => effect.accumulator_events(),
            };

            for event in events {
                // The input to the settlement is the sum of the outputs of accumulator events (i.e. deposits).
                // and the output of the settlement is the sum of the inputs (i.e. withdraws).
                let (event_input_sui, event_output_sui) = event.total_sui_in_event();

                let AccumulatorEvent {
                    accumulator_obj,
                    write:
                        AccumulatorWriteV1 {
                            operation,
                            value,
                            address,
                        },
                } = event;

                if let Some(prev) = addresses.insert(accumulator_obj, address.clone()) {
                    debug_assert_eq!(prev, address);
                }

                let entry = updates.entry(accumulator_obj).or_insert_with(|| {
                    let zero = MergedValueIntermediate::zero(&value);
                    Update {
                        merge: zero.clone(),
                        split: zero,
                        input_sui: 0,
                        output_sui: 0,
                    }
                });

                // The output of the event is the input of the settlement, and vice versa.
                entry.input_sui += event_output_sui;
                entry.output_sui += event_input_sui;

                match operation {
                    AccumulatorOperation::Merge => {
                        num_deposits += 1;
                        entry.merge.accumulate_into(
                            value,
                            checkpoint_seq,
                            tx_index as u64 + tx_index_offset,
                        );
                    }
                    AccumulatorOperation::Split => {
                        num_withdrawals += 1;
                        entry.split.accumulate_into(
                            value,
                            checkpoint_seq,
                            tx_index as u64 + tx_index_offset,
                        );
                    }
                }
            }
        }

        Self {
            updates,
            addresses,
            num_deposits,
            num_withdrawals,
        }
    }

    pub fn num_deposits(&self) -> u64 {
        self.num_deposits
    }

    pub fn num_withdrawals(&self) -> u64 {
        self.num_withdrawals
    }

    /// Returns a unified map of funds changes for all accounts.
    /// The funds change for each account is merged from the merge and split operations.
    pub fn collect_funds_changes(&self) -> BTreeMap<AccumulatorObjId, i128> {
        self.updates
            .iter()
            .filter_map(|(object_id, update)| match (&update.merge, &update.split) {
                (
                    MergedValueIntermediate::SumU128(merge),
                    MergedValueIntermediate::SumU128(split),
                ) => Some((*object_id, *merge as i128 - *split as i128)),
                _ => None,
            })
            .collect()
    }

    /// Builds settlement transactions that apply accumulator updates.
    pub fn build_tx(
        self,
        protocol_config: &ProtocolConfig,
        epoch: u64,
        accumulator_root_obj_initial_shared_version: SequenceNumber,
        checkpoint_height: u64,
        checkpoint_seq: u64,
    ) -> Vec<TransactionKind> {
        let Self {
            updates, addresses, ..
        } = self;

        let build_one_settlement_txn = |idx: u64, updates: &mut Vec<(AccumulatorObjId, Update)>| {
            let (total_input_sui, total_output_sui) =
                updates
                    .iter()
                    .fold((0, 0), |(acc_input, acc_output), (_, update)| {
                        (acc_input + update.input_sui, acc_output + update.output_sui)
                    });

            Self::build_one_settlement_txn(
                &addresses,
                epoch,
                idx,
                checkpoint_height,
                accumulator_root_obj_initial_shared_version,
                updates.drain(..),
                total_input_sui,
                total_output_sui,
                checkpoint_seq,
            )
        };

        let chunk_size = protocol_config
            .max_updates_per_settlement_txn_as_option()
            .unwrap_or(u32::MAX) as usize;

        updates
            .into_iter()
            .chunks(chunk_size)
            .into_iter()
            .enumerate()
            .map(|(idx, chunk)| {
                build_one_settlement_txn(idx as u64, &mut chunk.collect::<Vec<_>>())
            })
            .collect()
    }

    fn add_prologue(
        builder: &mut ProgrammableTransactionBuilder,
        root: Argument,
        epoch: u64,
        checkpoint_height: u64,
        idx: u64,
        total_input_sui: u64,
        total_output_sui: u64,
    ) {
        let epoch_arg = builder.pure(epoch).unwrap();
        let checkpoint_height_arg = builder.pure(checkpoint_height).unwrap();
        let idx_arg = builder.pure(idx).unwrap();
        let total_input_sui_arg = builder.pure(total_input_sui).unwrap();
        let total_output_sui_arg = builder.pure(total_output_sui).unwrap();

        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ACCUMULATOR_SETTLEMENT_MODULE.into(),
            ACCUMULATOR_ROOT_SETTLEMENT_PROLOGUE_FUNC.into(),
            vec![],
            vec![
                root,
                epoch_arg,
                checkpoint_height_arg,
                idx_arg,
                total_input_sui_arg,
                total_output_sui_arg,
            ],
        );
    }

    fn build_one_settlement_txn(
        addresses: &HashMap<AccumulatorObjId, AccumulatorAddress>,
        epoch: u64,
        idx: u64,
        checkpoint_height: u64,
        accumulator_root_obj_initial_shared_version: SequenceNumber,
        updates: impl Iterator<Item = (AccumulatorObjId, Update)>,
        total_input_sui: u64,
        total_output_sui: u64,
        checkpoint_seq: u64,
    ) -> TransactionKind {
        let mut builder = ProgrammableTransactionBuilder::new();

        let root = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: SUI_ACCUMULATOR_ROOT_OBJECT_ID,
                initial_shared_version: accumulator_root_obj_initial_shared_version,
                mutability: SharedObjectMutability::NonExclusiveWrite,
            }))
            .unwrap();

        Self::add_prologue(
            &mut builder,
            root,
            epoch,
            checkpoint_height,
            idx,
            total_input_sui,
            total_output_sui,
        );

        for (accumulator_obj, update) in updates {
            let Update { merge, split, .. } = update;
            let address = addresses.get(&accumulator_obj).unwrap();
            let merged_value = MergedValue::from(merge);
            let split_value = MergedValue::from(split);
            MergedValue::add_move_call(
                merged_value,
                split_value,
                root,
                address,
                checkpoint_seq,
                &mut builder,
            );
        }

        TransactionKind::ProgrammableSystemTransaction(builder.finish())
    }
}

/// Builds the barrier transaction that advances the version of the accumulator root object.
/// This must be called after all settlement transactions have been executed.
/// `settlement_effects` contains the effects of all settlement transactions.
pub fn build_accumulator_barrier_tx(
    epoch: u64,
    accumulator_root_obj_initial_shared_version: SequenceNumber,
    checkpoint_height: u64,
    settlement_effects: &[TransactionEffects],
) -> TransactionKind {
    let num_settlements = settlement_effects.len() as u64;

    let (objects_created, objects_destroyed) = count_accumulator_object_changes(settlement_effects);

    let mut builder = ProgrammableTransactionBuilder::new();
    let root = builder
        .input(CallArg::Object(ObjectArg::SharedObject {
            id: SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            initial_shared_version: accumulator_root_obj_initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        }))
        .unwrap();

    AccumulatorSettlementTxBuilder::add_prologue(
        &mut builder,
        root,
        epoch,
        checkpoint_height,
        num_settlements,
        0,
        0,
    );

    let objects_created_arg = builder.pure(objects_created).unwrap();
    let objects_destroyed_arg = builder.pure(objects_destroyed).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ACCUMULATOR_METADATA_MODULE.into(),
        ident_str!("record_accumulator_object_changes").into(),
        vec![],
        vec![root, objects_created_arg, objects_destroyed_arg],
    );

    TransactionKind::ProgrammableSystemTransaction(builder.finish())
}

pub(crate) fn count_accumulator_object_changes(
    settlement_effects: &[TransactionEffects],
) -> (u64, u64) {
    settlement_effects
        .iter()
        .flat_map(|effects| effects.object_changes())
        .fold((0u64, 0u64), |(created, destroyed), change| {
            match change.id_operation {
                IDOperation::Created => (created + 1, destroyed),
                IDOperation::Deleted => (created, destroyed + 1),
                IDOperation::None => (created, destroyed),
            }
        })
}

#[cfg(test)]
mod barrier_settlement_key_tests {
    use super::*;
    use sui_types::transaction::TransactionKey;

    #[test]
    fn test_barrier_tx_returns_accumulator_settlement_key() {
        let epoch = 5u64;
        let checkpoint_height = 42u64;

        let kind = build_accumulator_barrier_tx(
            epoch,
            SequenceNumber::from_u64(1),
            checkpoint_height,
            &[], // no settlement effects needed for key extraction
        );

        assert_eq!(
            kind.accumulator_barrier_settlement_key(),
            Some(TransactionKey::AccumulatorSettlement(
                epoch,
                checkpoint_height
            ))
        );
        assert!(kind.is_accumulator_barrier_settle_tx());
    }

    #[test]
    fn test_settlement_tx_has_no_barrier_key() {
        // Non-barrier settlement transactions use ReadOnly access to the accumulator root,
        // so they should not return an AccumulatorSettlement key.
        let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
        let builder = AccumulatorSettlementTxBuilder::new(None, &[], 0, 0);
        let txns = builder.build_tx(&protocol_config, 5, SequenceNumber::from_u64(1), 42, 0);
        for txn in txns {
            assert_eq!(txn.accumulator_barrier_settlement_key(), None);
            assert!(!txn.is_accumulator_barrier_settle_tx());
        }
    }
}

/// Regression test for audit finding F9: V2 settlement determinism / `tx_index_offset` matching.
///
/// The settlement scheduler (which constructs and executes settlement transactions) and the
/// checkpoint builder (which independently reconstructs digests to wait for effects) must agree
/// on the value of `tx_index_offset` passed to [`AccumulatorSettlementTxBuilder::new`]. The two
/// paths derive that offset by completely separate arithmetic:
///
/// * Scheduler (`execution_scheduler/settlement_scheduler.rs::run_queue`):
///   `running_tx_offset += batch_info.tx_keys.len() + (build_tx.len() + 1)` per batch.
/// * Builder (`checkpoints/mod.rs::resolve_checkpoint_transactions_v2`):
///   `tx_index_offset = all_effects.len()`, where `all_effects` is extended with
///   `sorted_root_effects ++ settlement_effects ++ barrier_effect` per chunk.
///
/// They match today only by algebraic coincidence (`tx_keys.len() == sorted_root_effects.len()`,
/// `build_tx.len()` is identical, `+1 barrier == +1 barrier_effect`). For pure-numeric
/// accumulators (`SumU128` / `SumU128U128`), `accumulate_into` ignores `transaction_idx`, so a
/// drift would be silent. For `AccumulatorValue::EventDigest`, `EventCommitment::new` stamps
/// `transaction_idx = tx_index + tx_index_offset` into the merkle leaf, and any drift in
/// `tx_index_offset` between the two paths immediately changes the settlement transaction
/// digest the checkpoint builder waits for, deadlocking the builder.
///
/// This test forges minimal `TransactionEffects` containing `EventDigest` accumulator writes,
/// simulates several `PendingCheckpointV2`-shaped chunk sequences end-to-end through both
/// `tx_index_offset` derivations, and asserts that the resulting settlement transaction digests
/// are byte-equal across paths. It is intentionally a property/invariant test rather than a
/// reproduction of a live bug: F9 is currently latent.
#[cfg(test)]
mod settlement_tx_index_offset_invariant_tests {
    use super::*;
    use nonempty::nonempty;
    use std::collections::BTreeMap;
    use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
    use sui_types::digests::Digest;
    use sui_types::effects::{
        AccumulatorAddress, AccumulatorOperation, AccumulatorValue, AccumulatorWriteV1,
        EffectsObjectChange, TransactionEffects,
    };
    use sui_types::execution_status::ExecutionStatus;
    use sui_types::gas::GasCostSummary;
    use sui_types::object::OBJECT_START_VERSION;
    use sui_types::transaction::TransactionKind;
    use sui_types::{Identifier, TypeTag};

    /// A non-Balance type tag bypasses `AccumulatorEvent::new`'s debug-only check that the
    /// `AccumulatorObjId` is the correctly-derived dynamic-field id for the address+type.
    /// Using a synthetic type also keeps `total_sui_in_event` at zero so the input/output SUI
    /// accounting in the settlement tx remains deterministic across both paths.
    fn synthetic_event_stream_type() -> TypeTag {
        TypeTag::Struct(Box::new(move_core_types::language_storage::StructTag {
            address: move_core_types::account_address::AccountAddress::ZERO,
            module: Identifier::new("event_stream_test").unwrap(),
            name: Identifier::new("EventStream").unwrap(),
            type_params: vec![],
        }))
    }

    /// Build a synthetic `TransactionEffects` whose only changed-object entry is a single
    /// `AccumulatorWriteV1::EventDigest` write. We deliberately go through `TransactionEffectsV2`
    /// directly rather than `TestEffectsBuilder` to avoid pulling in a `SenderSignedData` and to
    /// keep the test focused on `accumulator_events()` -> `EventCommitment::new`.
    fn effects_with_event_digest_write(
        tx_seed: u64,
        accumulator_obj_seed: u32,
        digest_byte: u8,
    ) -> TransactionEffects {
        let mut tx_digest_bytes = [0u8; 32];
        tx_digest_bytes[..8].copy_from_slice(&tx_seed.to_le_bytes());
        let tx_digest = TransactionDigest::new(tx_digest_bytes);

        let mut obj_id_bytes = [0u8; ObjectID::LENGTH];
        obj_id_bytes[..4].copy_from_slice(&accumulator_obj_seed.to_le_bytes());
        let accumulator_obj = ObjectID::new(obj_id_bytes);

        let address = AccumulatorAddress::new(SuiAddress::ZERO, synthetic_event_stream_type());
        let write = AccumulatorWriteV1 {
            address,
            operation: AccumulatorOperation::Merge,
            value: AccumulatorValue::EventDigest(nonempty![(
                0u64, // event_idx within the transaction
                Digest::new([digest_byte; 32]),
            )]),
        };

        let mut changed_objects = BTreeMap::new();
        changed_objects.insert(
            accumulator_obj,
            EffectsObjectChange::new_from_accumulator_write(write),
        );

        TransactionEffects::new_from_execution_v2(
            ExecutionStatus::Success,
            /* executed_epoch */ 0,
            GasCostSummary::default(),
            /* shared_objects */ vec![],
            /* loaded_per_epoch_config_objects */ std::collections::BTreeSet::new(),
            tx_digest,
            OBJECT_START_VERSION,
            changed_objects,
            /* gas_object */ None,
            /* events_digest */ None,
            /* dependencies */ vec![],
        )
    }

    /// Compute the digests the settlement scheduler would produce for one batch's settlement
    /// transactions, given the batch's effects, the per-batch `tx_index_offset`, and the
    /// usual builder construction parameters. Returns both the digests and the count of
    /// settlement txns (without the barrier) so the caller can advance its `running_tx_offset`.
    fn build_settlement_tx_digests(
        protocol_config: &ProtocolConfig,
        effects: &[TransactionEffects],
        checkpoint_seq: u64,
        checkpoint_height: u64,
        epoch: u64,
        tx_index_offset: u64,
    ) -> (Vec<sui_types::digests::TransactionDigest>, usize) {
        let builder = AccumulatorSettlementTxBuilder::new(
            /* cache */ None,
            effects,
            checkpoint_seq,
            tx_index_offset,
        );
        let kinds: Vec<TransactionKind> = builder.build_tx(
            protocol_config,
            epoch,
            SequenceNumber::from_u64(1),
            checkpoint_height,
            checkpoint_seq,
        );
        let count = kinds.len();
        let digests = kinds
            .into_iter()
            .map(|k| {
                *sui_types::transaction::VerifiedTransaction::new_system_transaction(k).digest()
            })
            .collect();
        (digests, count)
    }

    /// Description of one synthetic batch within a checkpoint: how many "root" transactions
    /// the batch contributes, how many of those transactions carry an accumulator write,
    /// and seed values to make digests vary across batches.
    #[derive(Clone, Copy, Debug)]
    struct BatchShape {
        num_root_txs: usize,
        num_accumulator_writes: usize,
    }

    /// Drive both paths' `tx_index_offset` math against the same batches and assert that the
    /// resulting settlement-transaction digests are identical, for every batch.
    fn assert_offset_paths_agree(
        protocol_config: &ProtocolConfig,
        checkpoint_seq: u64,
        checkpoint_height: u64,
        epoch: u64,
        batches: &[BatchShape],
    ) {
        // Builder-side accumulator: extended with root effects + settlement effects + 1 barrier
        // per batch, exactly like `resolve_checkpoint_transactions_v2` builds `all_effects`.
        let mut builder_all_effects_len: u64 = 0;

        // Scheduler-side running offset: starts at 0 for each checkpoint_seq, accumulated as
        // batch_tx_count + settlement_tx_count (incl. +1 barrier) per batch.
        let mut scheduler_running_tx_offset: u64 = 0;

        let mut digest_byte_seed: u8 = 1;

        for (batch_idx, batch) in batches.iter().enumerate() {
            assert!(
                batch.num_accumulator_writes <= batch.num_root_txs,
                "test setup: writes must come from root txs"
            );

            // Build this batch's root effects. The first `num_accumulator_writes` carry an
            // event-digest accumulator write; the rest have no accumulator events.
            let mut batch_effects = Vec::with_capacity(batch.num_root_txs);
            for tx_idx_in_batch in 0..batch.num_root_txs {
                if tx_idx_in_batch < batch.num_accumulator_writes {
                    batch_effects.push(effects_with_event_digest_write(
                        (batch_idx as u64) * 1_000_000 + tx_idx_in_batch as u64,
                        // Each accumulator write goes to a distinct accumulator object so
                        // they don't merge into one update.
                        (batch_idx as u32) * 1_000_000 + tx_idx_in_batch as u32 + 1,
                        digest_byte_seed,
                    ));
                    digest_byte_seed = digest_byte_seed.wrapping_add(1);
                } else {
                    // A "plain" root effect with no accumulator events still contributes to
                    // the index space (the production code iterates all effects in order).
                    let mut tx_digest_bytes = [0u8; 32];
                    tx_digest_bytes[..8].copy_from_slice(
                        &((batch_idx as u64) * 1000 + tx_idx_in_batch as u64).to_le_bytes(),
                    );
                    tx_digest_bytes[8] = 0xAA; // distinguish from accumulator-carrying txs
                    let tx_digest = TransactionDigest::new(tx_digest_bytes);
                    batch_effects.push(TransactionEffects::new_from_execution_v2(
                        ExecutionStatus::Success,
                        0,
                        GasCostSummary::default(),
                        vec![],
                        std::collections::BTreeSet::new(),
                        tx_digest,
                        OBJECT_START_VERSION,
                        BTreeMap::new(),
                        None,
                        None,
                        vec![],
                    ));
                }
            }

            // Scheduler path: tx_index_offset is the running offset *at the start* of this batch.
            let scheduler_offset = scheduler_running_tx_offset;
            let (scheduler_digests, scheduler_settlement_count) = build_settlement_tx_digests(
                protocol_config,
                &batch_effects,
                checkpoint_seq,
                checkpoint_height,
                epoch,
                scheduler_offset,
            );

            // Builder path: tx_index_offset = all_effects.len() at the start of this batch.
            let builder_offset = builder_all_effects_len;
            let (builder_digests, builder_settlement_count) = build_settlement_tx_digests(
                protocol_config,
                &batch_effects,
                checkpoint_seq,
                checkpoint_height,
                epoch,
                builder_offset,
            );

            assert_eq!(
                scheduler_offset, builder_offset,
                "batch {}: tx_index_offset diverged between paths (scheduler={}, builder={})",
                batch_idx, scheduler_offset, builder_offset,
            );
            assert_eq!(
                scheduler_settlement_count, builder_settlement_count,
                "batch {}: settlement tx count diverged between paths",
                batch_idx,
            );
            assert_eq!(
                scheduler_digests, builder_digests,
                "batch {}: settlement transaction digests diverged between paths. \
                 scheduler_offset={}, builder_offset={}",
                batch_idx, scheduler_offset, builder_offset,
            );

            // Advance both running offsets the way each production code path does.
            let batch_tx_count = batch_effects.len() as u64;
            let settlement_tx_count = scheduler_settlement_count as u64 + 1; // +1 barrier
            scheduler_running_tx_offset += batch_tx_count + settlement_tx_count;
            // Builder extends all_effects with sorted (root) effects + settlement_effects +
            // 1 barrier effect. The settlement_effects count equals the settlement tx count;
            // the barrier effect is a separate +1.
            builder_all_effects_len +=
                batch_tx_count + (scheduler_settlement_count as u64) + 1 /* barrier */;
        }
    }

    #[tokio::test]
    async fn settlement_tx_index_offset_matches_across_paths_single_batch() {
        let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
        assert_offset_paths_agree(
            &protocol_config,
            /* checkpoint_seq */ 7,
            /* checkpoint_height */ 11,
            /* epoch */ 3,
            &[BatchShape {
                num_root_txs: 5,
                num_accumulator_writes: 3,
            }],
        );
    }

    #[tokio::test]
    async fn settlement_tx_index_offset_matches_across_paths_multiple_batches() {
        let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
        assert_offset_paths_agree(
            &protocol_config,
            /* checkpoint_seq */ 42,
            /* checkpoint_height */ 100,
            /* epoch */ 9,
            // Multiple chunks per pending: the offset carries over from one batch to the next,
            // and any drift in either path's arithmetic will surface on batch 2 or later.
            &[
                BatchShape {
                    num_root_txs: 4,
                    num_accumulator_writes: 2,
                },
                BatchShape {
                    num_root_txs: 6,
                    num_accumulator_writes: 4,
                },
                BatchShape {
                    num_root_txs: 3,
                    num_accumulator_writes: 1,
                },
            ],
        );
    }

    #[tokio::test]
    async fn settlement_tx_index_offset_matches_across_paths_chunked_updates() {
        // Force the settlement-tx builder to emit multiple chunks per batch so that
        // `build_tx.len() > 1` exercises the `running_tx_offset += build_tx.len() + 1` path
        // (scheduler) and the builder's `all_effects.extend(settlement_effects)` path
        // (checkpoint builder) symmetrically. We use the default `max_updates_per_settlement_txn`
        // (currently 100 at the max protocol version) and supply enough accumulator updates per
        // batch to exceed it. Increasing the input size is cheap compared to plumbing in a
        // public setter for a private protocol-config field.
        let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
        let chunk_size = protocol_config
            .max_updates_per_settlement_txn_as_option()
            .expect("max_updates_per_settlement_txn must be set at max protocol version")
            as usize;
        // First batch produces 3 chunks; second produces 2 chunks; third produces 1. This
        // exercises varying `build_tx.len()` so an off-by-N drift in either path becomes
        // visible on the second and third batch's digests.
        let batch_1 = chunk_size * 2 + 5;
        let batch_2 = chunk_size + 7;
        let batch_3 = chunk_size / 2;
        assert_offset_paths_agree(
            &protocol_config,
            /* checkpoint_seq */ 17,
            /* checkpoint_height */ 23,
            /* epoch */ 5,
            &[
                BatchShape {
                    num_root_txs: batch_1,
                    num_accumulator_writes: batch_1,
                },
                BatchShape {
                    num_root_txs: batch_2,
                    num_accumulator_writes: batch_2,
                },
                BatchShape {
                    num_root_txs: batch_3,
                    num_accumulator_writes: batch_3,
                },
            ],
        );
    }

    #[tokio::test]
    async fn settlement_tx_index_offset_event_commitments_actually_depend_on_offset() {
        // Sanity check: this test would be vacuous if `tx_index_offset` did not actually
        // affect the resulting settlement tx digests. Build the same effects under two
        // different offsets and assert the digests differ. If this ever fails, the F9
        // invariant test above provides zero coverage and needs to be re-examined.
        let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
        let effects = vec![
            effects_with_event_digest_write(0, 1, 0xAB),
            effects_with_event_digest_write(1, 2, 0xCD),
        ];
        let (digests_offset_0, _) = build_settlement_tx_digests(
            &protocol_config,
            &effects,
            /* checkpoint_seq */ 1,
            /* checkpoint_height */ 1,
            /* epoch */ 1,
            /* tx_index_offset */ 0,
        );
        let (digests_offset_99, _) = build_settlement_tx_digests(
            &protocol_config,
            &effects,
            1,
            1,
            1,
            /* tx_index_offset */ 99,
        );
        assert_ne!(
            digests_offset_0, digests_offset_99,
            "tx_index_offset must influence the settlement tx digest for EventDigest \
             accumulators; otherwise the F9 invariant test is vacuous"
        );
    }
}
