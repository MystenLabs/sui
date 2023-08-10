// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::client::{DynamoDbClient, KVTable, KVWriteClient};
use anyhow::{anyhow, Result};
use mysten_metrics::spawn_monitored_task;
use prometheus::{register_int_gauge_with_registry, IntGauge, Registry};
use std::collections::HashSet;
use std::iter::repeat;
use std::time::Duration;
use sui_config::node::TransactionKeyValueStoreWriteConfig;
use sui_core::storage::RocksDbStore;
use sui_storage::http_key_value_store::TaggedKey;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::storage::ReadStore;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::info;

pub struct KVStoreMetrics {
    pub latest_checkpoint_uploaded_to_kv_store: IntGauge,
}

impl KVStoreMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            latest_checkpoint_uploaded_to_kv_store: register_int_gauge_with_registry!(
                "latest_checkpoint_uploaded_to_kv_store",
                "Latest checkpoint to have been uploaded to the remote key value store",
                registry
            )
            .unwrap(),
        }
    }
}

pub async fn setup_key_value_store_uploader(
    store: RocksDbStore,
    config: &Option<TransactionKeyValueStoreWriteConfig>,
    registry: &Registry,
) -> Result<Option<oneshot::Sender<()>>> {
    if config.is_none() {
        return Ok(None);
    }
    let config = config.as_ref().unwrap().clone();
    let (sender, receiver) = oneshot::channel();
    let metrics = KVStoreMetrics::new(registry);

    spawn_monitored_task!(async move {
        upload_to_kv_store(store, receiver, config, metrics)
            .await
            .expect("Upload failed to key value store")
    });
    Ok(Some(sender))
}

async fn upload_to_kv_store(
    store: RocksDbStore,
    mut receiver: oneshot::Receiver<()>,
    config: TransactionKeyValueStoreWriteConfig,
    metrics: KVStoreMetrics,
) -> Result<()> {
    let mut updates: HashSet<u64> = HashSet::new();
    let mut client = DynamoDbClient::new(&config).await;
    let mut checkpoint_number = client
        .get_state()
        .await
        .expect("failed to fetch key value uploader state")
        .unwrap_or_default();
    info!(
        "Key value store backfill. Current checkpoint is {}",
        checkpoint_number
    );

    let (progress_sender, mut progress_receiver) = mpsc::channel(1000);
    let mut child_handles = vec![];

    for shard_id in 0..config.concurrency {
        let cloned_store = store.clone();
        let cloned_config = config.clone();
        let cloned_progress_sender = progress_sender.clone();
        let (term_sender, term_receiver) = oneshot::channel();
        child_handles.push(term_sender);
        spawn_monitored_task!(async move {
            uploader(
                shard_id as u64,
                checkpoint_number,
                cloned_store,
                cloned_config,
                cloned_progress_sender,
                term_receiver,
            )
            .await
            .expect("Upload failed to key value store")
        });
    }

    loop {
        tokio::select! {
            _ = &mut receiver => break,
            Some(status_update) = progress_receiver.recv() => {
                updates.insert(status_update);
                let update_db_state = status_update == checkpoint_number;
                if update_db_state {
                    while updates.remove(&checkpoint_number) {
                        checkpoint_number += 1;
                    }
                    client.update_state(checkpoint_number).await?;
                    metrics
                        .latest_checkpoint_uploaded_to_kv_store
                        .set(checkpoint_number as i64);
                }
            }
        }
    }
    Ok(())
}

pub async fn uploader<S>(
    shard_id: u64,
    mut checkpoint_number: CheckpointSequenceNumber,
    store: S,
    config: TransactionKeyValueStoreWriteConfig,
    progress_sender: mpsc::Sender<u64>,
    mut receiver: oneshot::Receiver<()>,
) -> Result<()>
where
    S: ReadStore + Send,
    <S as ReadStore>::Error: Send,
{
    let mut client = DynamoDbClient::new(&config).await;
    while receiver.try_recv().is_err() {
        if let Some(checkpoint_summary) = store
            .get_checkpoint_by_sequence_number(checkpoint_number + shard_id)
            .map_err(|_| anyhow!("Failed to read checkpoint summary from store"))?
        {
            if let Some(contents) = store
                .get_full_checkpoint_contents(&checkpoint_summary.content_digest)
                .map_err(|_| anyhow!("Failed to read checkpoint content from store"))?
            {
                let mut transactions = vec![];
                let mut effects = vec![];
                let mut events = vec![];

                for content in contents.iter() {
                    let transaction_digest = content.transaction.digest().into_inner().to_vec();
                    effects.push((transaction_digest.clone(), content.effects.clone()));
                    transactions.push((transaction_digest, content.transaction.clone()));

                    if let Some(event_digest) = content.effects.events_digest() {
                        if let Some(tx_events) = store
                            .get_transaction_events(event_digest)
                            .map_err(|_| anyhow!("Failed to fetch events from the store"))?
                        {
                            events.push((event_digest.into_inner().to_vec(), tx_events));
                        }
                    }
                }
                client
                    .multi_set(KVTable::Transactions, transactions)
                    .await?;
                client.multi_set(KVTable::Effects, effects).await?;
                client.multi_set(KVTable::Events, events).await?;

                let serialized_checkpoint_number = bcs::to_bytes(
                    &TaggedKey::CheckpointSequenceNumber(checkpoint_summary.sequence_number),
                )?;
                client
                    .multi_set(
                        KVTable::CheckpointSummary,
                        [
                            serialized_checkpoint_number.clone(),
                            checkpoint_summary.digest().into_inner().to_vec(),
                        ]
                        .into_iter()
                        .zip(repeat(checkpoint_summary.inner())),
                    )
                    .await?;
                for key in [
                    serialized_checkpoint_number,
                    checkpoint_summary.content_digest.into_inner().to_vec(),
                ] {
                    client
                        .upload_blob(
                            KVTable::CheckpointContent,
                            key,
                            contents.checkpoint_contents(),
                        )
                        .await?;
                }
                progress_sender.send(checkpoint_number + shard_id).await?;
                checkpoint_number += config.concurrency as u64;
                continue;
            }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    Ok(())
}
