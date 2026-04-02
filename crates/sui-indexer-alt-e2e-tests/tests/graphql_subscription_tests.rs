// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::time::Duration;

use futures::SinkExt;
use futures::StreamExt;
use prometheus::Registry;
use serde_json::Value;
use serde_json::json;
use sui_futures::service::Service;
use sui_indexer_alt::config::IndexerConfig;
use sui_indexer_alt::setup_indexer;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs;
use sui_indexer_alt_graphql::RpcArgs as GraphQlArgs;
use sui_indexer_alt_graphql::args::KvArgs as GraphQlKvArgs;
use sui_indexer_alt_graphql::args::SubscriptionArgs;
use sui_indexer_alt_graphql::config::RpcConfig as GraphQlConfig;
use sui_indexer_alt_graphql::start_rpc as start_graphql;
use sui_indexer_alt_reader::consistent_reader::ConsistentReaderArgs;
use sui_indexer_alt_reader::fullnode_client::FullnodeArgs;
use sui_indexer_alt_reader::system_package_task::SystemPackageTaskArgs;
use sui_pg_db::DbArgs;
use sui_pg_db::temp::TempDb;
use sui_pg_db::temp::get_available_port;
use test_cluster::TestClusterBuilder;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::http::Request;
use url::Url;

struct SubscriptionTestCluster {
    subscription_url: String,
    #[allow(unused)]
    service: Service,
    #[allow(unused)]
    database: TempDb,
}

impl SubscriptionTestCluster {
    async fn new(validator_cluster: &test_cluster::TestCluster) -> Self {
        let graphql_port = get_available_port();
        let graphql_listen_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), graphql_port);

        let database = TempDb::new().expect("Failed to create temp database");
        let database_url = database.database().url().clone();

        let rpc_url = validator_cluster.rpc_url();

        let client_args = ClientArgs {
            ingestion: IngestionClientArgs {
                rpc_api_url: Some(Url::parse(rpc_url).expect("Invalid RPC URL")),
                ..Default::default()
            },
            ..Default::default()
        };

        let indexer = setup_indexer(
            database_url.clone(),
            DbArgs::default(),
            IndexerArgs::default(),
            client_args,
            IndexerConfig::for_test(),
            None,
            &Registry::new(),
        )
        .await
        .expect("Failed to setup indexer");

        let pipelines: Vec<String> = indexer.pipelines().map(|s| s.to_string()).collect();
        let s_indexer = indexer.run().await.expect("Failed to start indexer");

        let s_graphql = start_graphql(
            Some(database_url),
            FullnodeArgs {
                fullnode_rpc_url: Some(rpc_url.parse().unwrap()),
            },
            DbArgs::default(),
            GraphQlKvArgs::default(),
            ConsistentReaderArgs::default(),
            GraphQlArgs {
                rpc_listen_address: graphql_listen_address,
                no_ide: true,
            },
            SystemPackageTaskArgs::default(),
            SubscriptionArgs {
                checkpoint_stream_url: Some(rpc_url.parse().unwrap()),
            },
            "0.0.0",
            GraphQlConfig::default(),
            pipelines,
            &Registry::new(),
        )
        .await
        .expect("Failed to start GraphQL server");

        Self {
            subscription_url: format!("ws://{}/graphql", graphql_listen_address),
            service: s_graphql.merge(s_indexer),
            database,
        }
    }

    /// Connect to the subscription endpoint and subscribe with the given query.
    async fn subscribe(&self, query: &str) -> SubscriptionStream {
        let request = Request::builder()
            .uri(&self.subscription_url)
            .header("Sec-WebSocket-Protocol", "graphql-transport-ws")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header("Host", "localhost")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())
            .unwrap();

        let (ws, _) = connect_async(request)
            .await
            .expect("Failed to connect WebSocket");

        let (mut sink, mut stream) = ws.split();

        sink.send(Message::Text(
            json!({"type": "connection_init"}).to_string().into(),
        ))
        .await
        .expect("Failed to send connection_init");

        let ack = stream.next().await.expect("No ack").expect("WS error");
        let ack: Value = serde_json::from_str(ack.to_text().unwrap()).unwrap();
        assert_eq!(ack["type"], "connection_ack");

        sink.send(Message::Text(
            json!({
                "id": "1",
                "type": "subscribe",
                "payload": { "query": query }
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("Failed to send subscribe");

        SubscriptionStream { stream }
    }
}

struct SubscriptionStream {
    stream: futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
}

impl SubscriptionStream {
    /// Read the next subscription payload, with a timeout.
    async fn next_item(&mut self) -> Value {
        let msg = tokio::time::timeout(Duration::from_secs(30), self.stream.next())
            .await
            .expect("Timeout waiting for subscription item")
            .expect("Stream ended")
            .expect("WS error");

        let text = match msg {
            Message::Text(t) => t,
            other => panic!("Expected text message, got: {other:?}"),
        };

        let msg: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(msg["type"], "next", "Expected 'next' message, got: {msg}");
        msg["payload"].clone()
    }

    /// Collect the next `n` subscription payloads.
    async fn collect_items(&mut self, n: usize) -> Vec<Value> {
        let mut items = Vec::with_capacity(n);
        for _ in 0..n {
            items.push(self.next_item().await);
        }
        items
    }
}

#[tokio::test]
async fn test_checkpoint_subscription_sequential() {
    let validator_cluster = TestClusterBuilder::new().build().await;
    let cluster = SubscriptionTestCluster::new(&validator_cluster).await;

    let mut stream = cluster
        .subscribe(
            r#"subscription {
                checkpoints {
                    sequenceNumber
                }
            }"#,
        )
        .await;

    let items = stream.collect_items(3).await;

    let first = items[0]["data"]["checkpoints"]["sequenceNumber"]
        .as_u64()
        .unwrap();
    for (i, item) in items.iter().enumerate() {
        let seq = item["data"]["checkpoints"]["sequenceNumber"]
            .as_u64()
            .unwrap();
        assert_eq!(seq, first + i as u64);
    }
}

#[tokio::test]
async fn test_checkpoint_subscription_fields() {
    let validator_cluster = TestClusterBuilder::new().build().await;
    let cluster = SubscriptionTestCluster::new(&validator_cluster).await;

    let mut stream = cluster
        .subscribe(
            r#"subscription {
                checkpoints {
                    sequenceNumber
                    digest
                    contentDigest
                    timestamp
                    networkTotalTransactions
                    rollingGasSummary {
                        computationCost
                        storageCost
                        storageRebate
                        nonRefundableStorageFee
                    }
                    epoch {
                        epochId
                    }
                    validatorSignatures {
                        signature
                        signersMap
                    }
                }
            }"#,
        )
        .await;

    let item = stream.next_item().await;

    insta::assert_json_snapshot!("checkpoint_subscription_fields", item, {
        ".data.checkpoints.sequenceNumber" => "[seq]",
        ".data.checkpoints.digest" => "[digest]",
        ".data.checkpoints.contentDigest" => "[digest]",
        ".data.checkpoints.timestamp" => "[timestamp]",
        ".data.checkpoints.networkTotalTransactions" => "[total_txns]",
        ".data.checkpoints.rollingGasSummary.computationCost" => "[cost]",
        ".data.checkpoints.rollingGasSummary.storageCost" => "[cost]",
        ".data.checkpoints.rollingGasSummary.storageRebate" => "[cost]",
        ".data.checkpoints.rollingGasSummary.nonRefundableStorageFee" => "[cost]",
        ".data.checkpoints.validatorSignatures.signature" => "[signature]",
        ".data.checkpoints.validatorSignatures.signersMap" => "[signers_map]",
    });
}
