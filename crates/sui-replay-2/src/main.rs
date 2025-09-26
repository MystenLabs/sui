// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use core::panic;
use sui_replay_2::{
    handle_replay_config, load_config_file, merge_configs, print_effects_or_fork, Config,
};

// Define the `GIT_REVISION` and `VERSION` consts
bin_version::bin_version!();

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let config = Config::parse();

    let file_config = load_config_file()?;
    let stable_config = merge_configs(config.replay_stable, file_config);

    let output_root =
        handle_replay_config(&stable_config, &config.replay_experimental, VERSION).await?;

    if let Some(digest) = &stable_config.digest {
        print_effects_or_fork(
            digest,
            &output_root,
            stable_config.show_effects,
            &mut std::io::stdout(),
        )?;
    }
    Ok(())
}
