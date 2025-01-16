// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use simple_moving_average::SingleSumSMA;
use sui_types::{
    executable_transaction::VerifiedExecutableTransaction,
    messages_consensus::EntryPointKey,
    transaction::{ProgrammableTransaction, TransactionData},
};

use crate::consensus_adapter::ConsensusAdapter;

const LOCAL_OBSERVATION_WINDOW_SIZE: usize = 10;

pub struct ExecutionTimeEstimator {
    consensus_adapter: Arc<ConsensusAdapter>,

    local_observations: HashMap<
        ExecutionTimeObservationKey,
        SingleSumSMA<Duration, u32, LOCAL_OBSERVATION_WINDOW_SIZE>,
    >,
    // TODO-DNS this also needs to be saved to per epoch DBs for crash recovery
    // committee index is vector key
    // TODO-DNS: can we have a more efficient data structure for computing medians?
    consensus_observations: HashMap<ExecutionTimeObservationKey, Vec<Duration>>,
}

impl ExecutionTimeEstimator {
    pub fn new() -> Self {
        // TODO-DNS maybe prepopulate with EndOfEpochData from last epoch
        todo!()
        // Self {}
    }

    // Used by execution to report observed per-entry-point execution times to the estimator.
    // Updates moving average and submits observation to consensus if local observation differs
    // from consensus median.
    pub fn record_local_observations(
        &mut self,
        observations: Vec<(ExecutionTimeObservationKey, Duration)>,
    ) {
        todo!()
    }

    // Do these all at the begininng or end of commit, but not interleaved
    pub fn process_observations_from_consensus(
        &mut self,
        source: ValidatorIndex,
        observations: Vec<(ExecutionTimeObservationKey, Duration)>,
    ) {
        todo!()
    }

    pub fn get_estimate(&self, tx: &TransactionData) -> Duration {
        todo!()
    }
}
