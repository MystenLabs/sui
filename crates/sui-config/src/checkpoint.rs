// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CheckpointProcessControl {
    /// The time to allow upon quorum failure for sufficient
    /// authorities to come online, to proceed with the checkpointing
    /// main loop.
    pub delay_on_quorum_failure: Duration,

    /// The delay before we retry the process, when there is a local error
    /// that prevented us from making progress, e.g. failed to create
    /// a new proposal, or not ready to set a new checkpoint due to unexecuted transactions.
    pub delay_on_local_failure: Duration,

    /// The time between full iterations of the checkpointing
    /// logic loop.
    pub long_pause_between_checkpoints: Duration,

    /// The time we allow until a quorum of responses
    /// is received.
    pub timeout_until_quorum: Duration,

    /// The time we allow after a quorum is received for
    /// additional responses to arrive.
    pub extra_time_after_quorum: Duration,

    /// The estimate of the consensus delay.
    pub consensus_delay_estimate: Duration,

    /// The amount of time we wait on any specific authority
    /// per request (it could be byzantine)
    pub per_other_authority_delay: Duration,

    /// The amount if time we wait before retrying anything
    /// during an epoch change. We want this duration to be very small
    /// to minimize the amount of time to finish epoch change.
    pub epoch_change_retry_delay: Duration,
}

impl Default for CheckpointProcessControl {
    /// Standard parameters (currently set heuristically).
    fn default() -> Self {
        CheckpointProcessControl {
            delay_on_quorum_failure: Duration::from_secs(10),
            delay_on_local_failure: Duration::from_secs(3),
            long_pause_between_checkpoints: Duration::from_secs(120),
            timeout_until_quorum: Duration::from_secs(60),
            extra_time_after_quorum: Duration::from_millis(200),
            // TODO: Optimize this.
            // https://github.com/MystenLabs/sui/issues/3619.
            consensus_delay_estimate: Duration::from_secs(3),
            per_other_authority_delay: Duration::from_secs(30),
            epoch_change_retry_delay: Duration::from_millis(100),
        }
    }
}

impl CheckpointProcessControl {
    pub fn default_for_test() -> Self {
        CheckpointProcessControl {
            long_pause_between_checkpoints: Duration::from_secs(3),
            ..Default::default()
        }
    }
}
