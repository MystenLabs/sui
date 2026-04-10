// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::time::Duration;

use fastcrypto::encoding::Base58;
use fastcrypto::encoding::Encoding;
use futures::SinkExt;
use prometheus::Registry;
use serde_json::Value;
use serde_json::json;
use sui_futures::service::Service;
use sui_indexer_alt_graphql::RpcArgs as GraphQlArgs;
use sui_indexer_alt_graphql::args::KvArgs as GraphQlKvArgs;
use sui_indexer_alt_graphql::args::SubscriptionArgs;
use sui_indexer_alt_graphql::config::RpcConfig as GraphQlConfig;
use sui_indexer_alt_graphql::start_rpc as start_graphql;
use sui_indexer_alt_reader::consistent_reader::ConsistentReaderArgs;
use sui_indexer_alt_reader::fullnode_client::FullnodeArgs;
use sui_indexer_alt_reader::system_package_task::SystemPackageTaskArgs;
use sui_pg_db::DbArgs;
use sui_pg_db::temp::get_available_port;
use sui_test_transaction_builder::TestTransactionBuilder;
use tokio_stream::StreamExt;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::http::Request;

const SUBSCRIPTION_TIMEOUT: Duration = Duration::from_secs(30);

pub struct SubscriptionTestCluster {
    pub subscription_url: String,
    #[allow(unused)]
    service: Service,
}

impl SubscriptionTestCluster {
    pub async fn new(validator_cluster: &test_cluster::TestCluster) -> Self {
        let graphql_port = get_available_port();
        let graphql_listen_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), graphql_port);
        let rpc_url = validator_cluster.rpc_url();

        let service = start_graphql(
            None,
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
            vec![],
            &Registry::new(),
        )
        .await
        .expect("Failed to start GraphQL server");

        Self {
            subscription_url: format!("ws://{}/graphql", graphql_listen_address),
            service,
        }
    }

    /// Subscribe and return a stream of GraphQL payloads.
    /// Use `tokio_stream::StreamExt` methods (`next`, `take`, `collect`, etc.) to consume.
    pub async fn subscribe(
        &self,
        query: &str,
    ) -> std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Value> + Send>> {
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

        // Use futures::StreamExt::split (tokio_stream doesn't have split).
        let (mut sink, stream) = futures::StreamExt::split(ws);

        sink.send(Message::Text(
            json!({"type": "connection_init"}).to_string().into(),
        ))
        .await
        .expect("Failed to send connection_init");

        // Wrap with tokio_stream timeout, then wait for ack.
        let mut stream = Box::pin(stream.timeout(SUBSCRIPTION_TIMEOUT));

        let ack = stream
            .next()
            .await
            .expect("Stream ended")
            .expect("Timeout waiting for ack")
            .expect("WS error");
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

        // Return a stream that extracts payloads from "next" messages.
        Box::pin(stream.map(|result| {
            let msg = result.expect("Timeout").expect("WS error");
            let text = match msg {
                Message::Text(t) => t,
                other => panic!("Expected text message, got: {other:?}"),
            };
            let msg: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(msg["type"], "next", "Expected 'next' message, got: {msg}");
            msg["payload"].clone()
        }))
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
