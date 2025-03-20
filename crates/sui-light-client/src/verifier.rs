// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoint::{read_checkpoint, read_checkpoint_list, CheckpointsList};
use crate::config::Config;
use crate::object_store::SuiObjectStore;
use anyhow::{anyhow, Result};
use std::sync::Arc;
use sui_config::genesis::Genesis;
use sui_json_rpc_types::{SuiObjectDataOptions, SuiTransactionBlockResponseOptions};
use sui_sdk::SuiClientBuilder;
use sui_types::base_types::{ObjectID, TransactionDigest};
use sui_types::committee::Committee;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;
use tracing::info;

use sui_types::effects::TransactionEffectsAPI;

pub fn extract_verified_effects_and_events(
    checkpoint: &CheckpointData,
    committee: &Committee,
    tid: TransactionDigest,
) -> Result<(TransactionEffects, Option<TransactionEvents>)> {
    let summary = &checkpoint.checkpoint_summary;

    // Verify the checkpoint summary using the committee
    summary.verify_with_contents(committee, Some(&checkpoint.checkpoint_contents))?;

    // Check the validity of the transaction
    let contents = &checkpoint.checkpoint_contents;
    let (matching_tx, _) = checkpoint
        .transactions
        .iter()
        .zip(contents.iter())
        // Note that we get the digest of the effects to ensure this is
        // indeed the correct effects that are authenticated in the contents.
        .find(|(tx, digest)| {
            tx.effects.execution_digests() == **digest && digest.transaction == tid
        })
        .ok_or(anyhow!("Transaction not found in checkpoint contents"))?;

    // Check the events are all correct.
    let events_digest = matching_tx.events.as_ref().map(|events| events.digest());
    anyhow::ensure!(
        events_digest.as_ref() == matching_tx.effects.events_digest(),
        "Events digest does not match"
    );

    // Since we do not check objects we do not return them
    Ok((matching_tx.effects.clone(), matching_tx.events.clone()))
}

pub async fn get_verified_object(config: &Config, id: ObjectID) -> Result<Object> {
    let sui_client: Arc<sui_sdk::SuiClient> = Arc::new(
        SuiClientBuilder::default()
            .build(config.full_node_url.as_str())
            .await?,
    );

    info!("Getting object: {}", id);

    let read_api = sui_client.read_api();
    let object_json = read_api
        .get_object_with_options(id, SuiObjectDataOptions::bcs_lossless())
        .await
        .expect("Cannot get object");
    let object = object_json
        .into_object()
        .expect("Cannot make into object data");
    let object: Object = object.try_into().expect("Cannot reconstruct object");

    // Need to authenticate this object
    let (effects, _) = get_verified_effects_and_events(config, object.previous_transaction)
        .await
        .expect("Cannot get effects and events");

    // check that this object ID, version and hash is in the effects
    let target_object_ref = object.compute_object_reference();
    effects
        .all_changed_objects()
        .iter()
        .find(|object_ref| object_ref.0 == target_object_ref)
        .ok_or(anyhow!("Object not found"))
        .expect("Object not found");

    Ok(object)
}

pub async fn get_verified_effects_and_events(
    config: &Config,
    tid: TransactionDigest,
) -> Result<(TransactionEffects, Option<TransactionEvents>)> {
    let sui_mainnet: sui_sdk::SuiClient = SuiClientBuilder::default()
        .build(config.full_node_url.as_str())
        .await?;
    let read_api = sui_mainnet.read_api();

    info!("Getting effects and events for TID: {}", tid);

    // Lookup the transaction id and get the checkpoint sequence number
    let options = SuiTransactionBlockResponseOptions::new();
    let seq = read_api
        .get_transaction_with_options(tid, options)
        .await
        .map_err(|e| anyhow!(format!("Cannot get transaction: {e}")))?
        .checkpoint
        .ok_or(anyhow!("Transaction not found"))?;

    // Create object store
    let object_store = SuiObjectStore::new(config)?;

    // Download the full checkpoint for this sequence number
    let full_check_point = object_store
        .get_full_checkpoint(seq)
        .await
        .map_err(|e| anyhow!(format!("Cannot get full checkpoint: {e}")))?;

    // Load the list of stored checkpoints
    let checkpoints_list: CheckpointsList = read_checkpoint_list(config)?;

    // find the stored checkpoint before the seq checkpoint
    let prev_ckp_id = checkpoints_list
        .checkpoints
        .iter()
        .filter(|ckp_id| **ckp_id < seq)
        .last();

    let committee = if let Some(prev_ckp_id) = prev_ckp_id {
        // Read it from the store
        let prev_ckp = read_checkpoint(config, *prev_ckp_id)?;

        // Check we have the right checkpoint
        anyhow::ensure!(
            prev_ckp.epoch().checked_add(1).unwrap() == full_check_point.checkpoint_summary.epoch(),
            "Checkpoint sequence number does not match. Need to Sync."
        );

        // Get the committee from the previous checkpoint
        let current_committee = prev_ckp
            .end_of_epoch_data
            .as_ref()
            .ok_or(anyhow!(
                "Expected all checkpoints to be end-of-epoch checkpoints"
            ))?
            .next_epoch_committee
            .iter()
            .cloned()
            .collect();

        // Make a committee object using this
        Committee::new(prev_ckp.epoch().checked_add(1).unwrap(), current_committee)
    } else {
        // Since we did not find a small committee checkpoint we use the genesis
        let mut genesis_path = config.checkpoint_summary_dir.clone();
        genesis_path.push(&config.genesis_filename);
        Genesis::load(&genesis_path)?
            .committee()
            .map_err(|e| anyhow!(format!("Cannot load Genesis: {e}")))?
    };

    info!("Extracting effects and events for TID: {}", tid);
    extract_verified_effects_and_events(&full_check_point, &committee, tid)
        .map_err(|e| anyhow!(format!("Cannot extract effects and events: {e}")))
}

