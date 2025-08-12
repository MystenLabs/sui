// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};

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

use sui_types::digests::Digest;
use sui_types::effects::{
    AccumulatorAddress, AccumulatorOperation, AccumulatorValue, AccumulatorWriteV1,
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
        builder: &mut ProgrammableTransactionBuilder,
    ) {
        let ty = ClassifiedType::classify(&address.ty);
        let address_arg = builder.pure(address.address).unwrap();
        let checkpoint_seq = 0u64; /* TODO: replace with actual checkpoint sequence number */

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
            AccumulatorValue::EventDigest(_, _) => Self::Events(vec![]),
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
            (Self::Events(commitments), AccumulatorValue::EventDigest(event_idx, digest)) => {
                commitments.push(EventCommitment::new(
                    checkpoint_seq,
                    transaction_idx,
                    event_idx,
                    digest,
                ));
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
}

impl AccumulatorSettlementTxBuilder {
    pub fn new(
        cache: Option<&dyn TransactionCacheRead>,
        ckpt_effects: &[TransactionEffects],
        tx_index_offset: u64,
    ) -> Self {
        let checkpoint_seq = 0u64; /* TODO: replace with actual checkpoint sequence number */

        let mut updates = BTreeMap::<_, _>::new();

        let mut addresses = HashMap::<_, _>::new();

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
                        entry.merge.accumulate_into(
                            value,
                            checkpoint_seq,
                            tx_index as u64 + tx_index_offset,
                        );
                    }
                    AccumulatorOperation::Split => {
                        entry.split.accumulate_into(
                            value,
                            checkpoint_seq,
                            tx_index as u64 + tx_index_offset,
                        );
                    }
                }
            }
        }

        Self { updates, addresses }
    }

    pub fn num_updates(&self) -> usize {
        self.updates.len()
    }

    /// Returns a unified map of accumulator changes for all accounts.
    /// The accumulator change for each account is merged from the merge and split operations.
    pub fn collect_accumulator_changes(&self) -> BTreeMap<AccumulatorObjId, i128> {
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

    // TODO(address-balances): This currently only creates a single accumulator update transaction.
    // To support multiple accumulator update transactions, we need to:
    // - have each transaction take the accumulator root as a "non-exclusive mutable" input
    // - each transaction writes out a set of fields that are disjoint from the others.
    // - a barrier transaction must be added to advance the version of the accumulator root object.
    //   The barrier transaction doesn't do any field writes. This is necessary in order to provide
    //   a consistent view of the system accumulator state. When the version of the accumulator
    //   root object is advanced, we know that all accumulator state updates prior to that version
    //   have been applied.
    pub fn build_tx(
        self,
        protocol_config: &ProtocolConfig,
        epoch: u64,
        accumulator_root_obj_initial_shared_version: SequenceNumber,
        checkpoint_height: u64,
    ) -> (
        Vec<TransactionKind>, /* settlements */
        TransactionKind,      /* barrier */
    ) {
        let Self { updates, addresses } = self;

        let mut pending_updates = Vec::new();
        let mut settlements = Vec::new();

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
            )
        };

        for (obj, update) in updates.into_iter() {
            pending_updates.push((obj, update));

            if pending_updates.len()
                == protocol_config
                    .max_updates_per_settlement_txn_as_option()
                    .unwrap_or(u32::MAX) as usize
            {
                settlements.push(build_one_settlement_txn(
                    settlements.len() as u64,
                    // pending_updates will be drained and can be re-used
                    &mut pending_updates,
                ));
            }
        }

        if !pending_updates.is_empty() {
            settlements.push(build_one_settlement_txn(
                settlements.len() as u64,
                &mut pending_updates,
            ));
        }

        // Now construct the barrier transaction
        let mut builder = ProgrammableTransactionBuilder::new();
        let root = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: SUI_ACCUMULATOR_ROOT_OBJECT_ID,
                initial_shared_version: accumulator_root_obj_initial_shared_version,
                mutability: SharedObjectMutability::Mutable,
            }))
            .unwrap();

        Self::add_prologue(
            &mut builder,
            root,
            epoch,
            checkpoint_height,
            settlements.len() as u64,
            0u64,
            0u64,
        );

        let barrier = TransactionKind::ProgrammableSystemTransaction(builder.finish());

        (settlements, barrier)
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
            MergedValue::add_move_call(merged_value, split_value, root, address, &mut builder);
        }

        TransactionKind::ProgrammableSystemTransaction(builder.finish())
    }
}
