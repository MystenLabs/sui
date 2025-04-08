// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use aws_config::timeout::TimeoutConfig;
use aws_sdk_dynamodb::config::{Credentials, Region};
use aws_sdk_dynamodb::error::SdkError;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use std::str::FromStr;
use std::time::Duration;
use sui_data_ingestion_core::ProgressStore;
use sui_kvstore::BigTableProgressStore;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

pub struct DynamoDBProgressStore {
    client: Client,
    table_name: String,
    is_backfill: bool,
    bigtable_store: Option<BigTableProgressStore>,
}

impl DynamoDBProgressStore {
    pub async fn new(
        aws_access_key_id: &str,
        aws_secret_access_key: &str,
        aws_region: String,
        table_name: String,
        is_backfill: bool,
        bigtable_store: Option<BigTableProgressStore>,
    ) -> Self {
        let credentials = Credentials::new(
            aws_access_key_id,
            aws_secret_access_key,
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
            .region(Region::new(aws_region))
            .timeout_config(timeout_config)
            .load()
            .await;
        let client = Client::new(&aws_config);
        Self {
            client,
            table_name,
            is_backfill,
            bigtable_store,
        }
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
            if let AttributeValue::N(checkpoint_number) = &output["nstate"] {
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
        if self.is_backfill && checkpoint_number % 1000 != 0 {
            return Ok(());
        }
        if task_name == "bigtable" {
            if let Some(ref mut bigtable_store) = self.bigtable_store {
                bigtable_store
                    .save(task_name.clone(), checkpoint_number)
                    .await?;
            }
        }
        let backoff = backoff::ExponentialBackoff::default();
        backoff::future::retry(backoff, || async {
            let result = self
                .client
                .update_item()
                .table_name(self.table_name.clone())
                .key("task_name", AttributeValue::S(task_name.clone()))
                .update_expression("SET #nstate = :newState")
                .condition_expression("#nstate < :newState")
                .expression_attribute_names("#nstate", "nstate")
                .expression_attribute_values(
                    ":newState",
                    AttributeValue::N(checkpoint_number.to_string()),
                )
                .send()
                .await;
            match result {
                Ok(_) => Ok(()),
                Err(SdkError::ServiceError(err))
                    if err.err().is_conditional_check_failed_exception() =>
                {
                    Ok(())
                }
                Err(err) => Err(backoff::Error::transient(err)),
            }
        })
        .await?;
        Ok(())
    }
}