/// Get the verified checkpoint sequence number for an object.
/// This function will verify that the object is in the transaction's effects,
/// and that the transaction is in the checkpoint
/// and that the checkpoint is signed by the committee
/// and the committee is read from the verified checkpoint summary
/// which is signed by the previous committee.
pub async fn get_verified_checkpoint(
    id: ObjectID,
    config: &Config,
) -> Result<CheckpointSequenceNumber> {
    let sui_client: sui_sdk::SuiClient = SuiClientBuilder::default()
        .build(config.full_node_url.as_str())
        .await?;
    let read_api = sui_client.read_api();
    let object_json = read_api
        .get_object_with_options(id, SuiObjectDataOptions::bcs_lossless())
        .await
        .expect("Cannot get object");
    let object = object_json
        .into_object()
        .expect("Cannot make into object data");
    let object: Object = object.try_into().expect("Cannot reconstruct object");

    // Lookup the transaction id and get the checkpoint sequence number
    let options = SuiTransactionBlockResponseOptions::new();
    let seq = read_api
        .get_transaction_with_options(object.previous_transaction, options)
        .await
        .map_err(|e| anyhow!(format!("Cannot get transaction: {e}")))?
        .checkpoint
        .ok_or(anyhow!("Transaction not found"))?;

    // Need to authenticate this object
    let (effects, _) = get_verified_effects_and_events(config, object.previous_transaction)
        .await
        .expect("Cannot get effects and events");

    // check that this object ID, version and hash is in the effects
    let target_object_ref = object.compute_object_reference();
    effects
        .all_changed_objects()
        .iter()
        .find(|object_ref| object_ref.0 == target_object_ref)
        .ok_or(anyhow!("Object not found"))
        .expect("Object not found");

    // Create object store
    let object_store = SuiObjectStore::new(config)?;

    // Download the full checkpoint for this sequence number
    let full_check_point = object_store
        .get_full_checkpoint(seq)
        .await
        .map_err(|e| anyhow!(format!("Cannot get full checkpoint: {e}")))?;

    // Load the list of stored checkpoints
    let checkpoints_list: CheckpointsList = read_checkpoint_list(config)?;

    // find the stored checkpoint before the seq checkpoint
    let prev_ckp_id = checkpoints_list
        .checkpoints
        .iter()
        .filter(|ckp_id| **ckp_id < seq)
        .last();

    let committee = if let Some(prev_ckp_id) = prev_ckp_id {
        // Read it from the store
        let prev_ckp = read_checkpoint(config, *prev_ckp_id)?;

        // Check we have the right checkpoint
        anyhow::ensure!(
            prev_ckp.epoch().checked_add(1).unwrap() == full_check_point.checkpoint_summary.epoch(),
            "Checkpoint sequence number does not match. Need to Sync."
        );

        // Get the committee from the previous checkpoint
        let current_committee = prev_ckp
            .end_of_epoch_data
            .as_ref()
            .ok_or(anyhow!(
                "Expected all checkpoints to be end-of-epoch checkpoints"
            ))?
            .next_epoch_committee
            .iter()
            .cloned()
            .collect();

        // Make a committee object using this
        Committee::new(prev_ckp.epoch().checked_add(1).unwrap(), current_committee)
    } else {
        // Since we did not find a small committee checkpoint we use the genesis
        let mut genesis_path = config.checkpoint_summary_dir.clone();
        genesis_path.push(&config.genesis_filename);
        Genesis::load(&genesis_path)?
            .committee()
            .map_err(|e| anyhow!(format!("Cannot load Genesis: {e}")))?
    };

    // Verify that committee signed this checkpoint and checkpoint contents with digest
    full_check_point
        .checkpoint_summary
        .verify_with_contents(&committee, Some(&full_check_point.checkpoint_contents))?;

    if full_check_point
        .transactions
        .iter()
        .any(|t| *t.transaction.digest() == object.previous_transaction)
    {
        Ok(seq)
    } else {
        Err(anyhow!("Transaction not found in checkpoint"))
    }
}

