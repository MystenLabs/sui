// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::fuzz::TransactionKindMutator;
use rand::Rng;
use sui_types::transaction::TransactionKind;
use tracing::info;

pub struct DropCommandSuffix {
    pub rng: rand::rngs::StdRng,
    pub num_mutations_per_base_left: u64,
}

impl TransactionKindMutator for DropCommandSuffix {
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
            let slice_index = self.rng.gen_range(0..p.commands.len());
            p.commands.truncate(slice_index);
            info!("Mutation: Dropping command suffix");
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
