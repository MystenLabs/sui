// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::committee::Committee;

pub(crate) struct EpochCache {
    completed_committees: Vec<(u64, u64, Committee)>,
    current_epoch_number: u64,
    current_committee: Committee,
    current_epoch_start_checkpoint: u64,
}

impl EpochCache {
    pub fn new(genesis_committee: Committee) -> Self {
        Self {
            completed_committees: vec![],
            current_epoch_number: 0,
            current_committee: genesis_committee,
            current_epoch_start_checkpoint: 0,
        }
    }

    pub fn get_committee_for_checkpoint(&self, checkpoint_seq: u64) -> Option<Committee> {
        if checkpoint_seq >= self.current_epoch_start_checkpoint {
            return Some(self.current_committee.clone());
        }

        self.completed_committees
            .binary_search_by(|(start, end, _)| {
                if checkpoint_seq < *start {
                    std::cmp::Ordering::Greater
                } else if checkpoint_seq > *end {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok()
            .and_then(|idx| self.completed_committees.get(idx))
            .map(|(_, _, c)| c.clone())
    }

    pub fn current_epoch(&self) -> u64 {
        self.current_epoch_number
    }

    pub fn current_epoch_start_checkpoint(&self) -> u64 {
        self.current_epoch_start_checkpoint
    }

    pub fn current_committee(&self) -> &Committee {
        &self.current_committee
    }

    pub fn apply_ratchet_update(
        &mut self,
        old_epoch_start: u64,
        end_of_epoch_checkpoint: u64,
        old_committee: Committee,
        new_committee: Committee,
    ) {
        self.completed_committees
            .push((old_epoch_start, end_of_epoch_checkpoint, old_committee));

        self.current_epoch_number += 1;
        self.current_committee = new_committee;
        self.current_epoch_start_checkpoint = end_of_epoch_checkpoint + 1;
    }
}