// Make a test namespace
#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{Read, Write};
    use sui_types::messages_checkpoint::{CheckpointSummary, FullCheckpointContents};

    use super::*;
    use std::path::{Path, PathBuf};
    use std::str::FromStr;
    use sui_types::crypto::AuthorityQuorumSignInfo;
    use sui_types::message_envelope::Envelope;

    async fn read_full_checkpoint(checkpoint_path: &PathBuf) -> anyhow::Result<CheckpointData> {
        let mut reader = fs::File::open(checkpoint_path.clone())?;
        let metadata = fs::metadata(checkpoint_path)?;
        let mut buffer = vec![0; metadata.len() as usize];
        reader.read_exact(&mut buffer)?;
        bcs::from_bytes(&buffer).map_err(|_| anyhow!("Unable to parse checkpoint file"))
    }

    // clippy ignore dead-code
    #[allow(dead_code)]
    async fn write_full_checkpoint(
        checkpoint_path: &Path,
        checkpoint: &CheckpointData,
    ) -> anyhow::Result<()> {
        let mut writer = fs::File::create(checkpoint_path)?;
        let bytes = bcs::to_bytes(&checkpoint)
            .map_err(|_| anyhow!("Unable to serialize checkpoint summary"))?;
        writer.write_all(&bytes)?;
        Ok(())
    }

    async fn read_data() -> (Committee, CheckpointData) {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("test_files/20873329.yaml");

        let mut reader = fs::File::open(d.clone()).unwrap();
        let metadata = fs::metadata(&d).unwrap();
        let mut buffer = vec![0; metadata.len() as usize];
        reader.read_exact(&mut buffer).unwrap();
        let checkpoint: Envelope<CheckpointSummary, AuthorityQuorumSignInfo<true>> =
            bcs::from_bytes(&buffer)
                .map_err(|_| anyhow!("Unable to parse checkpoint file"))
                .unwrap();

        let prev_committee = checkpoint
            .end_of_epoch_data
            .as_ref()
            .ok_or(anyhow!(
                "Expected all checkpoints to be end-of-epoch checkpoints"
            ))
            .unwrap()
            .next_epoch_committee
            .iter()
            .cloned()
            .collect();

        // Make a committee object using this
        let committee = Committee::new(checkpoint.epoch().checked_add(1).unwrap(), prev_committee);

        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("test_files/20958462.bcs");

        let full_checkpoint = read_full_checkpoint(&d).await.unwrap();

        (committee, full_checkpoint)
    }

    #[tokio::test]
    async fn test_checkpoint_all_good() {
        let (committee, full_checkpoint) = read_data().await;

        extract_verified_effects_and_events(
            &full_checkpoint,
            &committee,
            TransactionDigest::from_str("8RiKBwuAbtu8zNCtz8SrcfHyEUzto6zi6cMVA9t4WhWk").unwrap(),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn test_checkpoint_bad_committee() {
        let (mut committee, full_checkpoint) = read_data().await;

        // Change committee
        committee.epoch += 10;

        assert!(extract_verified_effects_and_events(
            &full_checkpoint,
            &committee,
            TransactionDigest::from_str("8RiKBwuAbtu8zNCtz8SrcfHyEUzto6zi6cMVA9t4WhWk").unwrap(),
        )
        .is_err());
    }

    #[tokio::test]
    async fn test_checkpoint_no_transaction() {
        let (committee, full_checkpoint) = read_data().await;

        assert!(extract_verified_effects_and_events(
            &full_checkpoint,
            &committee,
            TransactionDigest::from_str("8RiKBwuAbtu8zNCtz8SrcfHyEUzto6zj6cMVA9t4WhWk").unwrap(),
        )
        .is_err());
    }

    #[tokio::test]
    async fn test_checkpoint_bad_contents() {
        let (committee, mut full_checkpoint) = read_data().await;

        // Change contents
        let random_contents = FullCheckpointContents::random_for_testing();
        full_checkpoint.checkpoint_contents = random_contents.checkpoint_contents();

        assert!(extract_verified_effects_and_events(
            &full_checkpoint,
            &committee,
            TransactionDigest::from_str("8RiKBwuAbtu8zNCtz8SrcfHyEUzto6zj6cMVA9t4WhWk").unwrap(),
        )
        .is_err());
    }

    #[tokio::test]
    async fn test_checkpoint_bad_events() {
        let (committee, mut full_checkpoint) = read_data().await;

        let event = full_checkpoint.transactions[4]
            .events
            .as_ref()
            .unwrap()
            .data[0]
            .clone();

        for t in &mut full_checkpoint.transactions {
            if let Some(events) = &mut t.events {
                events.data.push(event.clone());
            }
        }

        assert!(extract_verified_effects_and_events(
            &full_checkpoint,
            &committee,
            TransactionDigest::from_str("8RiKBwuAbtu8zNCtz8SrcfHyEUzto6zj6cMVA9t4WhWk").unwrap(),
        )
        .is_err());
    }
}
