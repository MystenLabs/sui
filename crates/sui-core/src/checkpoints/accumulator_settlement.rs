// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{btree_map::Entry, BTreeMap};

use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    committee::EpochId,
    effects::{AccumulatorOperation, AccumulatorUpdateValue, AccumulatorWriteV1},
    executable_transaction::VerifiedExecutableTransaction,
    transaction::VerifiedTransaction,
};

pub(crate) struct AccumulatorStateUpdateTxBuilder {
    merges: BTreeMap<ObjectID, AccumulatorUpdateValue>,
    splits: BTreeMap<ObjectID, AccumulatorUpdateValue>,
}

impl AccumulatorStateUpdateTxBuilder {
    pub fn new() -> Self {
        Self {
            merges: BTreeMap::new(),
            splits: BTreeMap::new(),
        }
    }

    pub fn add_accumulator_update(&mut self, id: ObjectID, write: AccumulatorWriteV1) {
        let group = match write.operation {
            AccumulatorOperation::Merge => &mut self.merges,
            AccumulatorOperation::Split => &mut self.splits,
        };
        match group.entry(id) {
            Entry::Vacant(entry) => {
                entry.insert(write.update_value);
            }
            Entry::Occupied(mut entry) => match (entry.get(), write.update_value) {
                (AccumulatorUpdateValue::Integer(a), AccumulatorUpdateValue::Integer(b)) => {
                    entry.insert(AccumulatorUpdateValue::Integer(a + b));
                }
            },
        }
    }

    pub fn build(
        self,
        epoch: EpochId,
        consensus_round: u64,
        initial_shared_version: SequenceNumber,
    ) -> VerifiedExecutableTransaction {
        VerifiedExecutableTransaction::new_system(
            VerifiedTransaction::new_accumulator_state_update(
                epoch,
                consensus_round,
                initial_shared_version,
                self.merges.into_iter().collect(),
                self.splits.into_iter().collect(),
            ),
            epoch,
        )
    }
}

// TODO: Add tests for this.
