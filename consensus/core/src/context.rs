// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::SystemTime};

use consensus_config::{AuthorityIndex, Committee, Parameters};
use consensus_config::{NetworkKeyPair, ProtocolKeyPair};
use consensus_types::block::BlockTimestampMs;
use sui_protocol_config::ProtocolConfig;
use tempfile::TempDir;
use tokio::time::Instant;

use crate::metrics::Metrics;
use crate::metrics::test_metrics;

/// Context contains per-epoch configuration and metrics shared by all components
/// of this authority.
#[derive(Clone)]
pub struct Context {
    /// Timestamp of the start of the current epoch.
    pub epoch_start_timestamp_ms: u64,
    /// Index of this authority in the committee.
    pub own_index: AuthorityIndex,
    /// Committee of the current epoch.
    pub committee: Committee,
    /// Parameters of this authority.
    pub parameters: Parameters,
    /// Protocol configuration of current epoch.
    pub protocol_config: ProtocolConfig,
    /// Metrics of this authority.
    pub metrics: Arc<Metrics>,
    /// Access to local clock
    pub clock: Arc<Clock>,
}

impl Context {
    pub(crate) fn new(
        epoch_start_timestamp_ms: u64,
        own_index: AuthorityIndex,
        committee: Committee,
        parameters: Parameters,
        protocol_config: ProtocolConfig,
        metrics: Arc<Metrics>,
        clock: Arc<Clock>,
    ) -> Self {
        Self {
            epoch_start_timestamp_ms,
            own_index,
            committee,
            parameters,
            protocol_config,
            metrics,
            clock,
        }
    }

    /// Create a test context with a committee of given size and even stake
    pub fn new_for_test(committee_size: usize) -> (Self, Vec<(NetworkKeyPair, ProtocolKeyPair)>) {
        Self::new_with_test_options(committee_size, true)
    }

    /// Create a test context with a committee of given size and even stake
    pub fn new_with_test_options(
        committee_size: usize,
        unused_port: bool,
    ) -> (Self, Vec<(NetworkKeyPair, ProtocolKeyPair)>) {
        let (committee, keypairs) = consensus_config::local_committee_and_keys_with_test_options(
            0,
            vec![1; committee_size],
            unused_port,
        );
        let metrics = test_metrics();
        let temp_dir = TempDir::new().unwrap();
        let clock = Arc::new(Clock::default());

        let context = Context::new(
            0,
            AuthorityIndex::new_for_test(0),
            committee,
            Parameters {
                db_path: temp_dir.keep(),
                ..Default::default()
            },
            ProtocolConfig::get_for_max_version_UNSAFE(),
            metrics,
            clock,
        );
        (context, keypairs)
    }

    pub fn with_epoch_start_timestamp_ms(mut self, epoch_start_timestamp_ms: u64) -> Self {
        self.epoch_start_timestamp_ms = epoch_start_timestamp_ms;
        self
    }

    pub fn with_authority_index(mut self, authority: AuthorityIndex) -> Self {
        self.own_index = authority;
        self
    }

    pub fn with_committee(mut self, committee: Committee) -> Self {
        self.committee = committee;
        self
    }

    pub fn with_parameters(mut self, parameters: Parameters) -> Self {
        self.parameters = parameters;
        self
    }

    pub fn with_protocol_config(mut self, protocol_config: ProtocolConfig) -> Self {
        self.protocol_config = protocol_config;
        self
    }
}

/// A clock that allows to derive the current UNIX system timestamp while guaranteeing that timestamp
/// will be monotonically incremented, tolerating ntp and system clock changes and corrections.
/// Explicitly avoid to make `[Clock]` cloneable to ensure that a single instance is shared behind an `[Arc]`
/// wherever is needed in order to make sure that consecutive calls to receive the system timestamp
/// will remain monotonically increasing.
pub struct Clock {
    initial_instant: Instant,
    initial_system_time: SystemTime,
    // `clock_drift` should be used only for testing
    clock_drift: BlockTimestampMs,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            initial_instant: Instant::now(),
            initial_system_time: SystemTime::now(),
            clock_drift: 0,
        }
    }
}

impl Clock {
    pub fn new_for_test(clock_drift: BlockTimestampMs) -> Self {
        Self {
            initial_instant: Instant::now(),
            initial_system_time: SystemTime::now(),
            clock_drift,
        }
    }

    // Returns the current time expressed as UNIX timestamp in milliseconds.
    // Calculated with Tokio Instant to ensure monotonicity,
    // and to allow testing with tokio clock.
    pub(crate) fn timestamp_utc_ms(&self) -> BlockTimestampMs {
        if cfg!(not(any(msim, test))) {
            assert_eq!(
                self.clock_drift, 0,
                "Clock drift should not be set in non testing environments."
            );
        }

        let now: Instant = Instant::now();
        let monotonic_system_time = self
            .initial_system_time
            .checked_add(
                now.checked_duration_since(self.initial_instant)
                    .unwrap_or_else(|| {
                        panic!(
                            "current instant ({:?}) < initial instant ({:?})",
                            now, self.initial_instant
                        )
                    }),
            )
            .expect("Computing system time should not overflow");
        monotonic_system_time
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_else(|_| {
                panic!(
                    "system time ({:?}) < UNIX_EPOCH ({:?})",
                    monotonic_system_time,
                    SystemTime::UNIX_EPOCH,
                )
            })
            .as_millis() as BlockTimestampMs
            + self.clock_drift
    }
}
