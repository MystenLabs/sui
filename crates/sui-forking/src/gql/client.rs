// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use anyhow::Error;
use anyhow::Result;
use cynic::GraphQlResponse;
use cynic::Operation;
use reqwest::header::USER_AGENT;

use sui_protocol_config::Chain;
use sui_types::effects::TransactionEvents;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;
use sui_types::supported_protocol_versions::ProtocolConfig;

use crate::CheckpointRead;
use crate::Node;
use crate::ObjectKey;
use crate::ObjectRead;
use crate::TransactionInfo;
use crate::TransactionRead;
use crate::gql::AddressOwnedObject;
use crate::gql::queries;
use sui_types::base_types::SuiAddress;

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

/// GraphQL Client for querying the GraphQL service.
#[derive(Debug, Clone)]
pub struct GraphQLClient {
    client: reqwest::Client,
    node: Node,
    rpc: reqwest::Url,
    version: String,
}

impl GraphQLClient {
    /// Create a new GraphQL client
    pub fn new(node: Node, version: &str) -> Result<Self, Error> {
        let rpc = reqwest::Url::parse(node.gql_url())
            .with_context(|| format!("invalid GraphQL URL '{}'", node.gql_url()))?;
        Ok(Self {
            client: reqwest::Client::new(),
            node,
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
            .header(USER_AGENT, format!("sui-forking-v{}", version))
            .json(operation)
            .send()
            .await
            .context("Failed to send GQL query")?
            .json::<GraphQlResponse<T>>()
            .await
            .context("Failed to read response in GQL query")
    }

    pub(crate) fn chain(&self) -> Chain {
        match self.node {
            Node::Mainnet => Chain::Mainnet,
            Node::Testnet => Chain::Testnet,
            Node::Devnet => Chain::Unknown,
            Node::Custom(_) => Chain::Unknown,
        }
    }
}

impl TransactionRead for GraphQLClient {
    fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        block_on!(queries::txn_query::query(tx_digest.to_owned(), self))
    }
}

impl GraphQLClient {
    /// Fetch address-owned object metadata at a checkpoint, paginating through
    /// the checkpoint-scoped ownership connection.
    pub(crate) async fn get_address_owned_objects_at_checkpoint(
        &self,
        address: SuiAddress,
        checkpoint: CheckpointSequenceNumber,
    ) -> Result<Vec<AddressOwnedObject>, Error> {
        queries::address_owned_objects_query::query(address, checkpoint, self).await
    }

    /// Get the latest checkpoint sequence number from GraphQL RPC.
    pub async fn get_latest_checkpoint_sequence_number(
        &self,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        queries::latest_checkpoint_query::query(self).await
    }

    /// Get a checkpoint (summary and contents) by sequence number from GraphQL RPC. If
    /// `sequence_number` is `None`, gets the latest checkpoint.
    async fn get_checkpoint_impl(
        &self,
        sequence_number: Option<CheckpointSequenceNumber>,
    ) -> Result<Option<(VerifiedCheckpoint, CheckpointContents)>, Error> {
        queries::checkpoint_query::query(sequence_number, self).await
    }

    /// Fetch all events for a transaction, paginating through the GraphQL
    /// events connection. Returns `None` if the transaction doesn't exist.
    pub(crate) fn get_transaction_events(
        &self,
        tx_digest: &str,
    ) -> Result<Option<TransactionEvents>, Error> {
        block_on!(queries::events_query::query(tx_digest, self))
    }

    /// Query `serviceConfig.availableRange` for both "Checkpoint" and
    /// "Transaction" types and return the max of their `first.sequenceNumber`.
    pub(crate) fn get_lowest_available_checkpoint(
        &self,
    ) -> Result<CheckpointSequenceNumber, Error> {
        let checkpoint_low = block_on!(queries::available_range_query::query("Checkpoint", self))?;
        let transaction_low =
            block_on!(queries::available_range_query::query("Transaction", self))?;
        Ok(checkpoint_low.max(transaction_low))
    }

    /// Query `serviceConfig.availableRange` for "Object" type and return
    /// `first.sequenceNumber`.
    pub(crate) fn get_lowest_available_checkpoint_objects(
        &self,
    ) -> Result<CheckpointSequenceNumber, Error> {
        block_on!(queries::available_range_query::query("Object", self))
    }
}

impl ObjectRead for GraphQLClient {
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        block_on!(crate::gql::queries::object_query::query(keys, self))
    }
}

