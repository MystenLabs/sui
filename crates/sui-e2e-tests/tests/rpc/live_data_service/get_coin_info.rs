// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::proto::rpc::v2alpha::live_data_service_client::LiveDataServiceClient;
use sui_rpc_api::proto::rpc::v2alpha::GetCoinInfoRequest;
use sui_rpc_api::proto::rpc::v2alpha::GetCoinInfoResponse;
use sui_sdk_types::TypeTag;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_coin_info() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let mut grpc_client = LiveDataServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let coin_type_sdk: TypeTag = "0x2::sui::SUI".parse().unwrap();
    let request = GetCoinInfoRequest {
        coin_type: Some(coin_type_sdk.to_string()),
    };

    let GetCoinInfoResponse {
        coin_type,
        metadata,
        treasury,
    } = grpc_client
        .get_coin_info(request)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(coin_type, Some(coin_type_sdk.to_string()));
    assert_eq!(metadata.unwrap().symbol, Some("SUI".to_owned()));
    assert_eq!(
        treasury.unwrap().total_supply,
        Some(sui_types::gas_coin::TOTAL_SUPPLY_MIST)
    );
}
