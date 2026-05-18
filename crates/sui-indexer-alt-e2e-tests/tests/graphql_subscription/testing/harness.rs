// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::time::Duration;

use async_stream::stream;
use bytes::BytesMut;
use fastcrypto::encoding::Base58;
use fastcrypto::encoding::Encoding;
use prometheus::Registry;
use serde_json::Value;
use serde_json::json;
use sui_futures::service::Service;
use sui_indexer_alt_graphql::RpcArgs as GraphQlArgs;
use sui_indexer_alt_graphql::args::SubscriptionArgs;
use sui_indexer_alt_graphql::config::RpcConfig as GraphQlConfig;
use sui_indexer_alt_graphql::start_rpc as start_graphql;
use sui_indexer_alt_reader::consistent_reader::ConsistentReaderArgs;
use sui_indexer_alt_reader::fullnode_client::FullnodeArgs;
use sui_indexer_alt_reader::kv_loader::KvArgs;
use sui_indexer_alt_reader::system_package_task::SystemPackageTaskArgs;
use sui_pg_db::DbArgs;
use sui_pg_db::temp::TempDb;
use sui_pg_db::temp::get_available_port;
use sui_test_transaction_builder::TestTransactionBuilder;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use tokio_stream::StreamExt;

use super::proxy;
use super::proxy::ProxyController;

const SUBSCRIPTION_TIMEOUT: Duration = Duration::from_secs(30);

pub struct SubscriptionTestCluster {
    pub validator: TestCluster,
    #[allow(unused)]
    pub db: TempDb,
    pub subscription_url: String,
    #[allow(unused)]
    service: Service,
    #[allow(unused)]
    indexer: Service,
    #[allow(unused)]
    ingestion_dir: tempfile::TempDir,
}

impl SubscriptionTestCluster {
    /// Set up a full streaming subscription test environment:
    /// validator + postgres DB + kv_packages indexer + GraphQL service.
    /// Waits for kv_packages to index the genesis checkpoint so subscriptions are ready.
    pub async fn new() -> Self {
        let (cluster, _controller) = Self::new_inner(false).await;
        cluster
    }

    /// Same as `new()`, but inserts a TCP proxy between graphql's streaming
    /// connection and the validator's gRPC. Returns a controller that tests
    /// can use to forcibly disconnect the streaming connection mid-test,
    /// exercising graphql's reconnect + gap-recovery code path.
    ///
    /// Only the streaming connection runs through the proxy. `ledger_grpc_url`
    /// (used by gap recovery) still points at the validator directly, so
    /// `disconnect_all()` only severs the stream and leaves recovery reads
    /// untouched.
    pub async fn new_with_disruption_proxy() -> (Self, ProxyController) {
        Self::new_inner(true).await
    }

    async fn new_inner(use_proxy: bool) -> (Self, ProxyController) {
        let ingestion_dir = tempfile::tempdir().expect("Failed to create ingestion dir");
        let validator = TestClusterBuilder::new()
            .with_num_validators(1)
            .with_data_ingestion_dir(ingestion_dir.path().to_owned())
            .build()
            .await;

        let db = TempDb::new().expect("Failed to create TempDb");
        let database_url = db.database().url().clone();
        let writer = sui_pg_db::Db::for_write(database_url.clone(), DbArgs::default())
            .await
            .expect("Failed to connect writer");
        writer
            .run_migrations(None)
            .await
            .expect("Failed to run migrations");

        let indexer = sui_indexer_alt::setup_indexer(
            database_url.clone(),
            DbArgs::default(),
            sui_indexer_alt_framework::IndexerArgs::default(),
            sui_indexer_alt_framework::ingestion::ClientArgs {
                ingestion:
                    sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs {
                        local_ingestion_path: Some(ingestion_dir.path().to_owned()),
                        ..Default::default()
                    },
                ..Default::default()
            },
            sui_indexer_alt::config::IndexerConfig::for_test(),
            None,
            &Registry::new(),
        )
        .await
        .expect("Failed to create indexer");
        let indexer = indexer.run().await.expect("Failed to start indexer");

        wait_for_kv_packages(&db, 0).await;

        let graphql_port = get_available_port();
        let graphql_listen_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), graphql_port);
        let rpc_url = validator.rpc_url();

        let (stream_url, controller): (String, ProxyController) = if use_proxy {
            proxy::start(rpc_url).await
        } else {
            (rpc_url.to_string(), ProxyController::default())
        };

        // The validator's gRPC v2 endpoint serves both `SubscriptionService` and
        // `LedgerService`, so we point `ledger_grpc_url` at the same URL graphql streams
        // from. Required because gap recovery makes `ledger_grpc_reader` mandatory when
        // streaming is enabled. Note: `ledger_grpc_url` always targets the validator
        // directly so `disconnect_all()` cannot interfere with gap-recovery reads.
        let kv_args = KvArgs {
            ledger_grpc_url: Some(rpc_url.parse().unwrap()),
            ..Default::default()
        };

        let service = start_graphql(
            Some(database_url),
            FullnodeArgs::new(rpc_url.parse().unwrap()),
            DbArgs::default(),
            kv_args,
            ConsistentReaderArgs::default(),
            GraphQlArgs {
                rpc_listen_address: graphql_listen_address,
                no_ide: true,
            },
            SystemPackageTaskArgs::default(),
            SubscriptionArgs {
                checkpoint_stream_url: Some(stream_url.parse().unwrap()),
            },
            "0.0.0",
            GraphQlConfig::default(),
            vec!["kv_packages".to_string()],
            &Registry::new(),
        )
        .await
        .expect("Failed to start GraphQL server");

        (
            Self {
                validator,
                db,
                subscription_url: format!(
                    "http://{}/graphql/subscriptions",
                    graphql_listen_address
                ),
                service,
                indexer,
                ingestion_dir,
            },
            controller,
        )
    }

    /// Latest checkpoint sequence number produced by the validator (the
    /// on-chain tip, not whatever graphql has currently streamed).
    pub fn validator_checkpoint_tip(&self) -> u64 {
        self.validator
            .fullnode_handle
            .sui_node
            .state()
            .get_latest_checkpoint_sequence_number()
            .expect("Failed to read validator checkpoint tip")
    }

    /// Subscribe and return a stream of GraphQL payloads.
    /// Use `tokio_stream::StreamExt` methods (`next`, `take`, `collect`, etc.) to consume.
    /// Optionally pass GraphQL variables (e.g. `json!({"sender": "0x..."})`).
    pub async fn subscribe(
        &self,
        query: &str,
    ) -> std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Value> + Send>> {
        self.subscribe_with_variables(query, None).await
    }

    pub async fn subscribe_with_variables(
        &self,
        query: &str,
        variables: Option<Value>,
    ) -> std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Value> + Send>> {
        let mut payload = json!({ "query": query });
        if let Some(vars) = variables {
            payload["variables"] = vars;
        }

        let response = reqwest::Client::new()
            .post(&self.subscription_url)
            .header("Accept", "text/event-stream")
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .expect("Failed to POST subscription request");

        assert!(
            response.status().is_success(),
            "Subscription request failed: {}",
            response.status(),
        );

        Box::pin(
            parse_sse_events(response.bytes_stream())
                .timeout(SUBSCRIPTION_TIMEOUT)
                .map(|result| result.expect("Timed out waiting for SSE event")),
        )
    }
}

