// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

/// Operational configurations of a consensus authority.
///
/// All fields should tolerate inconsistencies among authorities, without affecting safety of the
/// protocol. Otherwise, they need to be part of Sui protocol config or epoch state on-chain.
///
/// NOTE: fields with default values are specified in the serde default functions. Most operators
/// should not need to specify any field, except db_path.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Parameters {
    /// The database path.
    /// Required.
    pub db_path: Option<PathBuf>,

    /// Time to wait for parent round leader before sealing a block.
    #[serde(default = "Parameters::default_leader_timeout")]
    pub leader_timeout: Duration,

    /// Minimum delay between rounds, to avoid generating too many rounds when latency is low.
    /// This is especially necessary for tests running locally.
    /// If setting a non-default value, it should be set low enough to avoid reducing
    /// round rate and increasing latency in realistic and distributed configurations.
    #[serde(default = "Parameters::default_min_round_delay")]
    pub min_round_delay: Duration,

    /// Maximum forward time drift (how far in future) allowed for received blocks.
    #[serde(default = "Parameters::default_max_forward_time_drift")]
    pub max_forward_time_drift: Duration,

    /// Number of blocks to fetch per request.
    #[serde(default = "Parameters::default_max_blocks_per_fetch")]
    pub max_blocks_per_fetch: usize,

    /// The number of rounds of blocks to be kept in the Dag state cache per authority. The larger
    /// the number the more the blocks that will be kept in memory allowing minimising any potential
    /// disk access.
    /// Value should be at minimum 50 rounds to ensure node performance, but being too large can be
    /// expensive in memory usage.
    #[serde(default = "Parameters::default_dag_state_cached_rounds")]
    pub dag_state_cached_rounds: u32,

    // Number of authorities commit syncer fetches in parallel.
    // Both commits in a range and blocks referenced by the commits are fetched per authority.
    #[serde(default = "Parameters::default_commit_sync_parallel_fetches")]
    pub commit_sync_parallel_fetches: usize,

    // Number of commits to fetch in a batch, also the maximum number of commits returned per fetch.
    // If this value is set too small, fetching becomes inefficient.
    // If this value is set too large, it can result in load imbalance and stragglers.
    #[serde(default = "Parameters::default_commit_sync_batch_size")]
    pub commit_sync_batch_size: u32,

    // Maximum number of commit batches being fetched, before throttling
    // of outgoing commit fetches starts.
    #[serde(default = "Parameters::default_commit_sync_batches_ahead")]
    pub commit_sync_batches_ahead: usize,

    /// Anemo network settings.
    #[serde(default = "AnemoParameters::default")]
    pub anemo: AnemoParameters,

    /// Tonic network settings.
    #[serde(default = "TonicParameters::default")]
    pub tonic: TonicParameters,

    /// Time to wait during node start up until the node has synced the last proposed block via the
    /// network peers. When set to `0` the sync mechanism is disabled. This property is meant to be
    /// used for amnesia recovery.
    #[serde(default = "Parameters::default_sync_last_proposed_block_timeout")]
    pub sync_last_proposed_block_timeout: Duration,
}

impl Parameters {
    pub(crate) fn default_leader_timeout() -> Duration {
        Duration::from_millis(250)
    }

    pub(crate) fn default_min_round_delay() -> Duration {
        if cfg!(msim) || std::env::var("__TEST_ONLY_CONSENSUS_USE_LONG_MIN_ROUND_DELAY").is_ok() {
            // Checkpoint building and execution cannot keep up with high commit rate in simtests,
            // leading to long reconfiguration delays. This is because simtest is single threaded,
            // and spending too much time in consensus can lead to starvation elsewhere.
            Duration::from_millis(400)
        } else {
            Duration::from_millis(50)
        }
    }

    pub(crate) fn default_max_forward_time_drift() -> Duration {
        Duration::from_millis(500)
    }

    pub(crate) fn default_dag_state_cached_rounds() -> u32 {
        if cfg!(msim) {
            // Exercise reading blocks from store.
            5
        } else {
            500
        }
    }

