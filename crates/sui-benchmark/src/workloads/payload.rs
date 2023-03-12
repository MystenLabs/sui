// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::messages::VerifiedTransaction;

use crate::ExecutionEffects;
use rand::{prelude::*, rngs::OsRng};
use rand_distr::WeightedAliasIndex;

use crate::workloads::workload::WorkloadType;

pub trait Payload: Send + Sync + std::fmt::Debug {
    fn make_new_payload(&mut self, effects: &ExecutionEffects);
    fn make_transaction(&mut self) -> VerifiedTransaction;
    fn get_workload_type(&self) -> WorkloadType;
}

#[derive(Debug)]
pub struct CombinationPayload {
    pub payloads: Vec<Box<dyn Payload>>,
    pub dist: WeightedAliasIndex<u32>,
    pub curr_index: usize,
    pub rng: OsRng,
}

impl Payload for CombinationPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        for (pos, e) in self.payloads.iter_mut().enumerate() {
            if pos == self.curr_index {
                e.make_new_payload(effects);
            }
        }
        let mut rng = self.rng;
        let next_index = self.dist.sample(&mut rng);
        self.curr_index = next_index;
    }
    fn make_transaction(&mut self) -> VerifiedTransaction {
        let curr = self.payloads.get_mut(self.curr_index).unwrap();
        curr.make_transaction()
    }
    fn get_workload_type(&self) -> WorkloadType {
        self.payloads
            .get(self.curr_index)
            .unwrap()
            .get_workload_type()
    }
}
