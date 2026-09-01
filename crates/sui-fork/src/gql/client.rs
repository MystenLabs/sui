// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use anyhow::Error;
use anyhow::Result;
use cynic::GraphQlResponse;
use cynic::Operation;
use reqwest::header::USER_AGENT;

use sui_protocol_config::Chain;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEvents;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;

use crate::Node;
use crate::gql::AddressOwnedObject;
use crate::gql::CheckpointRead;
use crate::gql::ObjectKey;
use crate::gql::ObjectRead;
use crate::gql::ObjectSeedMetadata;
use crate::gql::TransactionInfo;
use crate::gql::TransactionRead;
use crate::gql::queries;

/// Worker threads for [`gql_runtime`]. GraphQL calls are I/O-bound and issued one at a time from a
/// blocking caller, so this only has to cover hyper's connection dispatch tasks running
/// concurrently with the request being awaited.
const GQL_RUNTIME_WORKER_THREADS: usize = 2;

/// The runtime every GraphQL request runs on, for the life of the process.
///
/// The storage traits this crate implements are synchronous, so each GraphQL call has to block
/// somewhere. Building a runtime per call, the obvious way to do that, silently corrupts connection
/// reuse, because hyper spawns a per-connection dispatch task onto whichever runtime is current
/// when the connection opens, so dropping that runtime kills the task while the connection itself
/// stays in the shared [`reqwest::Client`]'s idle pool. The next call to draw that connection fails
/// with `dispatch task is gone: runtime dropped the dispatch task`, which surfaces during execution
/// as a `STORAGE_ERROR` and an invariant violation. It is intermittent by construction, because it
/// depends on the pool handing back a connection whose runtime has died.
///
/// A single process-lifetime runtime keeps those dispatch tasks alive as long as the connections
/// they serve. It is parked on its own thread and deliberately never dropped, because dropping a
/// runtime from inside an async context panics, which is what would happen if the last handle were
/// released on a worker thread.
fn gql_runtime() -> &'static tokio::runtime::Handle {
    static HANDLE: std::sync::OnceLock<tokio::runtime::Handle> = std::sync::OnceLock::new();
    HANDLE.get_or_init(|| {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(GQL_RUNTIME_WORKER_THREADS)
            .thread_name("sui-fork-gql")
            .enable_all()
            .build()
            .expect("failed to build the GraphQL runtime");
        let handle = runtime.handle().clone();
        std::thread::Builder::new()
            .name("sui-fork-gql-runtime".to_owned())
            .spawn(move || {
                let _runtime = runtime;
                // `park` may wake spuriously; nothing ever unparks this thread.
                loop {
                    std::thread::park();
                }
            })
            .expect("failed to spawn the GraphQL runtime thread");
        handle
    })
}

