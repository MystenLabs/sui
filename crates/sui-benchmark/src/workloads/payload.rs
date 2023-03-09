// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::messages::VerifiedTransaction;

use crate::ExecutionEffects;
use rand::{prelude::*, rngs::OsRng};
use rand_distr::WeightedAliasIndex;

use crate::workloads::workload::WorkloadType;

pub trait Payload: Send + Sync + std::fmt::Debug {
    fn make_new_payload(self: Box<Self>, effects: &ExecutionEffects) -> Box<dyn Payload>;
    fn make_transaction(&self) -> VerifiedTransaction;
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
    fn make_new_payload(self: Box<Self>, effects: &ExecutionEffects) -> Box<dyn Payload> {
        let mut new_payloads = vec![];
        for (pos, e) in self.payloads.into_iter().enumerate() {
            if pos == self.curr_index {
                let updated = e.make_new_payload(effects);
                new_payloads.push(updated);
            } else {
                new_payloads.push(e);
            }
        }
        let mut rng = self.rng;
        let next_index = self.dist.sample(&mut rng);
        Box::new(CombinationPayload {
            payloads: new_payloads,
            dist: self.dist,
            curr_index: next_index,
            rng: self.rng,
        })
    }
    fn make_transaction(&self) -> VerifiedTransaction {
        let curr = self.payloads.get(self.curr_index).unwrap();
        curr.make_transaction()
    }
    fn get_workload_type(&self) -> WorkloadType {
        self.payloads
            .get(self.curr_index)
            .unwrap()
            .get_workload_type()
    }
}
