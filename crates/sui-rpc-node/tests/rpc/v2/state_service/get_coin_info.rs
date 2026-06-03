// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Ports the `get_coin_info_sui` test from
//! `sui-e2e-tests/tests/rpc/v2/state_service/get_coin_info.rs`.
//! The other tests in that file publish custom coin packages
//! through `test_cluster::TestCluster::publish_package`; porting
//! those needs an in-process Move build setup we don't have here
//! yet.

use sui_rpc::proto::sui::rpc::v2::GetCoinInfoRequest;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoResponse;
use sui_rpc::proto::sui::rpc::v2::coin_treasury::SupplyState;
use sui_rpc::proto::sui::rpc::v2::regulated_coin_metadata::CoinRegulatedState;
use sui_rpc::proto::sui::rpc::v2::state_service_client::StateServiceClient;
use sui_types::TypeTag;
use sui_types::base_types::ObjectID;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

#[tokio::test]
async fn get_coin_info_sui() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut client: StateServiceClient<Channel> =
        StateServiceClient::connect(cluster.grpc_url().to_string())
            .await
            .unwrap();

    let coin_type_sdk: TypeTag = "0x2::sui::SUI".parse().unwrap();
    let mut request = GetCoinInfoRequest::default();
    request.coin_type = Some(coin_type_sdk.to_string());

    let GetCoinInfoResponse {
        coin_type,
        metadata,
        treasury,
        regulated_metadata,
        ..
    } = client.get_coin_info(request).await.unwrap().into_inner();

    let expected_type = coin_type_sdk.to_canonical_string(true);
    assert_eq!(coin_type, Some(expected_type));

    let metadata = metadata.unwrap();
    let metadata_object_id = metadata.id.as_ref().unwrap();
    assert!(metadata_object_id.parse::<ObjectID>().is_ok());
    assert_eq!(metadata.decimals, Some(9));
    assert_eq!(metadata.symbol.as_deref(), Some("SUI"));
    assert_eq!(metadata.name.as_deref(), Some("Sui"));
    assert_eq!(metadata.description.as_deref(), Some(""));
    assert!(metadata.icon_url.is_none());
    assert!(metadata.metadata_cap_state.is_none());

    let treasury = treasury.unwrap();
    assert!(treasury.id.is_none());
    assert_eq!(
        treasury.total_supply,
        Some(sui_types::gas_coin::TOTAL_SUPPLY_MIST),
    );
    assert_eq!(
        treasury.supply_state,
        Some(SupplyState::Fixed as i32),
        "SUI should have Fixed supply state",
    );

    let regulated_metadata = regulated_metadata.unwrap();
    assert_eq!(
        regulated_metadata.coin_regulated_state,
        Some(CoinRegulatedState::Unregulated as i32),
    );
}
