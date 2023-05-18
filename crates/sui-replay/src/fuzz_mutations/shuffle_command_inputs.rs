// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::fuzz::TransactionKindMutator;
use rand::seq::SliceRandom;
use sui_types::transaction::{Command, TransactionKind};
use tracing::info;

pub struct ShuffleCommandInputs {
    pub rng: rand::rngs::StdRng,
    pub num_mutations_per_base_left: u64,
}

impl ShuffleCommandInputs {
    fn shuffle_command(&mut self, command: &mut Command) {
        match command {
            Command::MakeMoveVec(_, ref mut args)
            | Command::MergeCoins(_, ref mut args)
            | Command::SplitCoins(_, ref mut args)
            | Command::TransferObjects(ref mut args, _) => {
                args.shuffle(&mut self.rng);
            }
            Command::MoveCall(ref mut pt) => pt.arguments.shuffle(&mut self.rng),
            Command::Publish(_, _) => (),
            Command::Upgrade(_, _, _, _) => (),
        }
    }
}

impl TransactionKindMutator for ShuffleCommandInputs {
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
            info!("Mutation: Shuffling command inputs");
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