/// Parse a graphql-sse byte stream into a stream of GraphQL response payloads.
///
/// Reads `event: next` frames, parses their `data:` field as JSON, and yields each one.
/// Stops when the server sends `event: complete` or closes the connection.
fn parse_sse_events(
    body: impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> impl tokio_stream::Stream<Item = Value> + Send + 'static {
    stream! {
        let mut body = Box::pin(body);
        let mut buffer = BytesMut::new();
        let mut current_event: Option<String> = None;
        let mut current_data = String::new();

        while let Some(chunk) = futures::StreamExt::next(&mut body).await {
            let chunk = chunk.expect("SSE body error");
            buffer.extend_from_slice(&chunk);

            while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                let line_bytes = buffer.split_to(pos + 1);
                let line = std::str::from_utf8(&line_bytes[..line_bytes.len() - 1])
                    .expect("Invalid UTF-8 in SSE stream")
                    .trim_end_matches('\r');

                if line.is_empty() {
                    if current_event.as_deref() == Some("complete") {
                        return;
                    }
                    if current_event.as_deref() == Some("next") && !current_data.is_empty() {
                        let value: Value = serde_json::from_str(&current_data)
                            .expect("Invalid JSON in SSE data field");
                        yield value;
                    }
                    current_event = None;
                    current_data.clear();
                } else if let Some(rest) = line.strip_prefix("event:") {
                    current_event = Some(rest.trim().to_string());
                } else if let Some(rest) = line.strip_prefix("data:") {
                    if !current_data.is_empty() {
                        current_data.push('\n');
                    }
                    current_data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
                }
            }
        }
    }
}

/// Execute SUI transfers as a soft bundle and return Base58-encoded digests.
pub async fn transfer_coins(
    cluster: &mut test_cluster::TestCluster,
    amounts: &[u64],
) -> Vec<String> {
    let sender = cluster.wallet.active_address().unwrap();
    let recipient = sui_types::base_types::SuiAddress::ZERO;
    let mut excluded = BTreeSet::new();
    let mut txns = Vec::with_capacity(amounts.len());

    for &amount in amounts {
        let gas = cluster
            .wallet
            .gas_for_owner_budget(sender, 5000, excluded.clone())
            .await
            .unwrap()
            .1
            .compute_object_reference();
        excluded.insert(gas.0);
        txns.push(
            TestTransactionBuilder::new(sender, gas, 1000)
                .transfer_sui(Some(amount), recipient)
                .build(),
        );
    }

    cluster
        .sign_and_execute_txns_in_soft_bundle(&txns)
        .await
        .unwrap()
        .into_iter()
        .map(|(digest, _)| Base58::encode(digest))
        .collect()
}

