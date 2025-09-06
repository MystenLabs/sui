// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use core::panic;
use sui_replay_2::{handle_replay_config, print_effects_or_fork, Config};
use tracing::debug;

// Define the `GIT_REVISION` and `VERSION` consts
bin_version::bin_version!();

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let config = Config::parse();
    debug!("Parsed config: {:#?}", config);

    let output_root =
        handle_replay_config(&config.replay_stable, &config.replay_experimental, VERSION).await?;

    if let Some(digest) = &config.replay_stable.digest {
        print_effects_or_fork(
            digest,
            &output_root,
            config.replay_stable.show_effects,
            &mut std::io::stdout(),
        )?;
    }
    Ok(())
}
