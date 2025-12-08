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

/// Process package-related commands (rebuild, extract, overwrite)
async fn process_package_command(command: &Command) -> Result<()> {
    match command {
        Command::RebuildPackage {
            package_id,
            package_source,
            output_path,
            node,
        } => {
            let object_id =
                ObjectID::from_str(package_id).map_err(|e| anyhow!("Invalid package ID: {}", e))?;

            rebuild_package(
                node.clone(),
                object_id,
                package_source.clone(),
                output_path.clone(),
            )?;

            Ok(())
        }
        Command::ExtractPackage {
            package_id,
            output_path,
            node,
        } => {
            let object_id =
                ObjectID::from_str(package_id).map_err(|e| anyhow!("Invalid package ID: {}", e))?;

            extract_package(node.clone(), object_id, output_path.clone())?;

            Ok(())
        }
        Command::OverwritePackage {
            package_id,
            package_path,
            node,
        } => {
            let object_id =
                ObjectID::from_str(package_id).map_err(|e| anyhow!("Invalid package ID: {}", e))?;

            overwrite_package(node.clone(), object_id, package_path.clone())?;

            Ok(())
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let config = Config::parse();

    // Handle subcommands first
    if let Some(command) = &config.command {
        return process_package_command(command).await;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_id_parsing_valid() {
        // Test that valid object IDs can be parsed
        let valid_id = "0x0000000000000000000000000000000000000000000000000000000000000002";
        let result = ObjectID::from_str(valid_id);
        assert!(result.is_ok(), "Valid object ID should parse successfully");
    }

    #[test]
    fn test_object_id_parsing_invalid() {
        // Test that invalid object IDs fail to parse
        let invalid_id = "not_a_valid_object_id";
        let result = ObjectID::from_str(invalid_id);
        assert!(result.is_err(), "Invalid object ID should fail to parse");
    }

    #[tokio::test]
    async fn test_process_package_command_invalid_package_id() {
        use std::path::PathBuf;
        use sui_replay_2::Node;

        // Test rebuild command with invalid package ID
        let command = Command::RebuildPackage {
            package_id: "invalid_id".to_string(),
            package_source: PathBuf::from("/tmp/source"),
            output_path: None,
            node: Node::Mainnet,
        };

        let result = process_package_command(&command).await;
        assert!(
            result.is_err(),
            "Processing command with invalid package ID should fail"
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid package ID"),
            "Error message should mention invalid package ID"
        );
    }

    #[tokio::test]
    async fn test_process_package_command_extract_invalid_id() {
        use std::path::PathBuf;
        use sui_replay_2::Node;

        // Test extract command with invalid package ID
        let command = Command::ExtractPackage {
            package_id: "bad_id".to_string(),
            output_path: PathBuf::from("/tmp/output"),
            node: Node::Testnet,
        };

        let result = process_package_command(&command).await;
        assert!(
            result.is_err(),
            "Extract command with invalid package ID should fail"
        );
    }

    #[tokio::test]
    async fn test_process_package_command_overwrite_invalid_id() {
        use std::path::PathBuf;
        use sui_replay_2::Node;

        // Test overwrite command with invalid package ID
        let command = Command::OverwritePackage {
            package_id: "xyz".to_string(),
            package_path: PathBuf::from("/tmp/package"),
            node: Node::Mainnet,
        };

        let result = process_package_command(&command).await;
        assert!(
            result.is_err(),
            "Overwrite command with invalid package ID should fail"
        );
    }

    #[tokio::test]
    async fn test_handle_replay_config_missing_digest_and_path() {
        use sui_replay_2::{
            Node, ReplayConfigExperimental, ReplayConfigStableInternal, StoreMode,
            handle_replay_config,
        };

        // Create config with neither digest nor digests_path
        let stable_config = ReplayConfigStableInternal {
            digest: None,
            digests_path: None,
            terminate_early: false,
            trace: false,
            output_dir: None,
            show_effects: false,
            overwrite: false,
        };

        let experimental_config = ReplayConfigExperimental {
            node: Node::Mainnet,
            verbose: false,
            store_mode: StoreMode::GqlOnly,
            track_time: false,
            cache_executor: false,
        };

        let result =
            handle_replay_config(&stable_config, &experimental_config, "test_version").await;

        assert!(
            result.is_err(),
            "Config without digest or digests_path should fail"
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("either --digest or --digests-path must be provided"),
            "Error should mention missing digest or digests_path"
        );
    }

    #[tokio::test]
    async fn test_handle_replay_config_with_digest() {
        use sui_replay_2::{
            Node, ReplayConfigExperimental, ReplayConfigStableInternal, StoreMode,
            handle_replay_config,
        };
        use tempfile::TempDir;

        // Create a temporary output directory
        let temp_dir = TempDir::new().unwrap();

        // Create config with a digest
        let stable_config = ReplayConfigStableInternal {
            digest: Some("825koQKtB5ULrX6egKTZm525ermqsNsoJBAxjh45EsE8".to_string()),
            digests_path: None,
            terminate_early: false,
            trace: false,
            output_dir: Some(temp_dir.path().to_path_buf()),
            show_effects: false,
            overwrite: false,
        };

        let experimental_config = ReplayConfigExperimental {
            node: Node::Mainnet,
            verbose: false,
            store_mode: StoreMode::FsOnly,
            track_time: false,
            cache_executor: false,
        };

        // This will fail because there's no actual transaction to replay,
        // but it should get past the validation stage
        let result =
            handle_replay_config(&stable_config, &experimental_config, "test_version").await;

        // The function should fail during replay, not during config validation
        // If it fails with "must be provided" it means validation failed
        assert!(result.is_ok(), "Replay failed.");
    }

    #[tokio::test]
    async fn test_handle_replay_config_with_digests_path() {
        use std::io::Write;
        use sui_replay_2::{
            Node, ReplayConfigExperimental, ReplayConfigStableInternal, StoreMode,
            handle_replay_config,
        };
        use tempfile::NamedTempFile;

        let mut digests_file = NamedTempFile::new().unwrap();
        writeln!(digests_file, "825koQKtB5ULrX6egKTZm525ermqsNsoJBAxjh45EsE8").unwrap();
        writeln!(
            digests_file,
            "DqqRuchzvgXpVdPwi5wvmUR1oxXiEYc74w1sLipvS48n"
        )
        .unwrap();
        digests_file.flush().unwrap();

        let stable_config = ReplayConfigStableInternal {
            digest: None,
            digests_path: Some(digests_file.path().to_path_buf()),
            terminate_early: false,
            trace: false,
            output_dir: None,
            show_effects: false,
            overwrite: false,
        };

        let experimental_config = ReplayConfigExperimental {
            node: Node::Mainnet,
            verbose: false,
            store_mode: StoreMode::FsOnly,
            track_time: false,
            cache_executor: false,
        };

        let result =
            handle_replay_config(&stable_config, &experimental_config, "test_version").await;
        assert!(result.is_ok(), "Replay failed.");
    }

    #[cfg(feature = "tracing")]
    #[tokio::test]
    async fn test_handle_replay_config_tracing_enabled() {
        use sui_replay_2::{
            Node, ReplayConfigExperimental, ReplayConfigStableInternal, StoreMode,
            handle_replay_config,
        };
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();

        // Create config with tracing enabled
        let stable_config = ReplayConfigStableInternal {
            digest: Some("825koQKtB5ULrX6egKTZm525ermqsNsoJBAxjh45EsE8".to_string()),
            digests_path: None,
            terminate_early: false,
            trace: true,
            output_dir: Some(temp_dir.path().to_path_buf()),
            show_effects: false,
            overwrite: false,
        };

        let experimental_config = ReplayConfigExperimental {
            node: Node::Mainnet,
            verbose: false,
            store_mode: StoreMode::FsOnly,
            track_time: false,
            cache_executor: false,
        };

        // With tracing feature enabled, this should not fail due to tracing
        let result =
            handle_replay_config(&stable_config, &experimental_config, "test_version").await;

        assert!(result.is_ok(), "Replay failed.");
    }
}
