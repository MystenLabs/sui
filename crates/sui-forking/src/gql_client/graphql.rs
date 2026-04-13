// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Error, Result};
use cynic::{GraphQlResponse, Operation};
use reqwest::header::USER_AGENT;

use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;
use sui_types::supported_protocol_versions::ProtocolConfig;

use forking_data_store::CheckpointStore;
use forking_data_store::EpochData;
use forking_data_store::EpochStore;
use forking_data_store::Node;
use forking_data_store::ObjectKey;
use forking_data_store::ObjectStore;
use forking_data_store::TransactionInfo;
use forking_data_store::TransactionStore;

macro_rules! block_on {
    ($expr:expr) => {{
        #[allow(clippy::disallowed_methods, clippy::result_large_err)]
        {
            if tokio::runtime::Handle::try_current().is_ok() {
                std::thread::scope(|scope| {
                    scope
                        .spawn(|| {
                            let rt = tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()
                                .expect("failed to build Tokio runtime");
                            rt.block_on($expr)
                        })
                        .join()
                        .expect("failed to join scoped thread running nested runtime")
                })
            } else {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build Tokio runtime");
                rt.block_on($expr)
            }
        }
    }};
}

/// Remote GraphQL-backed store.
#[derive(Debug, Clone)]
pub struct GraphQLStore {
    client: reqwest::Client,
    rpc: reqwest::Url,
    version: String,
}

impl GraphQLStore {
    /// Create a new GraphQL-backed store.
    pub fn new(node: Node, version: &str) -> Result<Self, Error> {
        let rpc = reqwest::Url::parse(node.gql_url())
            .with_context(|| format!("invalid GraphQL URL '{}'", node.gql_url()))?;
        Ok(Self {
            client: reqwest::Client::new(),
            rpc,
            version: version.to_string(),
        })
    }

    pub(crate) async fn run_query<T, V>(
        &self,
        operation: &Operation<T, V>,
    ) -> Result<GraphQlResponse<T>, Error>
    where
        T: serde::de::DeserializeOwned,
        V: serde::Serialize,
    {
        Self::run_query_internal(&self.client, &self.rpc, &self.version, operation).await
    }

    async fn run_query_internal<T, V>(
        client: &reqwest::Client,
        rpc: &reqwest::Url,
        version: &str,
        operation: &Operation<T, V>,
    ) -> Result<GraphQlResponse<T>, Error>
    where
        T: serde::de::DeserializeOwned,
        V: serde::Serialize,
    {
        client
            .post(rpc.clone())
            .header(USER_AGENT, format!("forking-data-store-v{}", version))
            .json(operation)
            .send()
            .await
            .context("Failed to send GQL query")?
            .json::<GraphQlResponse<T>>()
            .await
            .context("Failed to read response in GQL query")
    }

    async fn get_verified_checkpoint_impl(
        &self,
        sequence_number: Option<CheckpointSequenceNumber>,
    ) -> Result<Option<VerifiedCheckpoint>, Error> {
        super::gql_queries::checkpoint_query::query(sequence_number, self).await
    }
}

impl TransactionStore for GraphQLStore {
    fn transaction_data_and_effects(
        &self,
        _tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        todo!("GraphQL transaction reads are not implemented in the skeleton")
    }
}

impl EpochStore for GraphQLStore {
    fn epoch_info(&self, _epoch: u64) -> Result<Option<EpochData>, Error> {
        todo!("GraphQL epoch reads are not implemented in the skeleton")
    }

    fn protocol_config(&self, _epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        todo!("GraphQL protocol-config reads are not implemented in the skeleton")
    }
}

impl ObjectStore for GraphQLStore {
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        block_on!(super::gql_queries::object_query::query(keys, self))
    }
}

impl CheckpointStore for GraphQLStore {
    fn get_verified_checkpoint(
        &self,
        sequence: Option<CheckpointSequenceNumber>,
    ) -> Result<Option<VerifiedCheckpoint>, Error> {
        Ok(block_on!(self.get_verified_checkpoint_impl(sequence))?)
    }
}

#[cfg(test)]
mod tests {
    use cynic::QueryBuilder;
    use fastcrypto::encoding::Base64 as FastCryptoBase64;
    use serde_json::json;
    use sui_types::{base_types::ObjectID, test_checkpoint_data_builder::TestCheckpointBuilder};
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use forking_data_store::VersionQuery;
    use super::super::gql_queries::checkpoint_query::{CheckpointArgs, Query as CheckpointQuery};

