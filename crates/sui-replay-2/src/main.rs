// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Result, anyhow};
use clap::*;
use core::panic;
use std::str::FromStr;
use sui_replay_2::{
    Command, Config, handle_replay_config, load_config_file, merge_configs,
    package_tools::{extract_package, overwrite_package, rebuild_package},
    print_effects_or_fork,
};
use sui_types::base_types::ObjectID;

// Define the `GIT_REVISION` and `VERSION` consts
bin_version::bin_version!();

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let config = Config::parse();

    // Handle subcommands first
    if let Some(command) = &config.command {
        match command {
            Command::RebuildPackage {
                package_id,
                package_source,
                output_path,
                node,
                env,
            } => {
                let object_id = ObjectID::from_str(package_id)
                    .map_err(|e| anyhow!("Invalid package ID: {}", e))?;

                rebuild_package(
                    node.clone(),
                    object_id,
                    package_source.clone(),
                    output_path.clone(),
                    env.clone(),
                )?;

                return Ok(());
            }
            Command::ExtractPackage {
                package_id,
                output_path,
                node,
            } => {
                let object_id = ObjectID::from_str(package_id)
                    .map_err(|e| anyhow!("Invalid package ID: {}", e))?;

                extract_package(node.clone(), object_id, output_path.clone())?;

                return Ok(());
            }
            Command::OverwritePackage {
                package_id,
                package_path,
                node,
            } => {
                let object_id = ObjectID::from_str(package_id)
                    .map_err(|e| anyhow!("Invalid package ID: {}", e))?;

                overwrite_package(node.clone(), object_id, package_path.clone())?;

                return Ok(());
            }
        }
    }

    // Handle regular replay mode
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
