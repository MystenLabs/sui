// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_json_rpc_api::{CoinReadApiClient, IndexerApiClient, ReadApiClient};
use sui_json_rpc_types::{
    CoinPage, SuiObjectDataOptions, SuiObjectResponse, SuiObjectResponseQuery,
};
use sui_swarm_config::genesis_config::DEFAULT_GAS_AMOUNT;
use test_cluster::TestClusterBuilder;

#[tokio::test]
async fn test_get_owned_objects() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new()
        .with_indexer_backed_rpc()
        .build()
        .await;

    let http_client = cluster.rpc_client();
    let address = cluster.get_address_0();

    let objects = http_client
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new(),
            )),
            None,
            None,
        )
        .await?;
    assert_eq!(5, objects.data.len());

    // Multiget objectIDs test
    let object_digests = objects
        .data
        .iter()
        .map(|o| o.object().unwrap().object_id)
        .collect();

    let object_resp = http_client.multi_get_objects(object_digests, None).await?;
    assert_eq!(5, object_resp.len());
    Ok(())
}

#[tokio::test]
async fn test_get_object_info() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new()
        .with_indexer_backed_rpc()
        .build()
        .await;
    let http_client = cluster.rpc_client();
    let address = cluster.get_address_0();
    let objects = http_client
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;

    for obj in objects {
        let oref = obj.into_object().unwrap();
        let result = http_client
            .get_object(
                oref.object_id,
                Some(SuiObjectDataOptions::new().with_owner()),
            )
            .await?;
        assert!(
            matches!(result, SuiObjectResponse { data: Some(object), .. } if oref.object_id == object.object_id && object.owner.unwrap().get_owner_address()? == address)
        );
    }
    Ok(())
}

#[tokio::test]
async fn test_get_coins() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new()
        .with_indexer_backed_rpc()
        .build()
        .await;
    let http_client = cluster.rpc_client();
    let address = cluster.get_address_0();

    let result: CoinPage = http_client.get_coins(address, None, None, None).await?;
    assert_eq!(5, result.data.len());
    assert!(!result.has_next_page);

    // We should get 0 coins for a non-existent coin type.
    let result: CoinPage = http_client
        .get_coins(address, Some("0x2::sui::TestCoin".into()), None, None)
        .await?;
    assert_eq!(0, result.data.len());

    // We should get all the 5 coins for SUI with the right balance.
    let result: CoinPage = http_client
        .get_coins(address, Some("0x2::sui::SUI".into()), None, None)
        .await?;
    assert_eq!(5, result.data.len());
    assert_eq!(result.data[0].balance, DEFAULT_GAS_AMOUNT);
    assert!(!result.has_next_page);

    // When we have more than 3 coins, we should get a next page.
    let result: CoinPage = http_client
        .get_coins(address, Some("0x2::sui::SUI".into()), None, Some(3))
        .await?;
    assert_eq!(3, result.data.len());
    assert!(result.has_next_page);

    // We should get the remaining 2 coins with the next page.
    let result: CoinPage = http_client
        .get_coins(
            address,
            Some("0x2::sui::SUI".into()),
            result.next_cursor,
            Some(3),
        )
        .await?;
    assert_eq!(2, result.data.len(), "{:?}", result);
    assert!(!result.has_next_page);

    // No more coins after the last page.
    let result: CoinPage = http_client
        .get_coins(
            address,
            Some("0x2::sui::SUI".into()),
            result.next_cursor,
            None,
        )
        .await?;
    assert_eq!(0, result.data.len(), "{:?}", result);
    assert!(!result.has_next_page);

    Ok(())
}
