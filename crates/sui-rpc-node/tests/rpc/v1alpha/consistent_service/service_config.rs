// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::ServiceConfigRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_client::ConsistentServiceClient;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

#[tokio::test]
async fn service_config_returns_pagination_defaults() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut client: ConsistentServiceClient<Channel> =
        ConsistentServiceClient::connect(cluster.grpc_url().to_string())
            .await
            .unwrap();

    let response = client
        .service_config(ServiceConfigRequest::default())
        .await
        .unwrap()
        .into_inner();

    // Defaults from `PaginationConfig::default()`.
    assert_eq!(response.default_page_size, Some(50));
    assert_eq!(response.max_batch_size, Some(200));
    assert_eq!(response.max_page_size, Some(200));
}
