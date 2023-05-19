// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::{seq::SliceRandom, SeedableRng};
use sui_types::transaction::TransactionKind;

use crate::fuzz::TransactionKindMutator;

pub mod drop_random_command_suffix;
pub mod drop_random_commands;
pub mod shuffle_command_inputs;
pub mod shuffle_commands;
pub mod shuffle_transaction_inputs;
pub mod shuffle_types;

// The number of times that we will try to select a different mutator if the selected one is unable
// to be applied for some reason.
const NUM_TRIES: u64 = 5;

// Combiners for `TransactionKindMutator`s:
// * `RandomMutator` will select a random mutator from a list of mutators
// * `ChainedMutator` will apply a list of mutators in sequence. If a given mutator doesn't apply
//   it will be skipped but other mutations both before and after the failed mutator may still be applied.
pub struct RandomMutator {
    pub rng: rand::rngs::StdRng,
    pub mutators: Vec<Box<dyn TransactionKindMutator + Send + Sync>>,
    pub num_tries: u64,
}

pub struct ChainedMutator {
    pub mutators: Vec<Box<dyn TransactionKindMutator>>,
}

impl RandomMutator {
    pub fn new() -> Self {
        Self {
            rng: rand::rngs::StdRng::from_seed([0u8; 32]),
            mutators: vec![],
            num_tries: NUM_TRIES,
        }
    }

    pub fn add_mutator(&mut self, mutator: Box<dyn TransactionKindMutator + Send + Sync>) {
        self.mutators.push(mutator);
    }

    pub fn select_mutator(&mut self) -> Option<&mut Box<dyn TransactionKindMutator + Send + Sync>> {
        self.mutators.choose_mut(&mut self.rng)
    }
}

impl Default for RandomMutator {
    fn default() -> Self {
        Self::new()
    }
}

impl TransactionKindMutator for RandomMutator {
    fn mutate(&mut self, transaction_kind: &TransactionKind) -> Option<TransactionKind> {
        for _ in 0..self.num_tries {
            if let Some(mutator) = self.select_mutator() {
                return mutator.mutate(transaction_kind);
            }
        }
        None
    }

    fn reset(&mut self, mutations_per_base: u64) {
        for mutator in self.mutators.iter_mut() {
            mutator.reset(mutations_per_base);
        }
    }
}

impl ChainedMutator {
    pub fn new() -> Self {
        Self { mutators: vec![] }
    }

    pub fn add_mutator(&mut self, mutator: Box<dyn TransactionKindMutator>) {
        self.mutators.push(mutator);
    }
}

impl Default for ChainedMutator {
    fn default() -> Self {
        Self::new()
    }
}

impl TransactionKindMutator for ChainedMutator {
    fn mutate(&mut self, transaction_kind: &TransactionKind) -> Option<TransactionKind> {
        let mut mutated = transaction_kind.clone();
        let mut num_mutations = 0;

        for mutator in self.mutators.iter_mut() {
            if let Some(new_mutated) = mutator.mutate(&mutated) {
                num_mutations += 1;
                mutated = new_mutated;
            }
        }

        if num_mutations == 0 {
            None
        } else {
            Some(mutated)
        }
    }

    fn reset(&mut self, mutations_per_base: u64) {
        for mutator in self.mutators.iter_mut() {
            mutator.reset(mutations_per_base);
        }
    }
}

pub fn base_fuzzers(num_mutations: u64) -> RandomMutator {
    let mut mutator = RandomMutator::new();
    mutator.add_mutator(Box::new(shuffle_commands::ShuffleCommands {
        rng: rand::rngs::StdRng::from_seed([0u8; 32]),
        num_mutations_per_base_left: num_mutations,
    }));
    mutator.add_mutator(Box::new(shuffle_types::ShuffleTypes {
        rng: rand::rngs::StdRng::from_seed([0u8; 32]),
        num_mutations_per_base_left: num_mutations,
    }));
    mutator.add_mutator(Box::new(shuffle_command_inputs::ShuffleCommandInputs {
        rng: rand::rngs::StdRng::from_seed([0u8; 32]),
        num_mutations_per_base_left: num_mutations,
    }));
    mutator.add_mutator(Box::new(
        shuffle_transaction_inputs::ShuffleTransactionInputs {
            rng: rand::rngs::StdRng::from_seed([0u8; 32]),
            num_mutations_per_base_left: num_mutations,
        },
    ));
    mutator.add_mutator(Box::new(drop_random_commands::DropRandomCommands {
        rng: rand::rngs::StdRng::from_seed([0u8; 32]),
        num_mutations_per_base_left: num_mutations,
    }));
    mutator.add_mutator(Box::new(drop_random_command_suffix::DropCommandSuffix {
        rng: rand::rngs::StdRng::from_seed([0u8; 32]),
        num_mutations_per_base_left: num_mutations,
    }));
    mutator
}
