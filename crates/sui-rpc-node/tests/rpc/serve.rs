// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Exercises `sui_rpc_node::start_serve`: the gRPC server comes up
//! over a freshly opened database with no ingestion source and no
//! indexer, and answers a (data-independent) request.

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::time::Duration;

use prometheus::Registry;
use sui_consistent_store::ChainId;
use sui_consistent_store::Db;
use sui_consistent_store::DbOptions;
use sui_consistent_store::FrameworkSchema;
use sui_consistent_store::PipelineTaskKey;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::ServiceConfigRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_client::ConsistentServiceClient;
use sui_rpc_node::config::ServiceConfig;
use sui_rpc_node::start_serve;
use sui_rpc_store::RpcStoreSchema;
use tonic::transport::Channel;

#[tokio::test]
async fn serve_opens_db_and_serves_without_an_indexer() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("db");

    // Seed a chain id so the database looks like one a restore left
    // behind (the rpc-api reads the chain id eagerly when the server
    // starts). Open in its own scope so the handle drops and releases
    // the lock before `start_serve` reopens the database.
    {
        let (db, _schema) =
            Db::open::<RpcStoreSchema>(&db_path, DbOptions::default()).expect("seed open");
        let framework = FrameworkSchema::new(db.clone());
        let mut batch = db.batch();
        batch
            .put(
                &framework.chain_ids,
                &PipelineTaskKey::new("seed"),
                &ChainId([1u8; 32]),
            )
            .expect("stage chain id");
        batch.commit().expect("commit chain id");
    }

    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

    let config = ServiceConfig::for_test(addr);
    let registry = Registry::new();

    // No `ClientArgs`, no ingestion source, no indexer: `start_serve`
    // opens the database and mounts only the RPC server. Holding the
    // returned `Service` keeps the server task alive for the test.
    let _service = start_serve(&db_path, "sui-rpc-node-tests", "0.0", config, &registry)
        .await
        .expect("serve-only startup should succeed without an ingestion source");

    // The server binds asynchronously, so retry until it accepts.
    let url = format!("http://{addr}");
    let mut client: ConsistentServiceClient<Channel> = {
        let mut attempt = 0;
        loop {
            match ConsistentServiceClient::connect(url.clone()).await {
                Ok(c) => break c,
                Err(_) if attempt < 50 => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(e) => panic!("could not connect to the serve-only RPC server: {e}"),
            }
        }
    };

    // `service_config` reads no chain data, so it answers even over a
    // freshly opened database with nothing indexed.
    let response = client
        .service_config(ServiceConfigRequest::default())
        .await
        .expect("serve-only RPC must answer service_config")
        .into_inner();
    assert_eq!(response.default_page_size, Some(50));
    assert_eq!(response.max_batch_size, Some(200));
    assert_eq!(response.max_page_size, Some(200));
}
