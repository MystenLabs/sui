// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::fuzz::TransactionKindMutator;
use rand::seq::SliceRandom;
use sui_types::transaction::{Command, TransactionKind};
use tracing::info;

pub struct ShuffleTypes {
    pub rng: rand::rngs::StdRng,
    pub num_mutations_per_base_left: u64,
}

impl ShuffleTypes {
    fn shuffle_command(&mut self, command: &mut Command) {
        if let Command::MoveCall(ref mut pt) = command {
            pt.type_arguments.shuffle(&mut self.rng)
        }
    }
}

impl TransactionKindMutator for ShuffleTypes {
    fn mutate(&mut self, transaction_kind: &TransactionKind) -> Option<TransactionKind> {
        if self.num_mutations_per_base_left == 0 {
            // Nothing else to do
            return None;
        }

        self.num_mutations_per_base_left -= 1;
        if let TransactionKind::ProgrammableTransaction(mut p) = transaction_kind.clone() {
            for command in &mut p.commands {
                self.shuffle_command(command);
            }
            info!("Mutation: Shuffling types");
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
