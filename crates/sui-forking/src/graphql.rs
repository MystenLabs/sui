// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::info;

/// GraphQL client for fetching data from Sui network
pub struct GraphQLClient {
    endpoint: String,
    client: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct GraphQLRequest {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    variables: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct GraphQLResponse<T> {
    data: Option<T>,
    errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Deserialize)]
struct GraphQLError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct CheckpointResponse {
    checkpoint: CheckpointData,
}

#[derive(Debug, Deserialize)]
struct LatestCheckpointResponse {
    checkpoint: CheckpointNumberProtocolVersion,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckpointNumberProtocolVersion {
    sequence_number: u64,
    query: QueryData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointData {
    pub content_bcs: String,
}

#[derive(Debug, Deserialize)]
struct ProtocolVersionResponse {
    checkpoint: CheckpointWithQuery,
}

#[derive(Debug, Deserialize)]
struct CheckpointWithQuery {
    query: QueryData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryData {
    protocol_configs: ProtocolConfigs,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolConfigs {
    pub protocol_version: u64,
}

impl GraphQLClient {
    pub fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            client: reqwest::Client::new(),
        }
    }

    /// Fetch a checkpoint by sequence number. If none is provided, fetch the latest checkpoint.
    pub async fn fetch_checkpoint(&self, sequence_number: u64) -> Result<CheckpointData> {
        let query = format!(
            r#"{{
  checkpoint(sequenceNumber:{}) {{
    contentBcs
  }}
}}"#,
            sequence_number
        );

        let request = GraphQLRequest {
            query,
            variables: None,
        };

        let response = self
            .client
            .post(&self.endpoint)
            .json(&request)
            .send()
            .await
            .context("Failed to send GraphQL request")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());
            anyhow::bail!(
                "GraphQL request failed with status {}: {}",
                status,
                error_text
            );
        }

        let graphql_response: GraphQLResponse<CheckpointResponse> = response
            .json()
            .await
            .context("Failed to parse GraphQL response")?;

        if let Some(errors) = graphql_response.errors {
            let error_messages: Vec<String> = errors.into_iter().map(|e| e.message).collect();
            anyhow::bail!("GraphQL errors: {}", error_messages.join(", "));
        }

        let data = graphql_response
            .data
            .context("No data in GraphQL response")?;

        Ok(data.checkpoint)
    }

    /// Fetch protocol version at a specific checkpoint
    pub async fn fetch_protocol_version(&self, sequence_number: Option<u64>) -> Result<u64> {
        let query = if let Some(seq) = sequence_number {
            format!(
                r#"query {{
  checkpoint(sequenceNumber: {}) {{
    query {{
      protocolConfigs {{
        protocolVersion
      }}
    }}
  }}
}}"#,
                seq
            )
        } else {
            info!("No checkpoint provided, fetching last checkpoint's protocol version");
            "{
               checkpoint {
                 query {
                  protocolConfigs {
                    protocolVersion
                  }
                }
              }
             }"
            .to_string()
        };
        info!("GraphQL query: {}", query);

        let request = GraphQLRequest {
            query,
            variables: None,
        };

        let response = self
            .client
            .post(&self.endpoint)
            .json(&request)
            .send()
            .await
            .context("Failed to send GraphQL request")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());
            anyhow::bail!(
                "GraphQL request failed with status {}: {}",
                status,
                error_text
            );
        }

        let graphql_response: GraphQLResponse<ProtocolVersionResponse> = response
            .json()
            .await
            .context("Failed to parse GraphQL response")?;

        if let Some(errors) = graphql_response.errors {
            let error_messages: Vec<String> = errors.into_iter().map(|e| e.message).collect();
            anyhow::bail!("GraphQL errors: {}", error_messages.join(", "));
        }

        let data = graphql_response
            .data
            .context("No data in GraphQL response")?;

        Ok(data.checkpoint.query.protocol_configs.protocol_version)
    }

    pub async fn fetch_latest_checkpoint_and_protocol_version(&self) -> Result<(u64, u64)> {
        let query = "query {
              checkpoint {
                sequenceNumber
                query {
                  protocolConfigs {
                    protocolVersion
                  }
                }
              }
            }"
        .to_string();

        let request = GraphQLRequest {
            query,
            variables: None,
        };

        let response = self
            .client
            .post(&self.endpoint)
            .json(&request)
            .send()
            .await
            .context("Failed to send GraphQL request")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());
            anyhow::bail!(
                "GraphQL request failed with status {}: {}",
                status,
                error_text
            );
        }

        let graphql_response: GraphQLResponse<LatestCheckpointResponse> = response
            .json()
            .await
            .context("Failed to parse GraphQL response")?;

        if let Some(errors) = graphql_response.errors {
            let error_messages: Vec<String> = errors.into_iter().map(|e| e.message).collect();
            anyhow::bail!("GraphQL errors: {}", error_messages.join(", "));
        }

        let data = graphql_response
            .data
            .context("No data in GraphQL response")?;

        Ok((
            data.checkpoint.sequence_number,
            data.checkpoint.query.protocol_configs.protocol_version,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_checkpoint_0_mainnet() {
        let client = GraphQLClient::new("https://graphql.mainnet.sui.io/graphql".to_string());
        let checkpoint = client.fetch_checkpoint(Some(0)).await.unwrap();
        assert!(!checkpoint.content_bcs.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_checkpoint_0_testnet() {
        let client = GraphQLClient::new("https://graphql.testnet.sui.io/graphql".to_string());
        let checkpoint = client.fetch_checkpoint(Some(0)).await.unwrap();
        assert!(!checkpoint.content_bcs.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_protocol_version_mainnet() {
        let client = GraphQLClient::new("https://graphql.mainnet.sui.io/graphql".to_string());
        let protocol_version = client.fetch_protocol_version(Some(0)).await.unwrap();
        assert!(protocol_version > 0);
    }

    #[tokio::test]
    async fn test_fetch_protocol_version_testnet() {
        let client = GraphQLClient::new("https://graphql.testnet.sui.io/graphql".to_string());
        let protocol_version = client.fetch_protocol_version(Some(0)).await.unwrap();
        assert!(protocol_version > 0);
    }
}
