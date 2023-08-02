// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use aws_sdk_dynamodb as dynamodb;
use aws_sdk_dynamodb::config::{Credentials, Region};
use aws_sdk_dynamodb::primitives::Blob;
use aws_sdk_dynamodb::types::{AttributeValue, PutRequest, WriteRequest};
use aws_sdk_s3 as s3;
use serde::Serialize;
use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use sui_config::node::TransactionKeyValueStoreWriteConfig;

#[derive(Hash, Eq, PartialEq, Debug, Copy, Clone)]
pub enum KVTable {
    Transactions,
    Effects,
    Events,
    CheckpointContent,
    CheckpointSummary,
    State,
}

const UPLOAD_PROGRESS_KEY: [u8; 1] = [0];

#[async_trait]
pub trait KVWriteClient {
    async fn multi_set<V: Serialize>(
        &mut self,
        table: KVTable,
        values: impl IntoIterator<Item = (Vec<u8>, V)> + std::marker::Send,
    ) -> anyhow::Result<()>;
    async fn get_state(&self) -> anyhow::Result<Option<u64>>;
    async fn update_state(&mut self, value: u64) -> anyhow::Result<()>;
    async fn upload_blob<V: Serialize + std::marker::Send>(
        &mut self,
        table: KVTable,
        key: Vec<u8>,
        value: V,
    ) -> anyhow::Result<()>;

    fn deserialize_state(bytes: Vec<u8>) -> u64 {
        let mut array: [u8; 8] = [0; 8];
        array.copy_from_slice(&bytes);
        u64::from_be_bytes(array)
    }
}

pub struct DynamoDbClient {
    dynamo_client: dynamodb::Client,
    s3_client: s3::Client,
    table_name: String,
    bucket_name: String,
}

impl DynamoDbClient {
    pub async fn new(config: &TransactionKeyValueStoreWriteConfig) -> Self {
        let credentials = Credentials::new(
            &config.aws_access_key_id,
            &config.aws_secret_access_key,
            None,
            None,
            "dynamodb",
        );
        let aws_config = aws_config::from_env()
            .credentials_provider(credentials)
            .region(Region::new(config.aws_region.clone()))
            .load()
            .await;
        let dynamo_client = dynamodb::Client::new(&aws_config);
        let s3_client = s3::Client::new(&aws_config);
        Self {
            dynamo_client,
            s3_client,
            table_name: config.table_name.clone(),
            bucket_name: config.bucket_name.clone(),
        }
    }

    fn type_name(table: KVTable) -> String {
        match table {
            KVTable::Transactions => "tx",
            KVTable::Effects => "fx",
            KVTable::Events => "ev",
            KVTable::State => "state",
            KVTable::CheckpointContent => "cc",
            KVTable::CheckpointSummary => "cs",
        }
        .to_string()
    }
}

#[async_trait]
impl KVWriteClient for DynamoDbClient {
    async fn multi_set<V: Serialize>(
        &mut self,
        table: KVTable,
        values: impl IntoIterator<Item = (Vec<u8>, V)> + std::marker::Send,
    ) -> anyhow::Result<()> {
        let mut items = vec![];
        let mut seen = HashSet::new();
        for (digest, value) in values {
            if seen.contains(&digest) {
                continue;
            }
            seen.insert(digest.clone());
            let item = WriteRequest::builder()
                .set_put_request(Some(
                    PutRequest::builder()
                        .item("digest", AttributeValue::B(Blob::new(digest)))
                        .item("type", AttributeValue::S(Self::type_name(table)))
                        .item(
                            "bcs",
                            AttributeValue::B(Blob::new(bcs::to_bytes(value.borrow())?)),
                        )
                        .build(),
                ))
                .build();
            items.push(item);
        }
        if items.is_empty() {
            return Ok(());
        }
        for chunk in items.chunks(25) {
            self.dynamo_client
                .batch_write_item()
                .set_request_items(Some(HashMap::from([(
                    self.table_name.clone(),
                    chunk.to_vec(),
                )])))
                .send()
                .await?;
        }
        Ok(())
    }

    async fn get_state(&self) -> anyhow::Result<Option<u64>> {
        let item = self
            .dynamo_client
            .get_item()
            .table_name(self.table_name.clone())
            .key("digest", AttributeValue::B(Blob::new(UPLOAD_PROGRESS_KEY)))
            .key("type", AttributeValue::S("state".to_string()))
            .send()
            .await?;
        if let Some(output) = item.item() {
            if let AttributeValue::B(progress) = &output["value"] {
                return Ok(Some(bcs::from_bytes(&progress.clone().into_inner())?));
            }
        }
        Ok(None)
    }

    async fn update_state(&mut self, value: u64) -> anyhow::Result<()> {
        self.dynamo_client
            .put_item()
            .table_name(self.table_name.clone())
            .item("digest", AttributeValue::B(Blob::new(UPLOAD_PROGRESS_KEY)))
            .item("type", AttributeValue::S("state".to_string()))
            .item(
                "value",
                AttributeValue::B(Blob::new(bcs::to_bytes(&value)?)),
            )
            .send()
            .await?;
        Ok(())
    }

    async fn upload_blob<V: Serialize + std::marker::Send>(
        &mut self,
        _table: KVTable,
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
}
