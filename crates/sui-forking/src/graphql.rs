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
struct LatestCheckpointResponse {
    checkpoint: CheckpointNumberProtocolVersion,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChainIdentifierResponse {
    chain_identifier: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckpointNumberProtocolVersion {
    sequence_number: u64,
    query: QueryData,
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

    pub fn endpoint(&self) -> &str {
        &self.endpoint
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
        extract_protocol_version(graphql_response)
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
        extract_latest_checkpoint_and_protocol_version(graphql_response)
    }

    /// Fetch the network chain identifier from GraphQL.
    pub async fn fetch_chain_identifier(&self) -> Result<String> {
        let request = GraphQLRequest {
            query: "query { chainIdentifier }".to_string(),
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

        let graphql_response: GraphQLResponse<ChainIdentifierResponse> = response
            .json()
            .await
            .context("Failed to parse GraphQL response")?;
        extract_chain_identifier(graphql_response)
    }
}

fn extract_graphql_data<T>(graphql_response: GraphQLResponse<T>) -> Result<T> {
    if let Some(errors) = graphql_response.errors {
        let error_messages: Vec<String> = errors.into_iter().map(|e| e.message).collect();
        anyhow::bail!("GraphQL errors: {}", error_messages.join(", "));
    }

    graphql_response.data.context("No data in GraphQL response")
}

fn extract_protocol_version(
    graphql_response: GraphQLResponse<ProtocolVersionResponse>,
) -> Result<u64> {
    let data = extract_graphql_data(graphql_response)?;
    Ok(data.checkpoint.query.protocol_configs.protocol_version)
}

fn extract_latest_checkpoint_and_protocol_version(
    graphql_response: GraphQLResponse<LatestCheckpointResponse>,
) -> Result<(u64, u64)> {
    let data = extract_graphql_data(graphql_response)?;
    Ok((
        data.checkpoint.sequence_number,
        data.checkpoint.query.protocol_configs.protocol_version,
    ))
}

fn extract_chain_identifier(
    graphql_response: GraphQLResponse<ChainIdentifierResponse>,
) -> Result<String> {
    let data = extract_graphql_data(graphql_response)?;
    data.chain_identifier
        .context("No chainIdentifier in GraphQL response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_protocol_version_uses_local_response() {
        let graphql_response: GraphQLResponse<ProtocolVersionResponse> =
            serde_json::from_value(serde_json::json!({
                "data": {
                    "checkpoint": {
                        "query": {
                            "protocolConfigs": {
                                "protocolVersion": 77
                            }
                        }
                    }
                }
            }))
            .expect("response parse");
        let protocol_version =
            extract_protocol_version(graphql_response).expect("protocol version");
        assert_eq!(protocol_version, 77);
    }

    #[test]
    fn fetch_latest_checkpoint_and_chain_identifier_use_local_responses() {
        let latest_response: GraphQLResponse<LatestCheckpointResponse> =
            serde_json::from_value(serde_json::json!({
                "data": {
                    "checkpoint": {
                        "sequenceNumber": 111,
                        "query": {
                            "protocolConfigs": {
                                "protocolVersion": 88
                            }
                        }
                    }
                }
            }))
            .expect("latest response parse");
        let (checkpoint, protocol_version) =
            extract_latest_checkpoint_and_protocol_version(latest_response)
                .expect("latest checkpoint");
        assert_eq!(checkpoint, 111);
        assert_eq!(protocol_version, 88);

        let chain_response: GraphQLResponse<ChainIdentifierResponse> =
            serde_json::from_value(serde_json::json!({
                "data": {
                    "chainIdentifier": "7f7ad12684f6f7325e5f279ce8f7f46dbf51b97f34f3412f178ff6424fdaceda"
                }
            }))
            .expect("chain response parse");
        let chain_identifier = extract_chain_identifier(chain_response).expect("chain identifier");
        assert_eq!(
            chain_identifier,
            "7f7ad12684f6f7325e5f279ce8f7f46dbf51b97f34f3412f178ff6424fdaceda"
        );
    }
}
