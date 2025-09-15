// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use core::panic;
use sui_replay_2::{
    handle_replay_config, load_config_file, merge_configs_with_presence, print_effects_or_fork,
    Config,
};

// Define the `GIT_REVISION` and `VERSION` consts
bin_version::bin_version!();

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    // Parse with flag presence detection needed to decide if flags
    // from the command line or from the config file are used
    let command = Config::command();
    let matches = command.get_matches();
    let config = Config::from_arg_matches(&matches)?;

    let file_config = load_config_file()?;
    let stable_config =
        merge_configs_with_presence(&config.replay_stable, file_config.as_ref(), &Some(matches));

    let output_root =
        handle_replay_config(&stable_config, &config.replay_experimental, VERSION).await?;

    if let Some(digest) = &config.replay_stable.digest {
        print_effects_or_fork(
            digest,
            &output_root,
            stable_config.show_effects,
            &mut std::io::stdout(),
        )?;
    }
    Ok(())
}
