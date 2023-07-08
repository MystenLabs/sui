// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jsonrpsee::core::client::{Subscription, SubscriptionClientT};
use jsonrpsee::rpc_params;
use sui_test_transaction_builder::{create_devnet_nft, publish_nfts_package};
use tokio::time::timeout;

use sui_core::test_utils::wait_for_tx;
use sui_json_rpc_types::{
    SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI, TransactionFilter,
};
use test_cluster::TestClusterBuilder;

#[tokio::test]
async fn test_subscribe_transaction() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await;

    let address = &cluster.get_address_0();
    let wallet = cluster.wallet;

    let ws_client = cluster.fullnode_handle.ws_client;

    let package_id = publish_nfts_package(&wallet).await.0;

    let mut sub: Subscription<SuiTransactionBlockEffects> = ws_client
        .subscribe(
            "suix_subscribeTransaction",
            rpc_params![TransactionFilter::FromAddress(*address)],
            "suix_unsubscribeTransaction",
        )
        .await
        .unwrap();

    let (_, _, digest) = create_devnet_nft(&wallet, package_id).await;
    wait_for_tx(digest, cluster.fullnode_handle.sui_node.state()).await;

    // Wait for streaming
    let effects = match timeout(Duration::from_secs(5), sub.next()).await {
        Ok(Some(Ok(tx))) => tx,
        _ => panic!("Failed to get tx"),
    };

    assert_eq!(&digest, effects.transaction_digest());
    Ok(())
}
