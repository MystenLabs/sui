// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use aws_config::timeout::TimeoutConfig;
use aws_sdk_dynamodb::primitives::Blob;
use aws_sdk_dynamodb::types::{AttributeValue, PutRequest, WriteRequest};
use aws_sdk_dynamodb::Client;
use aws_sdk_s3 as s3;
use aws_sdk_s3::config::{Credentials, Region};
use backoff::backoff::Backoff;
use backoff::ExponentialBackoff;
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::collections::{HashMap, HashSet, VecDeque};
use std::iter::repeat;
use std::time::{Duration, Instant};
use sui_data_ingestion_core::Worker;
use sui_storage::http_key_value_store::TaggedKey;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::storage::ObjectKey;
use tracing::error;

const TIMEOUT: Duration = Duration::from_secs(60);
const DDB_SIZE_LIMIT: usize = 399000;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KVStoreTaskConfig {
    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub aws_region: String,
    pub table_name: String,
    pub bucket_name: String,
}

#[derive(Clone)]
pub struct KVStoreWorker {
    dynamo_client: Client,
    s3_client: s3::Client,
    bucket_name: String,
    table_name: String,
}

#[derive(Hash, Eq, PartialEq, Debug, Copy, Clone)]
pub enum KVTable {
    Transactions,
    Effects,
    Events,
    Objects,
    CheckpointSummary,
    TransactionToCheckpoint,
}

impl KVStoreWorker {
    pub async fn new(config: KVStoreTaskConfig) -> Self {
        let credentials = Credentials::new(
            &config.aws_access_key_id,
            &config.aws_secret_access_key,
            None,
            None,
            "dynamodb",
        );
        let timeout_config = TimeoutConfig::builder()
            .operation_timeout(Duration::from_secs(3))
            .operation_attempt_timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(3))
            .build();
        let aws_config = aws_config::from_env()
            .credentials_provider(credentials)
            .region(Region::new(config.aws_region))
            .timeout_config(timeout_config)
            .load()
            .await;
        let dynamo_client = Client::new(&aws_config);
        let s3_client = s3::Client::new(&aws_config);
        Self {
            dynamo_client,
            s3_client,
            bucket_name: config.bucket_name,
            table_name: config.table_name,
        }
    }

    async fn multi_set<V: Serialize>(
        &self,
        table: KVTable,
        values: impl IntoIterator<Item = (Vec<u8>, V)> + std::marker::Send,
    ) -> anyhow::Result<()> {
        let instant = Instant::now();
        let mut items = vec![];
        let mut seen = HashSet::new();
        for (digest, value) in values {
            if seen.contains(&digest) {
                continue;
            }
            seen.insert(digest.clone());
            let bytes = bcs::to_bytes(value.borrow())?;
            if bytes.len() > DDB_SIZE_LIMIT {
                error!("large value for table {:?} and key {:?}", table, digest);
                continue;
            }
            let item = WriteRequest::builder()
                .set_put_request(Some(
                    PutRequest::builder()
                        .item("digest", AttributeValue::B(Blob::new(digest)))
                        .item("type", AttributeValue::S(Self::type_name(table)))
                        .item("bcs", AttributeValue::B(Blob::new(bytes)))
                        .build(),
                ))
                .build();
            items.push(item);
        }
        if items.is_empty() {
            return Ok(());
        }
        let mut backoff = ExponentialBackoff::default();
        let mut queue: VecDeque<Vec<_>> = items.chunks(25).map(|ck| ck.to_vec()).collect();
        while let Some(chunk) = queue.pop_front() {
            if instant.elapsed() > TIMEOUT {
                return Err(anyhow!("key value worker timed out"));
            }
            let response = self
                .dynamo_client
                .batch_write_item()
                .set_request_items(Some(HashMap::from([(
                    self.table_name.clone(),
                    chunk.to_vec(),
                )])))
                .send()
                .await?;
            if let Some(response) = response.unprocessed_items {
                if let Some(unprocessed) = response.into_iter().next() {
                    if !unprocessed.1.is_empty() {
                        if queue.is_empty() {
                            if let Some(duration) = backoff.next_backoff() {
                                tokio::time::sleep(duration).await;
                            }
                        }
                        queue.push_back(unprocessed.1);
                    }
                }
            }
        }
        Ok(())
    }

    async fn upload_blob<V: Serialize + std::marker::Send>(
        &self,
        key: Vec<u8>,
        value: V,
    ) -> anyhow::Result<()> {
        let body = bcs::to_bytes(value.borrow())?.into();
        self.s3_client
            .put_object()
            .bucket(self.bucket_name.clone())
            .key(base64_url::encode(&key))
            .body(body)
            .send()
            .await?;
        Ok(())
    }

    fn type_name(table: KVTable) -> String {
        match table {
            KVTable::Transactions => "tx",
            KVTable::Effects => "fx",
            KVTable::Events => "ev",
            KVTable::Objects => "ob",
            KVTable::CheckpointSummary => "cs",
            KVTable::TransactionToCheckpoint => "tx2c",
        }
        .to_string()
    }
}

#[async_trait]
impl Worker for KVStoreWorker {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> Result<()> {
        let mut transactions = vec![];
        let mut effects = vec![];
        let mut events = vec![];
        let mut objects = vec![];
        let mut transactions_to_checkpoint = vec![];
        let checkpoint_number = checkpoint.checkpoint_summary.sequence_number;

        for transaction in &checkpoint.transactions {
            let transaction_digest = transaction.transaction.digest().into_inner().to_vec();
            effects.push((transaction_digest.clone(), transaction.effects.clone()));
            transactions_to_checkpoint.push((transaction_digest.clone(), checkpoint_number));
            transactions.push((transaction_digest, transaction.transaction.clone()));

            if let Some(tx_events) = &transaction.events {
                events.push((tx_events.digest().into_inner().to_vec(), tx_events));
            }
            for object in &transaction.output_objects {
                let object_key = ObjectKey(object.id(), object.version());
                objects.push((bcs::to_bytes(&object_key)?, object));
            }
        }
        self.multi_set(KVTable::Transactions, transactions).await?;
        self.multi_set(KVTable::Effects, effects).await?;
        self.multi_set(KVTable::Events, events).await?;
        self.multi_set(KVTable::Objects, objects).await?;
        self.multi_set(KVTable::TransactionToCheckpoint, transactions_to_checkpoint)
            .await?;

        let serialized_checkpoint_number =
            bcs::to_bytes(&TaggedKey::CheckpointSequenceNumber(checkpoint_number))?;
        let checkpoint_summary = &checkpoint.checkpoint_summary;
        for key in [
            serialized_checkpoint_number.clone(),
            checkpoint_summary.content_digest.into_inner().to_vec(),
        ] {
            self.upload_blob(key, checkpoint.checkpoint_contents.clone())
                .await?;
        }
        self.multi_set(
            KVTable::CheckpointSummary,
            [
                serialized_checkpoint_number,
                checkpoint_summary.digest().into_inner().to_vec(),
            ]
            .into_iter()
            .zip(repeat(checkpoint_summary.data())),
        )
        .await?;
        Ok(())
    }
}