    pub(crate) fn default_commit_sync_parallel_fetches() -> usize {
        20
    }

    pub(crate) fn default_commit_sync_batch_size() -> u32 {
        if cfg!(msim) {
            // Exercise commit sync.
            5
        } else {
            100
        }
    }

    pub(crate) fn default_max_blocks_per_fetch() -> usize {
        if cfg!(msim) {
            // Exercise hitting blocks per fetch limit.
            10
        } else {
            1000
        }
    }

    pub(crate) fn default_commit_sync_batches_ahead() -> usize {
        200
    }

    pub(crate) fn default_sync_last_proposed_block_timeout() -> Duration {
        Duration::ZERO
    }

    pub fn is_sync_last_proposed_block_enabled(&self) -> bool {
        !self.sync_last_proposed_block_timeout.is_zero()
    }
}

impl Default for Parameters {
    fn default() -> Self {
        Self {
            db_path: None,
            leader_timeout: Parameters::default_leader_timeout(),
            min_round_delay: Parameters::default_min_round_delay(),
            max_forward_time_drift: Parameters::default_max_forward_time_drift(),
            dag_state_cached_rounds: Parameters::default_dag_state_cached_rounds(),
            max_blocks_per_fetch: Parameters::default_max_blocks_per_fetch(),
            sync_last_proposed_block_timeout: Parameters::default_sync_last_proposed_block_timeout(
            ),
            commit_sync_parallel_fetches: Parameters::default_commit_sync_parallel_fetches(),
            commit_sync_batch_size: Parameters::default_commit_sync_batch_size(),
            commit_sync_batches_ahead: Parameters::default_commit_sync_batches_ahead(),
            anemo: AnemoParameters::default(),
            tonic: TonicParameters::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AnemoParameters {
    /// Size in bytes above which network messages are considered excessively large. Excessively
    /// large messages will still be handled, but logged and reported in metrics for debugging.
    ///
    /// If unspecified, this will default to 8 MiB.
    #[serde(default = "AnemoParameters::default_excessive_message_size")]
    pub excessive_message_size: usize,
}

impl AnemoParameters {
    fn default_excessive_message_size() -> usize {
        8 << 20
    }
}

impl Default for AnemoParameters {
    fn default() -> Self {
        Self {
            excessive_message_size: AnemoParameters::default_excessive_message_size(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TonicParameters {
    /// Keepalive interval and timeouts for both client and server.
    ///
    /// If unspecified, this will default to 5s.
    #[serde(default = "TonicParameters::default_keepalive_interval")]
    pub keepalive_interval: Duration,

    /// Size of various per-connection buffers.
    ///
    /// If unspecified, this will default to 32MiB.
    #[serde(default = "TonicParameters::default_connection_buffer_size")]
    pub connection_buffer_size: usize,

    /// Messages over this size threshold will increment a counter.
    ///
    /// If unspecified, this will default to 16MiB.
    #[serde(default = "TonicParameters::default_excessive_message_size")]
    pub excessive_message_size: usize,

    /// Hard message size limit for both requests and responses.
    /// This value is higher than strictly necessary, to allow overheads.
    /// Message size targets and soft limits are computed based on this value.
    ///
    /// If unspecified, this will default to 1GiB.
    #[serde(default = "TonicParameters::default_message_size_limit")]
    pub message_size_limit: usize,
}

impl TonicParameters {
    fn default_keepalive_interval() -> Duration {
        Duration::from_secs(5)
    }

    fn default_connection_buffer_size() -> usize {
        32 << 20
    }

    fn default_excessive_message_size() -> usize {
        16 << 20
    }

    fn default_message_size_limit() -> usize {
        64 << 20
    }
}

impl Default for TonicParameters {
    fn default() -> Self {
        Self {
            keepalive_interval: TonicParameters::default_keepalive_interval(),
            connection_buffer_size: TonicParameters::default_connection_buffer_size(),
            excessive_message_size: TonicParameters::default_excessive_message_size(),
            message_size_limit: TonicParameters::default_message_size_limit(),
        }
    }
}
