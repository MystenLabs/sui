// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};

use mysten_common::fatal;
use sui_types::accumulator_event::AccumulatorEvent;
use sui_types::accumulator_root::{
    AccumulatorObjId, ACCUMULATOR_ROOT_SETTLEMENT_PROLOGUE_FUNC, ACCUMULATOR_ROOT_SETTLE_U128_FUNC,
    ACCUMULATOR_SETTLEMENT_MODULE,
};
use sui_types::balance::{BALANCE_MODULE_NAME, BALANCE_STRUCT_NAME};
use sui_types::effects::{
    AccumulatorAddress, AccumulatorOperation, AccumulatorValue, AccumulatorWriteV1,
    TransactionEffects, TransactionEffectsAPI,
};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{Argument, CallArg, ObjectArg, TransactionKind};
use sui_types::{
    TypeTag, SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_FRAMEWORK_ADDRESS, SUI_FRAMEWORK_PACKAGE_ID,
};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::epoch_start_configuration::EpochStartConfigTrait;
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
            _ => fatal!("invalid merge {:?} {:?}", merge, split),
        }
    }
}

impl From<MergedValueIntermediate> for MergedValue {
    fn from(value: MergedValueIntermediate) -> Self {
        match value {
            MergedValueIntermediate::SumU128(v) => MergedValue::SumU128(v),
            MergedValueIntermediate::SumU128U128(v1, v2) => MergedValue::SumU128U128(v1, v2),
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
#[derive(Debug, Copy, Clone)]
enum MergedValueIntermediate {
    SumU128(u128),
    SumU128U128(u128, u128),
}

impl MergedValueIntermediate {
    // Create a zero value with the appropriate type for the accumulator value.
    fn zero(value: &AccumulatorValue) -> Self {
        match value {
            AccumulatorValue::Integer(_) => Self::SumU128(0),
            AccumulatorValue::IntegerTuple(_, _) => Self::SumU128U128(0, 0),
        }
    }

    fn accumulate_into(&mut self, value: AccumulatorValue) {
        match (self, value) {
            (Self::SumU128(v1), AccumulatorValue::Integer(v2)) => *v1 += v2 as u128,
            (Self::SumU128U128(v1, v2), AccumulatorValue::IntegerTuple(w1, w2)) => {
                *v1 += w1 as u128;
                *v2 += w2 as u128;
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
}

pub(crate) struct AccumulatorSettlementTxBuilder {
    updates: HashMap<AccumulatorObjId, Update>,
    addresses: HashMap<AccumulatorObjId, AccumulatorAddress>,

    total_input_sui: u64,
    total_output_sui: u64,
}

impl AccumulatorSettlementTxBuilder {
    pub fn new(
        cache: Option<&dyn TransactionCacheRead>,
        ckpt_effects: &[TransactionEffects],
    ) -> Self {
        let mut updates = HashMap::<_, _>::new();

        let mut addresses = HashMap::<_, _>::new();

        let mut total_input_sui = 0;
        let mut total_output_sui = 0;

        for effect in ckpt_effects {
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
                let (input_sui, output_sui) = event.total_sui_in_event();
                // The input to the settlement is the sum of the outputs of accumulator events (i.e. deposits).
                total_input_sui += output_sui;
                // and the output of the settlement is the sum of the inputs (i.e. withdraws).
                total_output_sui += input_sui;

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
                        merge: zero,
                        split: zero,
                    }
                });

                match operation {
                    AccumulatorOperation::Merge => {
                        entry.merge.accumulate_into(value);
                    }
                    AccumulatorOperation::Split => {
                        entry.split.accumulate_into(value);
                    }
                }
            }
        }

        Self {
            updates,
            addresses,
            total_input_sui,
            total_output_sui,
        }
    }

    pub fn num_updates(&self) -> usize {
        self.updates.len()
    }

    /// Returns a unified map of accumulator changes for all accounts.
    /// The accumulator change for each account is merged from the merge and split operations.
    pub fn collect_accumulator_changes(&self) -> BTreeMap<AccumulatorObjId, i128> {
        self.updates
            .iter()
            .map(|(object_id, update)| match (update.merge, update.split) {
                (
                    MergedValueIntermediate::SumU128(merge),
                    MergedValueIntermediate::SumU128(split),
                ) => (*object_id, merge as i128 - split as i128),
                _ => todo!(),
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
        epoch_store: &AuthorityPerEpochStore,
        checkpoint_height: u64,
    ) -> Vec<TransactionKind> {
        let epoch = epoch_store.epoch();
        let accumulator_root_obj_initial_shared_version = epoch_store
            .epoch_start_config()
            .accumulator_root_obj_initial_shared_version()
            .expect("accumulator root object must exist");

        let mut builder = ProgrammableTransactionBuilder::new();

        let root = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: SUI_ACCUMULATOR_ROOT_OBJECT_ID,
                initial_shared_version: accumulator_root_obj_initial_shared_version,
                mutable: true,
            }))
            .unwrap();

        let epoch_arg = builder.pure(epoch).unwrap();
        let checkpoint_height_arg = builder.pure(checkpoint_height).unwrap();
        let idx_arg = builder.pure(0u64).unwrap();
        let total_input_sui_arg = builder.pure(self.total_input_sui).unwrap();
        let total_output_sui_arg = builder.pure(self.total_output_sui).unwrap();
        tracing::debug!("total_input_sui: {}", self.total_input_sui);
        tracing::debug!("total_output_sui: {}", self.total_output_sui);

        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ACCUMULATOR_SETTLEMENT_MODULE.into(),
            ACCUMULATOR_ROOT_SETTLEMENT_PROLOGUE_FUNC.into(),
            vec![],
            vec![
                epoch_arg,
                checkpoint_height_arg,
                idx_arg,
                total_input_sui_arg,
                total_output_sui_arg,
            ],
        );

        for (accumulator_obj, update) in self.updates {
            let Update { merge, split } = update;
            let address = self.addresses.get(&accumulator_obj).unwrap();
            let merged_value = MergedValue::from(merge);
            let split_value = MergedValue::from(split);
            MergedValue::add_move_call(merged_value, split_value, root, address, &mut builder);
        }

        vec![TransactionKind::ProgrammableSystemTransaction(
            builder.finish(),
        )]
    }
}
