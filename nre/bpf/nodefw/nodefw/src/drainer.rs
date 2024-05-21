// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub async fn watch(ctx: CancellationToken, path: &str) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                if let Ok(_) = fs::metadata(path) {
                    // drain file exists, unload program and close things down
                    info!("drain file found, terminating firewall");
                    return;
                }
            }
            _ = ctx.cancelled() => {
                return;
            }
        }
    }
}