macro_rules! block_on {
    ($expr:expr) => {{
        #[allow(clippy::disallowed_methods, clippy::result_large_err)]
        {
            let handle = gql_runtime();
            // `Handle::block_on` panics inside an async context, so a caller that is
            // already on a runtime waits on a scoped thread instead. Either way the
            // future itself runs on the shared runtime, which is the point.
            if tokio::runtime::Handle::try_current().is_ok() {
                std::thread::scope(|scope| {
                    scope
                        .spawn(|| handle.block_on($expr))
                        .join()
                        .expect("GraphQL runtime bridge thread panicked")
                })
            } else {
                handle.block_on($expr)
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
            .header(USER_AGENT, format!("sui-fork-v{}", version))
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
    /// Fetch metadata for objects owned by an address at a checkpoint, paginating through the
    /// checkpoint-scoped ownership connection.
    pub(crate) async fn get_address_owned_objects_at_checkpoint(
        &self,
        address: SuiAddress,
        checkpoint: CheckpointSequenceNumber,
    ) -> Result<Vec<AddressOwnedObject>, Error> {
        queries::address_owned_objects_query::query(address, checkpoint, self).await
    }

    /// Coin types for which `address` holds an accumulator balance at a checkpoint. See
    /// [`queries::address_balances_query`] for why the seed needs to ask.
    pub(crate) async fn get_address_balance_coin_types_at_checkpoint(
        &self,
        address: SuiAddress,
        checkpoint: CheckpointSequenceNumber,
    ) -> Result<Vec<String>, Error> {
        queries::address_balances_query::query(address, checkpoint, self).await
    }

    /// Resolve object references at a checkpoint without an owner check, for ids the fork derived
    /// rather than the user named.
    pub(crate) async fn get_object_refs_at_checkpoint(
        &self,
        object_ids: &[ObjectID],
        checkpoint: CheckpointSequenceNumber,
    ) -> Result<Vec<Option<ObjectRef>>, Error> {
        queries::object_seed_query::query_refs(object_ids, checkpoint, self).await
    }

    /// Fetch lightweight metadata for explicit object seeds at a checkpoint.
    pub(crate) async fn get_object_seed_metadata_at_checkpoint(
        &self,
        object_ids: &[ObjectID],
        checkpoint: CheckpointSequenceNumber,
    ) -> Result<Vec<ObjectSeedMetadata>, Error> {
        queries::object_seed_query::query(object_ids, checkpoint, self).await
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

    /// Fetch all events for a transaction, paginating through the GraphQL events connection.
    /// Returns `None` if the transaction doesn't exist.
    pub(crate) fn get_transaction_events(
        &self,
        tx_digest: &str,
    ) -> Result<Option<TransactionEvents>, Error> {
        block_on!(queries::events_query::query(tx_digest, self))
    }

    /// Query `serviceConfig.availableRange` for both "Checkpoint" and "Transaction" types and
    /// return the max of their `first.sequenceNumber`.
    pub(crate) fn get_lowest_available_checkpoint(
        &self,
    ) -> Result<CheckpointSequenceNumber, Error> {
        let checkpoint_low = block_on!(queries::available_range_query::query("Checkpoint", self))?;
        let transaction_low =
            block_on!(queries::available_range_query::query("Transaction", self))?;
        Ok(checkpoint_low.max(transaction_low))
    }

    /// Query `serviceConfig.availableRange` for "Object" type and return `first.sequenceNumber`.
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
    use itertools::Itertools as _;
    use serde_json::json;
    use sui_types::{base_types::ObjectID, test_checkpoint_data_builder::TestCheckpointBuilder};
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::super::queries::checkpoint_query::{CheckpointArgs, Query as CheckpointQuery};
    use super::*;
    use crate::gql::VersionQuery;

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
                        "multiGetObjects": [object.map(|object| {
                            json!({
                                "address": object.id().to_string(),
                                "version": object.version().value(),
                                "objectBcs": FastCryptoBase64::from_bytes(
                                    &bcs::to_bytes(object).expect("object should serialize"),
                                )
                                .encoded(),
                            })
                        })]
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
            .and(header("user-agent", "sui-fork-vtest-version"))
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
        // Exercises the multiGetObjects batch path, which serves the
        // "standard" key kinds. `VersionAtCheckpoint` keys are routed to a
        // separate checkpoint-scoped query and are covered elsewhere.
        let server = MockServer::start().await;
        let root_version_object = Object::immutable_with_id_for_testing(ObjectID::random());
        let missing_object_id = ObjectID::random();

        Mock::given(method("POST"))
            .and(path("/"))
            .and(header("user-agent", "sui-fork-vtest-version"))
            .and(body_partial_json(json!({
                "variables": {
                    "keys": [
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
                ResponseTemplate::new(200)
                    .set_body_json(object_response_body(&[Some(&root_version_object), None])),
            )
            .mount(&server)
            .await;

        let store = mock_store(&server);
        let objects = store
            .get_objects(&[
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
            .and(header("user-agent", "sui-fork-vtest-version"))
            .and(body_partial_json(json!({
                "variables": {
                    "sequenceNumber": 31,
                    "keys": [{
                        "address": object.id().to_string(),
                        "version": object.version().value(),
                    }],
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
        // The pin lives on the query scope, not on the keys: a key carrying both
        // a version and an `atCheckpoint` is rejected as over-bounded, which is
        // what forced these reads to go one-at-a-time before.
        assert!(query.contains("checkpoint(sequenceNumber: $sequenceNumber)"));
        assert!(query.contains("multiGetObjects(keys: $keys)"));
        assert!(!query.contains("atCheckpoint"));
    }

    /// Seeding hydrates every manifest entry through this path, so a regression to one request per
    /// object is the difference between two round trips and seventy. Nothing about the returned
    /// values would reveal it — only the request count does.
    #[tokio::test]
    async fn test_exact_versions_at_one_checkpoint_batch_into_a_single_request() {
        let server = MockServer::start().await;
        let objects: Vec<_> = (0..3)
            .map(|_| Object::immutable_with_id_for_testing(ObjectID::random()))
            .collect();

        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_partial_json(
                json!({ "variables": { "sequenceNumber": 31 } }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {
                    "checkpoint": {
                        "query": {
                            "multiGetObjects": objects
                                .iter()
                                .map(|object| json!({
                                    "address": object.id().to_string(),
                                    "version": object.version().value(),
                                    "objectBcs": FastCryptoBase64::from_bytes(
                                        &bcs::to_bytes(object).expect("object should serialize"),
                                    )
                                    .encoded(),
                                }))
                                .collect::<Vec<_>>(),
                        }
                    }
                }
            })))
            .mount(&server)
            .await;

        let store = mock_store(&server);
        let keys: Vec<_> = objects
            .iter()
            .map(|object| ObjectKey {
                object_id: object.id(),
                version_query: VersionQuery::VersionAtCheckpoint {
                    version: object.version().value(),
                    checkpoint: 31,
                },
            })
            .collect();

        let fetched = store
            .get_objects(&keys)
            .expect("batched versioned query should succeed");
        assert_eq!(fetched.len(), 3);
        for (object, result) in objects.iter().zip_eq(fetched) {
            assert_eq!(result, Some((object.clone(), object.version().value())));
        }

        let requests = server
            .received_requests()
            .await
            .expect("wiremock should record requests");
        assert_eq!(
            requests.len(),
            1,
            "three exact versions at one checkpoint must cost one request, not three",
        );
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
                    "keys": [{
                        "address": object_id.to_string(),
                        "version": 7,
                    }],
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
            .and(header("user-agent", "sui-fork-vtest-version"))
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
