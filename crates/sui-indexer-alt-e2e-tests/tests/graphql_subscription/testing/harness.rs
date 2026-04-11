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
use tokio::time::timeout;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::http::Request;

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

    pub async fn subscribe(&self, query: &str) -> SubscriptionStream {
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

pub struct SubscriptionStream {
    stream: futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
}

impl SubscriptionStream {
    pub async fn next_item(&mut self) -> Value {
        let msg = timeout(Duration::from_secs(30), self.stream.next())
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

    pub async fn collect_items(&mut self, n: usize) -> Vec<Value> {
        let mut items = Vec::with_capacity(n);
        for _ in 0..n {
            items.push(self.next_item().await);
        }
        items
    }
}
