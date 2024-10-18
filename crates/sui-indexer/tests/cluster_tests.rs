// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
#[cfg(not(msim))]
use std::str::FromStr;
use std::time::Duration;
use sui_json::{call_args, type_args};
use sui_json_rpc_api::{
    CoinReadApiClient, GovernanceReadApiClient, IndexerApiClient, ReadApiClient,
    TransactionBuilderClient, WriteApiClient,
};
use sui_json_rpc_types::ObjectChange;
use sui_json_rpc_types::ObjectsPage;
use sui_json_rpc_types::{
    Balance, CoinPage, DelegatedStake, StakeStatus, SuiCoinMetadata, SuiExecutionStatus,
    SuiObjectDataOptions, SuiObjectResponse, SuiObjectResponseQuery, SuiTransactionBlockEffectsAPI,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions, TransactionBlockBytes,
};
use sui_macros::sim_test;
use sui_move_build::BuildConfig;
use sui_swarm_config::genesis_config::{DEFAULT_GAS_AMOUNT, DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT};
use sui_types::balance::Supply;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::coin::{TreasuryCap, COIN_MODULE_NAME};
use sui_types::digests::ObjectDigest;
use sui_types::gas_coin::GAS;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::{parse_sui_struct_tag, SUI_FRAMEWORK_ADDRESS};
use test_cluster::TestClusterBuilder;
use tokio::time::sleep;

#[sim_test]
async fn test_get_objects() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().with_indexer_backed_rpc().build().await;

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
