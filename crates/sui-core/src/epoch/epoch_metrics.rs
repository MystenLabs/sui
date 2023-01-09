// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::histogram::Histogram;
use prometheus::{register_int_gauge_with_registry, IntGauge, Registry};
use std::sync::Arc;

pub struct EpochMetrics {
    /// The current epoch ID. This is updated only when the AuthorityState finishes reconfiguration.
    pub current_epoch: IntGauge,

    /// Total duration of the epoch. This is measured from when the current epoch store is opened,
    /// until the current epoch store is replaced with the next epoch store.
    pub epoch_total_duration: Histogram,

    // TODO: To add this metric, we need to track the first checkpoint of each epoch somewhere.
    // /// Number of checkpoints in the latest completed epoch.
    // pub epoch_checkpoint_count: IntGauge,

    // An active validator reconfigures through the following steps:
    // 1. Halt validator (a.k.a. close epoch) and stop accepting user transaction certs.
    // 2. Finishes processing all pending certificates and then send EndOfPublish message.
    // 3. Stop accepting messages from Narwhal after seeing 2f+1 EndOfPublish messages.
    // 4. Creating the last checkpoint of the epoch by augmenting it with AdvanceEpoch transaction.
    // 5. CheckpointExecutor finishes executing the last checkpoint, and triggers reconfiguration.
    // 6. During reconfiguration, we tear down Narwhal, reconfigure state (at which point we opens
    //    up user certs), and start Narwhal again.
    // 7. After reconfiguration, and eventually Narwhal starts successfully, at some point the first
    //    checkpoint of the new epoch will be created.
    // We introduce various metrics to cover the latency of above steps.
    /// The duration from when the epoch is closed (i.e. validator halted) to when all pending
    /// certificates are processed (i.e. ready to send EndOfPublish message).
    /// This is the duration of (1) through (2) above.
    pub epoch_pending_certs_processed_time_since_epoch_close_ms: Histogram,

    /// The interval from when the epoch is closed to when we receive 2f+1 EndOfPublish messages.
    /// This is the duration of (1) through (3) above.
    pub epoch_end_of_publish_quorum_time_since_epoch_close_ms: Histogram,

    /// The interval from when the epoch is closed to when we created the last checkpoint of the
    /// epoch.
    /// This is the duration of (1) through (4) above.
    pub epoch_last_checkpoint_created_time_since_epoch_close_ms: Histogram,

    /// The total time takes to create the certificate of the last transaction of the epoch. Since
    /// we currently this cert by querying a quorum of validators, it may take some time and we
    /// should track how long this process is.
    /// This should be the primary time contributor of (4) above.
    pub epoch_last_transaction_cert_creation_time_ms: Histogram,

    /// The interval from when the epoch is closed to when we finished executing the last transaction
    /// of the checkpoint (and hence triggering reconfiguration process).
    /// This is the duration of (1) through (5) above.
    pub epoch_reconfig_start_time_since_epoch_close_ms: Histogram,

    /// The total duration when this validator is halted, and hence does not accept certs from users.
    /// This is the duration of (1) through (6) above, and is the most important latency metric
    /// reflecting reconfiguration delay for each validator.
    pub epoch_validator_halt_duration_ms: Histogram,

    /// The interval from when the epoch begins (i.e. right after state reconfigure, when the new
    /// epoch_store is created), to when the first checkpoint of the epoch is ready for creation locally.
    /// This is (7) above, and is a good proxy to how long it takes for the validator
    /// to become useful in the network after reconfiguration.
    pub epoch_first_checkpoint_ready_time_since_epoch_begin_ms: Histogram,
}

impl EpochMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        let this = Self {
            current_epoch: register_int_gauge_with_registry!(
                "current_epoch",
                "Current epoch ID",
                registry
            )
            .unwrap(),
            epoch_total_duration: Histogram::new_in_registry(
                "epoch_total_duration",
                "Total duration of the epoch",
                registry,
            ),
            epoch_pending_certs_processed_time_since_epoch_close_ms: Histogram::new_in_registry(
                "epoch_pending_certs_processed_time_since_epoch_close_ms",
                "Time interval from when epoch was closed to when all pending certificates are processed",
                registry,
            ),
            epoch_end_of_publish_quorum_time_since_epoch_close_ms: Histogram::new_in_registry(
                "epoch_end_of_publish_quorum_time_since_epoch_close_ms",
                "Time interval from when epoch was closed to when 2f+1 EndOfPublish messages are received",
                registry,
            ),
            epoch_last_checkpoint_created_time_since_epoch_close_ms: Histogram::new_in_registry(
                "epoch_last_checkpoint_created_time_since_epoch_close_ms",
                "Time interval from when epoch was closed to when the last checkpoint of the epoch is created",
                registry,
            ),
            epoch_last_transaction_cert_creation_time_ms: Histogram::new_in_registry(
                "epoch_last_transaction_cert_creation_time_ms",
                "Time takes to create the last transaction certificate of the epoch",
                registry,
            ),
            epoch_reconfig_start_time_since_epoch_close_ms: Histogram::new_in_registry(
                "epoch_reconfig_start_time_since_epoch_close_ms",
                "Total time duration from when epoch was closed to when we begin to reconfigure the validator",
                registry,
            ),
            epoch_validator_halt_duration_ms: Histogram::new_in_registry(
                "epoch_validator_halt_duration_ms",
                "Total time duration when the validator was halted (i.e. epoch closed)",
                registry,
            ),
            epoch_first_checkpoint_ready_time_since_epoch_begin_ms: Histogram::new_in_registry(
                "epoch_first_checkpoint_created_time_since_epoch_begin_ms",
                "Time interval from when the epoch opens at new epoch to the first checkpoint is created locally",
                registry,
            ),
        };
        Arc::new(this)
    }
}
