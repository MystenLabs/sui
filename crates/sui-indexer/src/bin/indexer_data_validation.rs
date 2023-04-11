// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use tracing::{error, info, warn};

use sui_indexer::new_rpc_client;
use sui_json_rpc_types::{
    CheckpointId, ObjectChange, SuiObjectDataOptions, SuiTransactionBlockResponseOptions,
};
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::digests::TransactionDigest;

#[tokio::main]
async fn main() -> Result<()> {
    // NOTE: this is to print out tracing like info, warn & error.
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();
    info!("Running correctness check for indexer...");
    let config = TestConfig::parse();
    let fn_rpc_client = new_rpc_client(&config.fn_rpc_client_url).await?;
    let indexer_rpc_client = new_rpc_client(&config.indexer_rpc_client_url).await?;

    let fn_latest_checkpoint = fn_rpc_client
        .read_api()
        .get_latest_checkpoint_sequence_number()
        .await?;
    let indexer_latest_checkpoint = indexer_rpc_client
        .read_api()
        .get_latest_checkpoint_sequence_number()
        .await?;

    let end_checkpoint = config.end_checkpoint;
    if end_checkpoint <= fn_latest_checkpoint && end_checkpoint <= indexer_latest_checkpoint {
        for checkpoint in config.start_checkpoint..=end_checkpoint {
            info!("Checking checkpoint {}", checkpoint);
            check_checkpoint(&config, &fn_rpc_client, &indexer_rpc_client, checkpoint).await?;
        }
    } else {
        error!("Start checkpoint is not available in both FN and Indexer, start: {}, FN latest: {}, indexer latest: {}", 
        end_checkpoint, fn_latest_checkpoint, indexer_latest_checkpoint);
    }
    Ok(())
}

pub async fn check_checkpoint(
    config: &TestConfig,
    fn_client: &SuiClient,
    indexer_client: &SuiClient,
    checkpoint: u64,
) -> Result<(), anyhow::Error> {
    let fn_checkpoint = fn_client
        .read_api()
        .get_checkpoint(CheckpointId::SequenceNumber(checkpoint))
        .await?;
    let indexer_checkpoint = indexer_client
        .read_api()
        .get_checkpoint(CheckpointId::SequenceNumber(checkpoint))
        .await?;

    if fn_checkpoint != indexer_checkpoint {
        error!("Checkpoint mismatch found in {}", checkpoint);
        warn!(
            "Checkpoint mismatch with checkpoint seq {:?}:\nFN:\n{:?}\nIndexer:\n{:?} ",
            checkpoint, fn_checkpoint, indexer_checkpoint
        );
    }
    if config.check_transactions {
        check_transactions(
            config,
            fn_client,
            indexer_client,
            fn_checkpoint.transactions,
        )
        .await?;
    }
    Ok(())
}

pub async fn check_transactions(
    config: &TestConfig,
    fn_client: &SuiClient,
    indexer_client: &SuiClient,
    tx_vec: Vec<TransactionDigest>,
) -> Result<(), anyhow::Error> {
    for digest in tx_vec {
        let fetch_options = SuiTransactionBlockResponseOptions::full_content();
        let fn_sui_tx_response = fn_client
            .read_api()
            .get_transaction_with_options(digest, fetch_options.clone())
            .await?;
        let indexer_sui_tx_response = indexer_client
            .read_api()
            .get_transaction_with_options(digest, fetch_options)
            .await?;
        if fn_sui_tx_response != indexer_sui_tx_response {
            error!("Checkpoint transactions mismatch found in {}", digest);
            warn!(
                "Transaction response mismatch with digest {:?}:\nFN:\n{:?}\nIndexer:\n{:?} ",
                digest, fn_sui_tx_response, indexer_sui_tx_response
            );
            continue;
        }
        if config.check_events {
            check_events(fn_client, indexer_client, digest).await?;
        }
        if config.check_objects {
            let objects = fn_sui_tx_response.object_changes.unwrap();
            check_objects(fn_client, indexer_client, objects).await?;
        }
    }
    Ok(())
}

pub async fn check_events(
    fn_client: &SuiClient,
    indexer_client: &SuiClient,
    tx_digest: TransactionDigest,
) -> Result<(), anyhow::Error> {
    let mut fn_events = fn_client.event_api().get_events(tx_digest).await?;
    fn_events.sort_by(|a, b| a.id.event_seq.cmp(&b.id.event_seq));
    let mut indexer_events = indexer_client.event_api().get_events(tx_digest).await?;
    indexer_events.sort_by(|a, b| a.id.event_seq.cmp(&b.id.event_seq));

    for (fn_event, indexer_event) in fn_events.iter().zip(indexer_events.iter()) {
        if fn_event != indexer_event {
            error!(
                "Checkpoint events mismatch found in {:?}, with sequence number: {}",
                tx_digest, fn_event.id.event_seq
            );
            warn!(
                "Event mismatch with digest {:?} and seq nunber:\nFN:\n{:?}\nIndexer:\n{:?} ",
                tx_digest, fn_event, indexer_event
            );
        }
    }
    Ok(())
}

pub async fn check_objects(
    fn_client: &SuiClient,
    indexer_client: &SuiClient,
    objects: Vec<ObjectChange>,
) -> Result<(), anyhow::Error> {
    let options = SuiObjectDataOptions::full_content();
    let object_id_and_version_vec = objects
        .iter()
        .filter_map(get_object_id_and_version)
        .collect::<Vec<(ObjectID, SequenceNumber)>>();
    for (object_id, version) in object_id_and_version_vec {
        let fn_object = fn_client
            .read_api()
            .try_get_parsed_past_object(object_id, version, options.clone())
            .await?;
        let indexer_object = indexer_client
            .read_api()
            .try_get_parsed_past_object(object_id, version, options.clone())
            .await?;
        if fn_object != indexer_object {
            error!(
                "Checkpoint objects mismatch found with object id: {:?} and version: {:?}",
                object_id, version
            );
            warn!(
                "Object mismatch with digest {:?} and object id:\nFN:\n{:?}\nIndexer:\n{:?} ",
                version, fn_object, indexer_object
            );
        }
    }
    Ok(())
}

fn get_object_id_and_version(object_change: &ObjectChange) -> Option<(ObjectID, SequenceNumber)> {
    match object_change {
        ObjectChange::Transferred {
            object_id, version, ..
        } => Some((*object_id, *version)),
        ObjectChange::Mutated {
            object_id, version, ..
        } => Some((*object_id, *version)),
        ObjectChange::Created {
            object_id, version, ..
        } => Some((*object_id, *version)),
        // TODO(gegaowp): needs separate checks for packages and modules publishing
        // TODO(gegaowp): ?? needs separate checks for deleted and wrapped objects
        _ => None,
    }
}

#[derive(Parser)]
#[clap(name = "Transactions Test")]
pub struct TestConfig {
    #[clap(long)]
    pub fn_rpc_client_url: String,
    #[clap(long)]
    pub indexer_rpc_client_url: String,
    #[clap(long)]
    pub start_checkpoint: u64,
    #[clap(long)]
    pub end_checkpoint: u64,
    #[clap(long)]
    pub check_transactions: bool,
    #[clap(long)]
    pub check_events: bool,
    #[clap(long)]
    pub check_objects: bool,
}
