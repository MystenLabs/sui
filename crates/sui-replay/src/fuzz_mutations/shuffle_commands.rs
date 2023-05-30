// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::fuzz::TransactionKindMutator;
use rand::seq::SliceRandom;
use sui_types::transaction::TransactionKind;
use tracing::info;

pub struct ShuffleCommands {
    pub rng: rand::rngs::StdRng,
    pub num_mutations_per_base_left: u64,
}

impl TransactionKindMutator for ShuffleCommands {
    fn mutate(&mut self, transaction_kind: &TransactionKind) -> Option<TransactionKind> {
        if self.num_mutations_per_base_left == 0 {
            // Nothing else to do
            return None;
        }

        self.num_mutations_per_base_left -= 1;
        if let TransactionKind::ProgrammableTransaction(mut p) = transaction_kind.clone() {
            p.commands.shuffle(&mut self.rng);
            info!("Mutation: Shuffling commands");
            Some(TransactionKind::ProgrammableTransaction(p))
        } else {
            // Other types not supported yet
            None
        }
    }

    fn reset(&mut self, mutations_per_base: u64) {
        self.num_mutations_per_base_left = mutations_per_base;
    }
}