impl CheckpointRead for GraphQLClient {
    fn get_checkpoint(
        &self,
        sequence: Option<CheckpointSequenceNumber>,
    ) -> Result<Option<(VerifiedCheckpoint, CheckpointContents)>, Error> {
        Ok(block_on!(self.get_checkpoint_impl(sequence))?)
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

    use super::super::queries::checkpoint_query::{CheckpointArgs, Query as CheckpointQuery};
    use super::*;
    use crate::VersionQuery;

    fn mock_store(server: &MockServer) -> GraphQLClient {
        GraphQLClient::new(Node::Custom(server.uri()), "test-version").expect("store should build")
    }

    fn checkpoint_response_body(
        certified: &sui_types::messages_checkpoint::CertifiedCheckpointSummary,
        contents: &CheckpointContents,
    ) -> serde_json::Value {
        json!({
            "data": {
                "checkpoint": {
                    "summaryBcs": FastCryptoBase64::from_bytes(
                        &bcs::to_bytes(certified.data()).expect("summary should serialize"),
                    )
                    .encoded(),
                    "contentBcs": FastCryptoBase64::from_bytes(
                        &bcs::to_bytes(contents).expect("contents should serialize"),
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
            .and(header("user-agent", "sui-forking-vtest-version"))
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
                ResponseTemplate::new(200).set_body_json(checkpoint_response_body(
                    &checkpoint.summary,
                    &checkpoint.contents,
                )),
            )
            .mount(&server)
            .await;

        let store = mock_store(&server);
        let (verified, contents) = store
            .get_checkpoint_impl(Some(11))
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
        assert_eq!(contents.digest(), checkpoint.contents.digest());
    }

    #[tokio::test]
    async fn test_get_objects() {
        let server = MockServer::start().await;
        let versioned_object = Object::immutable_with_id_for_testing(ObjectID::random());
        let root_version_object = Object::immutable_with_id_for_testing(ObjectID::random());
        let missing_object_id = ObjectID::random();

        Mock::given(method("POST"))
            .and(path("/"))
            .and(header("user-agent", "sui-forking-vtest-version"))
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
            .and(header("user-agent", "sui-forking-vtest-version"))
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

    fn transaction_response_body(
        tx: &sui_types::full_checkpoint_content::ExecutedTransaction,
        checkpoint: u64,
    ) -> serde_json::Value {
        let signatures: Vec<_> = tx
            .signatures
            .iter()
            .map(|sig| {
                json!({
                    "signatureBytes": FastCryptoBase64::from_bytes(sig.as_ref()).encoded(),
                })
            })
            .collect();
        json!({
            "data": {
                "transaction": {
                    "transactionBcs": FastCryptoBase64::from_bytes(
                        &bcs::to_bytes(&tx.transaction).expect("transaction data should serialize"),
                    )
                    .encoded(),
                    "signatures": signatures,
                    "effects": {
                        "checkpoint": { "sequenceNumber": checkpoint },
                        "effectsBcs": FastCryptoBase64::from_bytes(
                            &bcs::to_bytes(&tx.effects).expect("effects should serialize"),
                        )
                        .encoded(),
                    },
                }
            }
        })
    }

    #[tokio::test]
    async fn test_get_transaction_data_and_effects() {
        let server = MockServer::start().await;
        let checkpoint = TestCheckpointBuilder::new(1)
            .start_transaction(0)
            .finish_transaction()
            .build_checkpoint();
        let executed = checkpoint
            .transactions
            .into_iter()
            .next()
            .expect("checkpoint should have one transaction");
        let digest = sui_types::transaction::Transaction::from_generic_sig_data(
            executed.transaction.clone(),
            executed.signatures.clone(),
        )
        .digest()
        .base58_encode();

        Mock::given(method("POST"))
            .and(path("/"))
            .and(header("user-agent", "sui-forking-vtest-version"))
            .and(body_partial_json(json!({
                "variables": {
                    "digest": digest,
                }
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(transaction_response_body(&executed, 42)),
            )
            .mount(&server)
            .await;

        let store = mock_store(&server);
        let info = store
            .transaction_data_and_effects(&digest)
            .expect("transaction query should succeed")
            .expect("transaction should be present");

        assert_eq!(info.transaction.digest().base58_encode(), digest);
        assert_eq!(info.effects, executed.effects);
        assert_eq!(info.checkpoint, 42);

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
        assert!(query.contains("transactionBcs"));
        assert!(query.contains("signatures"));
        assert!(query.contains("signatureBytes"));
        assert!(query.contains("effectsBcs"));
    }

    #[tokio::test]
    async fn test_get_transaction_returns_none() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": { "transaction": null }
            })))
            .mount(&server)
            .await;

        let store = mock_store(&server);
        let info = store
            .transaction_data_and_effects("missing-digest")
            .expect("transaction query should succeed");
        assert!(info.is_none());
    }
}