/// Wait for a stream item where `find_digests` extracts digests and any match the expected ones.
pub async fn wait_for_matching_item(
    stream: &mut (impl tokio_stream::Stream<Item = Value> + Unpin),
    digests: &[String],
    find_digests: impl Fn(&Value) -> Vec<&str>,
) -> Value {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let item = stream.next().await.expect("Stream ended");
            let found = find_digests(&item);
            if found
                .iter()
                .any(|d| digests.iter().any(|expected| expected == d))
            {
                return item;
            }
        }
    })
    .await
    .expect("Timed out waiting for matching item")
}

/// Extract digests from a checkpoint subscription response.
/// Path: data.checkpoints.transactions.nodes[].digest
pub fn checkpoint_tx_digests(item: &Value) -> Vec<&str> {
    item["data"]["checkpoints"]["transactions"]["nodes"]
        .as_array()
        .map(|nodes| nodes.iter().filter_map(|n| n["digest"].as_str()).collect())
        .unwrap_or_default()
}

/// Extract `sequenceNumber` from a top-level checkpoint subscription response.
/// Path: data.checkpoints.sequenceNumber
pub fn checkpoint_seq(item: &Value) -> u64 {
    item["data"]["checkpoints"]["sequenceNumber"]
        .as_u64()
        .expect("checkpoint payload missing sequenceNumber")
}

/// Read items from `stream` until it goes quiet for one second, returning every item
/// observed in order. Returns an empty `Vec` when the stream stalls before delivering
/// anything. Used to assert silence (`drained.is_empty()`) or to recover the last
/// observed value via a domain-specific extractor (`drained.last().map(...)`).
pub async fn drain_until_stalled(
    stream: &mut (impl tokio_stream::Stream<Item = Value> + Unpin),
) -> Vec<Value> {
    let mut drained = Vec::new();
    loop {
        match tokio::time::timeout(Duration::from_secs(1), stream.next()).await {
            Ok(Some(item)) => drained.push(item),
            Ok(None) => panic!("stream ended unexpectedly"),
            Err(_) => return drained,
        }
    }
}

/// Common snapshot redactions for GraphQL subscription tests.
///
/// Redacts seed-dependent fields (digests, addresses, versions, timestamps, etc.)
/// and sorts object changes by type name for stable ordering across sim_test seeds.
///
/// Usage:
/// ```ignore
/// graphql_redactions().bind(|| {
///     insta::assert_json_snapshot!("my_snapshot", data);
/// });
/// ```
pub fn graphql_redactions() -> insta::Settings {
    let mut settings = insta::Settings::clone_current();
    settings.add_redaction(".**.sequenceNumber", "[seq]");
    settings.add_redaction(".**.digest", "[digest]");
    settings.add_redaction(".**.contentDigest", "[contentDigest]");
    settings.add_redaction(".**.address", "[address]");
    settings.add_redaction(".**.version", "[version]");
    settings.add_redaction(".**.timestamp", "[timestamp]");
    settings.add_redaction(".**.networkTotalTransactions", "[networkTotalTransactions]");
    settings.add_redaction(".**.cursor", "[cursor]");
    settings.add_redaction(".**.signature", "[signature]");
    settings.add_dynamic_redaction(".**.repr", |value, _path| {
        let s = value.as_str().unwrap();
        if let Some(idx) = s.find("::") {
            insta::internals::Content::from(format!("[pkg]{}", &s[idx..]))
        } else {
            insta::internals::Content::from(s.to_string())
        }
    });
    settings.add_dynamic_redaction(".**.objectChanges.nodes", |mut value, _path| {
        // Sort object changes by type name (after ::) for stable ordering across seeds.
        if let insta::internals::Content::Seq(ref mut items) = value {
            items.sort_by_key(|item| {
                let s = format!("{:?}", item);
                s.find("::").map(|i| s[i..].to_string()).unwrap_or(s)
            });
        }
        value
    });
    settings
}

/// Extract digest from a top-level transaction subscription response.
/// Path: data.transactions.digest
pub fn transaction_digest(item: &Value) -> Vec<&str> {
    item["data"]["transactions"]["digest"]
        .as_str()
        .into_iter()
        .collect()
}

/// Poll the kv_packages watermark until it reaches `target_checkpoint`.
pub async fn wait_for_kv_packages(db: &sui_pg_db::temp::TempDb, target_checkpoint: u64) {
    use diesel::ExpressionMethods;
    use diesel::QueryDsl;
    use sui_indexer_alt_schema::schema::watermarks::dsl as w;

    let reader = sui_indexer_alt_reader::pg_reader::PgReader::new(
        Some("wait_for_kv_packages"),
        Some(db.database().url().clone()),
        DbArgs::default(),
        &Registry::new(),
    )
    .await
    .expect("Failed to create PgReader");

    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(mut conn) = reader.connect().await
                && let Ok(hi) = conn
                    .results(
                        w::watermarks
                            .select(w::checkpoint_hi_inclusive)
                            .filter(w::pipeline.eq("kv_packages")),
                    )
                    .await
                && hi
                    .first()
                    .is_some_and(|&cp: &i64| cp as u64 >= target_checkpoint)
            {
                return;
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Timed out waiting for kv_packages indexer");
}
