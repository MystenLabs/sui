// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::Config;
use crate::graphql::query_last_checkpoint_of_epoch;
use crate::object_store::SuiObjectStore;
use anyhow::{anyhow, Result};
use object_store::parse_url;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::{fs, io::Write};
use sui_archival::read_manifest;
use sui_config::genesis::Genesis;
use sui_sdk::SuiClientBuilder;
use sui_types::committee::Committee;
use sui_types::messages_checkpoint::EndOfEpochData;
use sui_types::{
    crypto::AuthorityQuorumSignInfo, message_envelope::Envelope,
    messages_checkpoint::CheckpointSummary,
};
use tracing::info;
use url::Url;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CheckpointsList {
    pub checkpoints: Vec<u64>,
}

pub fn read_checkpoint_list(config: &Config) -> Result<CheckpointsList> {
    let checkpoints_path = config.checkpoint_list_path();
    let reader = fs::File::open(checkpoints_path)?;
    Ok(serde_yaml::from_reader(reader)?)
}

pub fn write_checkpoint_list(config: &Config, checkpoints_list: &CheckpointsList) -> Result<()> {
    let checkpoints_path = config.checkpoint_list_path();
    let mut writer = fs::File::create(checkpoints_path)?;
    let bytes = serde_yaml::to_vec(checkpoints_list)?;
    writer
        .write_all(&bytes)
        .map_err(|e| anyhow!("Unable to serialize checkpoint list: {}", e))
}

pub fn read_checkpoint(
    config: &Config,
    seq: u64,
) -> Result<Envelope<CheckpointSummary, AuthorityQuorumSignInfo<true>>> {
    read_checkpoint_general(config, seq, None)
}

fn read_checkpoint_general(
    config: &Config,
    seq: u64,
    path: Option<&str>,
) -> Result<Envelope<CheckpointSummary, AuthorityQuorumSignInfo<true>>> {
    let checkpoint_path = config.checkpoint_path(seq, path);
    let mut reader = fs::File::open(checkpoint_path)?;
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;
    bcs::from_bytes(&buffer).map_err(|_| anyhow!("Unable to parse checkpoint file"))
}

pub fn write_checkpoint(
    config: &Config,
    summary: &Envelope<CheckpointSummary, AuthorityQuorumSignInfo<true>>,
) -> Result<()> {
    write_checkpoint_general(config, summary, None)
}

fn write_checkpoint_general(
    config: &Config,
    summary: &Envelope<CheckpointSummary, AuthorityQuorumSignInfo<true>>,
    path: Option<&str>,
) -> Result<()> {
    let checkpoint_path = config.checkpoint_path(*summary.sequence_number(), path);
    let mut writer = fs::File::create(checkpoint_path)?;
    let bytes =
        bcs::to_bytes(summary).map_err(|_| anyhow!("Unable to serialize checkpoint summary"))?;
    writer.write_all(&bytes)?;
    Ok(())
}

/// Downloads the list of end of epoch checkpoints from the archive store or the GraphQL endpoint
async fn sync_checkpoint_list_to_latest(config: &Config) -> anyhow::Result<()> {
    if config.graphql_url.is_some() {
        sync_checkpoint_list_to_latest_using_graphql(config).await
    } else {
        sync_checkpoint_list_to_latest_using_archive(config).await
    }
}
/// Downloads the list of end of epoch checkpoints from the archive store
async fn sync_checkpoint_list_to_latest_using_archive(config: &Config) -> anyhow::Result<()> {
    let url = Url::parse(&config.archive_store_url.as_ref().cloned().unwrap())?;
    let (dyn_store, _store_path) = parse_url(&url)?;
    let manifest = read_manifest(dyn_store).await?;
    let checkpoints = manifest.get_all_end_of_epoch_checkpoint_seq_numbers()?;
    write_checkpoint_list(config, &CheckpointsList { checkpoints })?;
    Ok(())
}

