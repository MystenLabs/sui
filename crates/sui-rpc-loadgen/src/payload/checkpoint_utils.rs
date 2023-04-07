// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::join_all;
use std::fmt;
use std::fmt::Display;
use sui_sdk::SuiClient;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

pub(crate) struct CheckpointStats {
    pub latest_checkpoints: Vec<CheckpointSequenceNumber>,
}

impl CheckpointStats {
    pub fn max_latest_checkpoint(&self) -> CheckpointSequenceNumber {
        *self
            .latest_checkpoints
            .iter()
            .max()
            .expect("get_latest_checkpoint_sequence_number should not return empty")
    }

    pub fn min_latest_checkpoint(&self) -> CheckpointSequenceNumber {
        *self
            .latest_checkpoints
            .iter()
            .min()
            .expect("get_latest_checkpoint_sequence_number should not return empty")
    }

    pub fn max_lag(&self) -> u64 {
        self.max_latest_checkpoint() - self.min_latest_checkpoint()
    }
}

impl Display for CheckpointStats {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Max Checkpoint {}, Min Checkpoint {}, Max Lag {}. All latest checkpoints {:?}",
            self.max_latest_checkpoint(),
            self.min_latest_checkpoint(),
            self.max_lag(),
            self.latest_checkpoints
        )
    }
}

pub(crate) async fn get_latest_checkpoint_stats(
    clients: &[SuiClient],
    end_checkpoint: Option<CheckpointSequenceNumber>,
) -> CheckpointStats {
    let latest_checkpoints: Vec<CheckpointSequenceNumber> =
        join_all(clients.iter().map(|client| async {
            match end_checkpoint {
                Some(e) => e,
                None => client
                    .read_api()
                    .get_latest_checkpoint_sequence_number()
                    .await
                    .expect("get_latest_checkpoint_sequence_number should not fail"),
            }
        }))
        .await;

    CheckpointStats { latest_checkpoints }
}
