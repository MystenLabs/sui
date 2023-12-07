// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::progress_store::ProgressStore;
use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use aws_sdk_s3::config::{Credentials, Region};
use std::str::FromStr;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

pub struct DynamoDBProgressStore {
    client: Client,
    table_name: String,
}

impl DynamoDBProgressStore {
    pub async fn new(
        aws_access_key_id: &str,
        aws_secret_access_key: &str,
        aws_region: String,
        table_name: String,
    ) -> Self {
        let credentials = Credentials::new(
            aws_access_key_id,
            aws_secret_access_key,
            None,
            None,
            "dynamodb",
        );
        let aws_config = aws_config::from_env()
            .credentials_provider(credentials)
            .region(Region::new(aws_region))
            .load()
            .await;
        let client = Client::new(&aws_config);
        Self { client, table_name }
    }
}

#[async_trait]
impl ProgressStore for DynamoDBProgressStore {
    async fn load(&mut self, task_name: String) -> Result<CheckpointSequenceNumber> {
        let item = self
            .client
            .get_item()
            .table_name(self.table_name.clone())
            .key("task_name", AttributeValue::S(task_name))
            .send()
            .await?;
        if let Some(output) = item.item() {
            if let AttributeValue::S(checkpoint_number) = &output["state"] {
                return Ok(CheckpointSequenceNumber::from_str(checkpoint_number)?);
            }
        }
        Ok(0)
    }
    async fn save(
        &mut self,
        task_name: String,
        checkpoint_number: CheckpointSequenceNumber,
    ) -> Result<()> {
        self.client
            .put_item()
            .table_name(self.table_name.clone())
            .item("task_name", AttributeValue::S(task_name))
            .item("state", AttributeValue::S(checkpoint_number.to_string()))
            .send()
            .await?;
        Ok(())
    }
}