/// Run binary search to for each end of epoch checkpoint that is missing
/// between the latest on the list and the latest checkpoint.
async fn sync_checkpoint_list_to_latest_using_graphql(config: &Config) -> anyhow::Result<()> {
    // Get the local checkpoint list
    let mut checkpoints_list: CheckpointsList = read_checkpoint_list(config)?;
    let latest_in_list = checkpoints_list
        .checkpoints
        .last()
        .ok_or(anyhow!("Empty checkpoint list"))?;
    // Create object store
    let object_store = SuiObjectStore::new(config)?;

    // Download the latest in list checkpoint
    let summary = object_store
        .download_checkpoint_summary(*latest_in_list)
        .await?;
    let mut last_epoch = summary.epoch();

    // Download the very latest checkpoint
    let client = SuiClientBuilder::default()
        .build(config.full_node_url.as_str())
        .await
        .expect("Cannot connect to full node");

    let latest_seq = client
        .read_api()
        .get_latest_checkpoint_sequence_number()
        .await?;
    let latest = object_store.download_checkpoint_summary(latest_seq).await?;

    // Sequentially record all the missing end of epoch checkpoints numbers
    while last_epoch + 1 < latest.epoch() {
        let target_epoch = last_epoch + 1;
        let target_last_checkpoint_number =
            query_last_checkpoint_of_epoch(config, target_epoch).await?;

        // Add to the list
        checkpoints_list
            .checkpoints
            .push(target_last_checkpoint_number);
        write_checkpoint_list(config, &checkpoints_list)?;

        // Update
        last_epoch = target_epoch;

        info!(
            "Last Epoch: {} Last Checkpoint: {}",
            target_epoch, target_last_checkpoint_number
        );
    }

    Ok(())
}

pub async fn check_and_sync_checkpoints(config: &Config) -> anyhow::Result<()> {
    sync_checkpoint_list_to_latest(config)
        .await
        .map_err(|e| anyhow!(format!("Cannot refresh list: {e}")))?;

    // Get the local checkpoint list
    let checkpoints_list: CheckpointsList = read_checkpoint_list(config)
        .map_err(|e| anyhow!(format!("Cannot read checkpoint list: {e}")))?;

    // Load the genesis committee
    let mut genesis_path = config.checkpoint_summary_dir.clone();
    genesis_path.push(&config.genesis_filename);
    let genesis_committee = Genesis::load(&genesis_path)?
        .committee()
        .map_err(|e| anyhow!(format!("Cannot load Genesis: {e}")))?;

    // Check the signatures of all checkpoints
    // And download any missing ones

    let mut prev_committee = genesis_committee;
    let object_store = SuiObjectStore::new(config)?;
    for ckp_id in &checkpoints_list.checkpoints {
        // check if there is a file with this name ckp_id.yaml in the checkpoint_summary_dir
        let mut checkpoint_path = config.checkpoint_summary_dir.clone();
        checkpoint_path.push(format!("{}.yaml", ckp_id));

        // If file exists read the file otherwise download it from the server
        let summary = if checkpoint_path.exists() {
            read_checkpoint(config, *ckp_id)
                .map_err(|e| anyhow!(format!("Cannot read checkpoint: {e}")))?
        } else {
            // Download the checkpoint from the server
            let summary = object_store
                .download_checkpoint_summary(*ckp_id)
                .await
                .map_err(|e| anyhow!(format!("Cannot download summary: {e}")))?;
            summary.clone().try_into_verified(&prev_committee)?;
            // Write the checkpoint summary to a file
            write_checkpoint(config, &summary)?;
            summary
        };

        // Print the id of the checkpoint and the epoch number
        info!(
            "Epoch: {} Checkpoint ID: {}",
            summary.epoch(),
            summary.digest()
        );

        // Extract the new committee information
        if let Some(EndOfEpochData {
            next_epoch_committee,
            ..
        }) = &summary.end_of_epoch_data
        {
            let next_committee = next_epoch_committee.iter().cloned().collect();
            prev_committee =
                Committee::new(summary.epoch().checked_add(1).unwrap(), next_committee);
        } else {
            return Err(anyhow!(
                "Expected all checkpoints to be end-of-epoch checkpoints"
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::default;
    use tempfile::TempDir;

    fn create_test_config() -> (Config, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            checkpoint_summary_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        (config, temp_dir)
    }

    #[test]
    fn test_checkpoint_list_read_write() {
        let (config, _temp_dir) = create_test_config();
        let test_list = CheckpointsList {
            checkpoints: vec![1, 2, 3],
        };

        write_checkpoint_list(&config, &test_list).unwrap();
        let read_list = read_checkpoint_list(&config).unwrap();

        assert_eq!(test_list.checkpoints, read_list.checkpoints);
    }

    #[test]
    fn test_checkpoint_read_write() {
        let (config, _temp_dir) = create_test_config();
        let test_summary = Envelope::new_from_data_and_sig(
            CheckpointSummary::default(),
            AuthorityQuorumSignInfo::default(),
        );

        write_checkpoint(&config, &test_summary).unwrap();
        let read_summary = read_checkpoint(&config, test_summary.sequence_number()).unwrap();

        assert_eq!(
            test_summary.sequence_number(),
            read_summary.sequence_number()
        );
    }
}