    fn mock_store(server: &MockServer) -> GraphQLStore {
        GraphQLStore::new(Node::Custom(server.uri()), "test-version").expect("store should build")
    }

    fn checkpoint_response_body(
        certified: &sui_types::messages_checkpoint::CertifiedCheckpointSummary,
    ) -> serde_json::Value {
        json!({
            "data": {
                "checkpoint": {
                    "summaryBcs": FastCryptoBase64::from_bytes(
                        &bcs::to_bytes(certified.data()).expect("summary should serialize"),
                    )
                    .encoded(),
                    "validatorSignatures": {
                        "signature": FastCryptoBase64::from_bytes(
                            certified.auth_sig().signature.as_ref(),
                        )
                        .encoded(),
                        "signersMap": certified
                            .auth_sig()
                            .signers_map
                            .iter()
                            .map(|index| i32::try_from(index).expect("signer index fits in i32"))
                            .collect::<Vec<_>>(),
                    },
                }
            }
        })
    }

    fn object_response_body(objects: &[Option<&Object>]) -> serde_json::Value {
        json!({
            "data": {
                "multiGetObjects": objects
                    .iter()
                    .map(|object| {
                        object.as_ref().map(|object| {
                            json!({
                                "address": object.id().to_string(),
                                "version": object.version().value(),
                                "objectBcs": FastCryptoBase64::from_bytes(
                                    &bcs::to_bytes(*object).expect("object should serialize"),
                                )
                                .encoded(),
                            })
                        })
                    })
                    .collect::<Vec<_>>(),
            }
        })
    }

    fn versioned_object_at_checkpoint_response_body(object: Option<&Object>) -> serde_json::Value {
        json!({
            "data": {
                "checkpoint": {
                    "query": {
                        "object": object.map(|object| {
                            json!({
                                "address": object.id().to_string(),
                                "version": object.version().value(),
                                "objectBcs": FastCryptoBase64::from_bytes(
                                    &bcs::to_bytes(object).expect("object should serialize"),
                                )
                                .encoded(),
                            })
                        })
                    }
                }
            }
        })
    }

    #[tokio::test]
    async fn test_run_query() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .and(header("user-agent", "forking-data-store-vtest-version"))
            .and(body_partial_json(json!({
                "variables": {
                    "sequenceNumber": 7,
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {
                    "checkpoint": null,
                }
            })))
            .mount(&server)
            .await;

        let store = mock_store(&server);
        let operation = CheckpointQuery::build(CheckpointArgs {
            sequence_number: Some(7),
        });

        let response = store
            .run_query(&operation)
            .await
            .expect("query should succeed");
        assert!(response.data.is_some());

        let requests = server
            .received_requests()
            .await
            .expect("wiremock should record requests");
        let request_body: serde_json::Value = requests[0]
            .body_json()
            .expect("request body should be json");
        let query = request_body
            .get("query")
            .and_then(serde_json::Value::as_str)
            .expect("query string should be present");
        assert!(query.contains("checkpoint"));
        assert!(query.contains("summaryBcs"));
        assert!(query.contains("validatorSignatures"));
    }

    #[tokio::test]
    async fn test_get_checkpoint_by_sequence_number() {
        let server = MockServer::start().await;
        let checkpoint = TestCheckpointBuilder::new(11).build_checkpoint();

        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_partial_json(json!({
                "variables": {
                    "sequenceNumber": 11,
                }
            })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(checkpoint_response_body(&checkpoint.summary)),
            )
            .mount(&server)
            .await;

        let store = mock_store(&server);
        let verified = store
            .get_verified_checkpoint_impl(Some(11))
            .await
            .expect("checkpoint query should succeed")
            .expect("checkpoint should be present");

        assert_eq!(verified.data(), checkpoint.summary.data());
        assert_eq!(
            verified.auth_sig().epoch,
            checkpoint.summary.auth_sig().epoch
        );
        assert_eq!(
            verified.auth_sig().signature.as_ref(),
            checkpoint.summary.auth_sig().signature.as_ref()
        );
        assert_eq!(
            verified.auth_sig().signers_map,
            checkpoint.summary.auth_sig().signers_map
        );
    }

