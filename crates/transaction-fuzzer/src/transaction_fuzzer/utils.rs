// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;

// TODO: actually calculate the rewards here
pub fn calculate_rewards(
    _initial_amount: u64,
    start_epoch: u64,
    end_epoch: u64,
    _system_states: &BTreeMap<u64, SuiSystemStateSummary>,
) -> Option<u64> {
    if start_epoch >= end_epoch {
        return None;
    }
    std::todo!()
}
