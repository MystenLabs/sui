// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::committee::Committee;
use sui_types::messages_checkpoint::CertifiedCheckpointSummary;

pub fn extract_new_committee_info(
    summary: &CertifiedCheckpointSummary,
) -> anyhow::Result<Committee> {
    if let Some(next_epoch_committee) = summary.next_epoch_committee() {
        let next_committee = next_epoch_committee.iter().cloned().collect();
        let next_epoch = summary
            .epoch()
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Epoch addition overflow"))?;
        Ok(Committee::new(next_epoch, next_committee))
    } else {
        Err(anyhow::anyhow!("Expected end of epoch checkpoint"))
    }
}
