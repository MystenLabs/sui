// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::fuzz::TransactionKindMutator;
use rand::seq::SliceRandom;
use sui_types::transaction::TransactionKind;
use tracing::info;

pub struct DropRandomCommands {
    pub rng: rand::rngs::StdRng,
    pub num_mutations_per_base_left: u64,
}

impl TransactionKindMutator for DropRandomCommands {
    fn mutate(&mut self, transaction_kind: &TransactionKind) -> Option<TransactionKind> {
        if self.num_mutations_per_base_left == 0 {
            // Nothing else to do
            return None;
        }

        self.num_mutations_per_base_left -= 1;
        if let TransactionKind::ProgrammableTransaction(mut p) = transaction_kind.clone() {
            if p.commands.is_empty() {
                return None;
            }
            p.commands = p
                .commands
                .choose_multiple(&mut self.rng, p.commands.len() - 1)
                .cloned()
                .collect();
            info!("Mutation: Dropping random commands");
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
