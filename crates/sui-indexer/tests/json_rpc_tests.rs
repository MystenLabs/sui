// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use sui_json_rpc_api::{CoinReadApiClient, IndexerApiClient, ReadApiClient};
use sui_json_rpc_types::{
    CoinPage, EventFilter, SuiObjectDataOptions, SuiObjectResponse, SuiObjectResponseQuery,
};
use sui_swarm_config::genesis_config::DEFAULT_GAS_AMOUNT;
use sui_test_transaction_builder::publish_package;
use sui_types::{event::EventID, transaction::CallArg};
use test_cluster::TestClusterBuilder;

#[tokio::test]
async fn test_get_owned_objects() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new()
        .with_indexer_backed_rpc()
        .build()
        .await;

    let http_client = cluster.rpc_client();
    let address = cluster.get_address_0();

    let data_option = SuiObjectDataOptions::new().with_owner();
    let objects = http_client
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                data_option.clone(),
            )),
            None,
            None,
        )
        .await?
        .data;
    let fullnode_objects = cluster
        .fullnode_handle
        .rpc_client
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                data_option.clone(),
            )),
            None,
            None,
        )
        .await?
        .data;
    assert_eq!(5, objects.len());
    // TODO: right now we compare the results from indexer and fullnode, but as we deprecate fullnode rpc,
    // we should change this to compare the results with the object id/digest from genesis potentially.
    assert_eq!(objects, fullnode_objects);

    for obj in &objects {
        let oref = obj.clone().into_object().unwrap();
        let result = http_client
            .get_object(oref.object_id, Some(data_option.clone()))
            .await?;
        assert!(
            matches!(result, SuiObjectResponse { data: Some(object), .. } if oref.object_id == object.object_id && object.owner.clone().unwrap().get_owner_address()? == address)
        );
    }

    // Multiget objectIDs test
    let object_ids: Vec<_> = objects
        .iter()
        .map(|o| o.object().unwrap().object_id)
        .collect();

    let object_resp = http_client
        .multi_get_objects(object_ids.clone(), None)
        .await?;
    let fullnode_object_resp = cluster
        .fullnode_handle
        .rpc_client
        .multi_get_objects(object_ids, None)
        .await?;
    assert_eq!(5, object_resp.len());
    // TODO: right now we compare the results from indexer and fullnode, but as we deprecate fullnode rpc,
    // we should change this to compare the results with the object id/digest from genesis potentially.
    assert_eq!(object_resp, fullnode_object_resp);
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

#[tokio::test]
async fn test_events() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new()
        .with_indexer_backed_rpc()
        .build()
        .await;

    // publish package
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    let move_package = publish_package(&cluster.wallet, path).await.0;

    // execute a transaction to generate events
    let function = "emit_3";
    let arguments = vec![CallArg::Pure(bcs::to_bytes(&5u64).unwrap())];
    let transaction = cluster
        .test_transaction_builder()
        .await
        .move_call(move_package, "events_queries", function, arguments)
        .build();
    let signed_transaction = cluster.wallet.sign_transaction(&transaction);
    cluster.execute_transaction(signed_transaction).await;

    // query for events
    let http_client = cluster.rpc_client();

    // start with ascending order
    let event_filter = EventFilter::All([]);
    let mut cursor: Option<EventID> = None;
    let mut limit = None;
    let mut descending_order = Some(false);
    let result = http_client
        .query_events(event_filter.clone(), cursor, limit, descending_order)
        .await?;
    assert_eq!(3, result.data.len());
    assert!(!result.has_next_page);
    let forward_paginated_events = result.data;

    // Fetch the initial event
    limit = Some(1);
    let result = http_client
        .query_events(event_filter.clone(), cursor, limit, descending_order)
        .await?;
    assert_eq!(1, result.data.len());
    assert!(result.has_next_page);
    assert_eq!(forward_paginated_events[0], result.data[0]);

    // Fetch remaining events
    cursor = result.next_cursor;
    limit = None;
    let result = http_client
        .query_events(event_filter.clone(), cursor, limit, descending_order)
        .await?;
    assert_eq!(2, result.data.len());
    assert_eq!(forward_paginated_events[1..], result.data[..]);

    // now descending order - make sure to reset parameters
    cursor = None;
    descending_order = Some(true);
    limit = None;
    let result = http_client
        .query_events(event_filter.clone(), cursor, limit, descending_order)
        .await?;
    assert_eq!(3, result.data.len());
    assert!(!result.has_next_page);
    let backward_paginated_events = result.data;

    // Fetch the initial event
    limit = Some(1);
    let result = http_client
        .query_events(event_filter.clone(), cursor, limit, descending_order)
        .await?;
    assert_eq!(1, result.data.len());
    assert!(result.has_next_page);
    assert_eq!(backward_paginated_events[0], result.data[0]);
    assert_eq!(forward_paginated_events[2], result.data[0]);

    // Fetch remaining events
    cursor = result.next_cursor;
    limit = None;
    let result = http_client
        .query_events(event_filter.clone(), cursor, limit, descending_order)
        .await?;
    assert_eq!(2, result.data.len());
    assert_eq!(backward_paginated_events[1..], result.data[..]);

    // check that the forward and backward paginated events are in reverse order
    assert_eq!(
        forward_paginated_events
            .into_iter()
            .rev()
            .collect::<Vec<_>>(),
        backward_paginated_events
    );

    Ok(())
}

#[tokio::test]
async fn test_event_type_filter() {
    let cluster = TestClusterBuilder::new()
        .with_indexer_backed_rpc()
        .build()
        .await;

    let client = cluster.rpc_client();

    cluster.trigger_reconfiguration().await;

    let result = client.query_events(EventFilter::MoveEventType("0x0000000000000000000000000000000000000000000000000000000000000003::validator_set::ValidatorEpochInfoEventV2".parse().unwrap()), None, None, None).await;
    assert!(result.is_ok());
    assert!(!result.unwrap().data.is_empty());
    let result = client
        .query_events(
            EventFilter::MoveEventType(
                "0x3::validator_set::ValidatorEpochInfoEventV2"
                    .parse()
                    .unwrap(),
            ),
            None,
            None,
            None,
        )
        .await;
    assert!(result.is_ok());
    assert!(!result.unwrap().data.is_empty());
    let result = client
        .query_events(
            EventFilter::MoveEventType(
                "0x0003::validator_set::ValidatorEpochInfoEventV2"
                    .parse()
                    .unwrap(),
            ),
            None,
            None,
            None,
        )
        .await;
    assert!(result.is_ok());
    assert!(!result.unwrap().data.is_empty());
    let result = client
        .query_events(
            EventFilter::MoveEventType(
                "0x1::validator_set::ValidatorEpochInfoEventV2"
                    .parse()
                    .unwrap(),
            ),
            None,
            None,
            None,
        )
        .await;
    assert!(result.is_ok());
    assert!(result.unwrap().data.is_empty());
}