    #[tokio::test]
    async fn test_get_objects() {
        let server = MockServer::start().await;
        let versioned_object = Object::immutable_with_id_for_testing(ObjectID::random());
        let root_version_object = Object::immutable_with_id_for_testing(ObjectID::random());
        let missing_object_id = ObjectID::random();

        Mock::given(method("POST"))
            .and(path("/"))
            .and(header("user-agent", "forking-data-store-vtest-version"))
            .and(body_partial_json(json!({
                "variables": {
                    "keys": [
                        {
                            "address": versioned_object.id().to_string(),
                            "version": versioned_object.version().value(),
                        },
                        {
                            "address": root_version_object.id().to_string(),
                            "rootVersion": 17,
                        },
                        {
                            "address": missing_object_id.to_string(),
                            "atCheckpoint": 29,
                        },
                    ],
                }
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(object_response_body(&[
                    Some(&versioned_object),
                    Some(&root_version_object),
                    None,
                ])),
            )
            .mount(&server)
            .await;

        let store = mock_store(&server);
        let objects = store
            .get_objects(&[
                ObjectKey {
                    object_id: versioned_object.id(),
                    version_query: VersionQuery::Version(versioned_object.version().value()),
                },
                ObjectKey {
                    object_id: root_version_object.id(),
                    version_query: VersionQuery::RootVersion(17),
                },
                ObjectKey {
                    object_id: missing_object_id,
                    version_query: VersionQuery::AtCheckpoint(29),
                },
            ])
            .expect("object query should succeed");

        assert_eq!(
            objects,
            vec![
                Some((versioned_object.clone(), versioned_object.version().value())),
                Some((
                    root_version_object.clone(),
                    root_version_object.version().value(),
                )),
                None,
            ]
        );

        let requests = server
            .received_requests()
            .await
            .expect("wiremock should record requests");
        let request_body: serde_json::Value = requests[0]
            .body_json()
            .expect("request body should be json");
        let query = request_body
            .get("query")
            .and_then(serde_json::Value::as_str)
            .expect("query string should be present");
        assert!(query.contains("multiGetObjects"));
        assert!(query.contains("objectBcs"));
    }

    #[tokio::test]
    async fn test_get_object_exact_version_at_checkpoint() {
        let server = MockServer::start().await;
        let object = Object::immutable_with_id_for_testing(ObjectID::random());

        Mock::given(method("POST"))
            .and(path("/"))
            .and(header("user-agent", "forking-data-store-vtest-version"))
            .and(body_partial_json(json!({
                "variables": {
                    "sequenceNumber": 31,
                    "address": object.id().to_string(),
                    "version": object.version().value(),
                }
            })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(versioned_object_at_checkpoint_response_body(Some(&object))),
            )
            .mount(&server)
            .await;

        let store = mock_store(&server);
        let objects = store
            .get_objects(&[ObjectKey {
                object_id: object.id(),
                version_query: VersionQuery::VersionAtCheckpoint {
                    version: object.version().value(),
                    checkpoint: 31,
                },
            }])
            .expect("versioned object query should succeed");

        assert_eq!(
            objects,
            vec![Some((object.clone(), object.version().value()))]
        );

        let requests = server
            .received_requests()
            .await
            .expect("wiremock should record requests");
        let request_body: serde_json::Value = requests[0]
            .body_json()
            .expect("request body should be json");
        let query = request_body
            .get("query")
            .and_then(serde_json::Value::as_str)
            .expect("query string should be present");
        assert!(query.contains("checkpoint"));
        assert!(query.contains("object(address: $address, version: $version)"));
        assert!(!query.contains("multiGetObjects"));
    }

    #[tokio::test]
    async fn test_get_object_exact_version_at_checkpoint_returns_none() {
        let server = MockServer::start().await;
        let object_id = ObjectID::random();

        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_partial_json(json!({
                "variables": {
                    "sequenceNumber": 31,
                    "address": object_id.to_string(),
                    "version": 7,
                }
            })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(versioned_object_at_checkpoint_response_body(None)),
            )
            .mount(&server)
            .await;

        let store = mock_store(&server);
        let objects = store
            .get_objects(&[ObjectKey {
                object_id,
                version_query: VersionQuery::VersionAtCheckpoint {
                    version: 7,
                    checkpoint: 31,
                },
            }])
            .expect("versioned object query should succeed");

        assert_eq!(objects, vec![None]);
    }
}
