// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use ethers::types::Address as EthAddress;

use anyhow::anyhow;
use diesel::associations::HasTable;
use diesel::QueryDsl;
use diesel_async::RunQueryDsl;
use diesel_migrations::{embed_migrations, EmbeddedMigrations};
use ethers::contract::ContractCall;
use ethers::prelude::U256;
use move_core_types::ident_str;
use prometheus::Registry;
use std::collections::HashMap;
use std::time::Duration;
use sui_bridge::abi::{EthBridgeEvent, EthSuiBridge, EthSuiBridgeEvents};
use sui_bridge::sui_client::SuiBridgeClient;
use sui_bridge::types::BridgeActionStatus;
use sui_bridge::utils::EthSigner;
use sui_bridge_indexer::config::IndexerConfig;
use sui_bridge_indexer::metrics::BridgeIndexerMetrics;
use sui_bridge_indexer::models::{GovernanceAction, TokenTransfer};
use sui_bridge_indexer::postgres_manager::get_connection_pool;
use sui_bridge_indexer::storage::PgBridgePersistent;
use sui_bridge_indexer::{create_sui_indexer, schema};
use sui_bridge_test_utils::test_utils::{
    send_eth_tx_and_get_tx_receipt, BridgeTestCluster, BridgeTestClusterBuilder,
};
use sui_data_ingestion_core::DataIngestionMetrics;
use sui_indexer::database::Connection;
use sui_indexer::tempdb::TempDb;
use sui_indexer_builder::indexer_builder::IndexerProgressStore;
use sui_json_rpc_types::SuiTransactionBlockResponse;
use sui_sdk::wallet_context::WalletContext;
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::bridge::{BridgeChainId, BRIDGE_MODULE_NAME, TOKEN_ID_ETH};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{ObjectArg, TransactionData};
use sui_types::{TypeTag, BRIDGE_PACKAGE_ID};
use tap::TapFallible;
use tracing::info;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("src/migrations");

#[tokio::test]
async fn test_indexing_transfer() {
    let metrics = BridgeIndexerMetrics::new_for_testing();
    let registry = Registry::new();
    let ingestion_metrics = DataIngestionMetrics::new(&registry);

    let (config, cluster, _db) = setup_bridge_env(false).await;

    let pool = get_connection_pool(config.db_url.clone()).await;
    let indexer = create_sui_indexer(pool.clone(), metrics.clone(), ingestion_metrics, &config)
        .await
        .unwrap();
    let storage = indexer.test_only_storage().clone();
    let indexer_name = indexer.test_only_name();
    let indexer_handle = tokio::spawn(indexer.start());

    // wait until backfill finish
    wait_for_back_fill_to_finish(&storage, &indexer_name)
        .await
        .unwrap();

    let data: Vec<TokenTransfer> = schema::token_transfer::dsl::token_transfer::table()
        .load(&mut pool.get().await.unwrap())
        .await
        .unwrap();

    // token transfer data should be empty
    assert!(data.is_empty());

    use schema::governance_actions::columns;
    let data = schema::governance_actions::dsl::governance_actions::table()
        .select((
            columns::nonce,
            columns::data_source,
            columns::txn_digest,
            columns::sender_address,
            columns::timestamp_ms,
            columns::action,
            columns::data,
        ))
        .load::<GovernanceAction>(&mut pool.get().await.unwrap())
        .await
        .unwrap();
    assert_eq!(8, data.len());

    // transfer eth to sui
    initiate_bridge_eth_to_sui(&cluster, 1000, 0).await.unwrap();

    let current_block_height = cluster
        .sui_client()
        .read_api()
        .get_latest_checkpoint_sequence_number()
        .await
        .unwrap();
    wait_for_block(&storage, &indexer_name, current_block_height)
        .await
        .unwrap();

    let data = schema::token_transfer::dsl::token_transfer::table()
        .load::<TokenTransfer>(&mut pool.get().await.unwrap())
        .await
        .unwrap()
        .iter()
        .map(|t| (t.chain_id, t.nonce, t.status.clone()))
        .collect::<Vec<_>>();

    assert_eq!(2, data.len());
    assert_eq!(
        vec![
            (12, 0, "Approved".to_string()),
            (12, 0, "Claimed".to_string())
        ],
        data
    );

    indexer_handle.abort()
}

async fn wait_for_block(
    storage: &PgBridgePersistent,
    task: &str,
    block: u64,
) -> Result<(), anyhow::Error> {
    while storage
        .get_ongoing_tasks(task)
        .await?
        .live_task()
        .map(|t| t.start_checkpoint)
        .unwrap_or_default()
        < block
    {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}

async fn wait_for_back_fill_to_finish(
    storage: &PgBridgePersistent,
    task: &str,
) -> Result<(), anyhow::Error> {
    // wait until tasks are set up
    while storage.get_ongoing_tasks(task).await?.live_task().is_none() {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // wait until all backfill task has completed
    while !storage
        .get_ongoing_tasks(&task)
        .await?
        .backfill_tasks_ordered_desc()
        .is_empty()
    {
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    Ok(())
}

async fn setup_bridge_env(with_eth_env: bool) -> (IndexerConfig, BridgeTestCluster, TempDb) {
    let bridge_test_cluster = BridgeTestClusterBuilder::new()
        .with_eth_env(with_eth_env)
        .with_bridge_cluster(true)
        .with_num_validators(3)
        .build()
        .await;

    let db = TempDb::new().unwrap();

    // Run database migration
    let conn = Connection::dedicated(db.database().url()).await.unwrap();
    conn.run_pending_migrations(MIGRATIONS).await.unwrap();

    let config = IndexerConfig {
        remote_store_url: format!("{}/rest", bridge_test_cluster.sui_rpc_url()),
        checkpoints_path: None,
        sui_rpc_url: bridge_test_cluster.sui_rpc_url(),
        eth_rpc_url: bridge_test_cluster.eth_rpc_url(),
        // TODO: add WS support
        eth_ws_url: "".to_string(),
        db_url: db.database().url().to_string(),
        concurrency: 10,
        sui_bridge_genesis_checkpoint: 0,
        eth_bridge_genesis_block: 0,
        eth_sui_bridge_contract_address: bridge_test_cluster.sui_bridge_address(),
        metric_port: 9001,
        disable_eth: None,
    };

    (config, bridge_test_cluster, db)
}

// TODO remove below functions when we can import bridge test utils
pub async fn initiate_bridge_eth_to_sui(
    bridge_test_cluster: &BridgeTestCluster,
    amount: u64,
    nonce: u64,
) -> Result<(), anyhow::Error> {
    info!("Depositing native Ether to Solidity contract, nonce: {nonce}, amount: {amount}");
    let (eth_signer, eth_address) = bridge_test_cluster
        .get_eth_signer_and_address()
        .await
        .unwrap();

    let sui_address = bridge_test_cluster.sui_user_address();
    let sui_chain_id = bridge_test_cluster.sui_chain_id();
    let eth_chain_id = bridge_test_cluster.eth_chain_id();
    let token_id = TOKEN_ID_ETH;

    let sui_amount = (U256::from(amount) * U256::exp10(8)).as_u64(); // DP for Ether on Sui

    let eth_tx = deposit_native_eth_to_sol_contract(
        &eth_signer,
        bridge_test_cluster.contracts().sui_bridge,
        sui_address,
        sui_chain_id,
        amount,
    )
    .await;
    let tx_receipt = send_eth_tx_and_get_tx_receipt(eth_tx).await;
    let eth_bridge_event = tx_receipt
        .logs
        .iter()
        .find_map(EthBridgeEvent::try_from_log)
        .unwrap();
    let EthBridgeEvent::EthSuiBridgeEvents(EthSuiBridgeEvents::TokensDepositedFilter(
        eth_bridge_event,
    )) = eth_bridge_event
    else {
        unreachable!();
    };
    // assert eth log matches
    assert_eq!(eth_bridge_event.source_chain_id, eth_chain_id as u8);
    assert_eq!(eth_bridge_event.nonce, nonce);
    assert_eq!(eth_bridge_event.destination_chain_id, sui_chain_id as u8);
    assert_eq!(eth_bridge_event.token_id, token_id);
    assert_eq!(eth_bridge_event.sui_adjusted_amount, sui_amount);
    assert_eq!(eth_bridge_event.sender_address, eth_address);
    assert_eq!(eth_bridge_event.recipient_address, sui_address.to_vec());
    info!(
        "Deposited Eth to Solidity contract, block: {:?}",
        tx_receipt.block_number
    );

    wait_for_transfer_action_status(
        bridge_test_cluster.bridge_client(),
        eth_chain_id,
        nonce,
        BridgeActionStatus::Claimed,
    )
    .await
    .tap_ok(|_| {
        info!("Eth to Sui bridge transfer claimed");
    })
}

async fn wait_for_transfer_action_status(
    sui_bridge_client: &SuiBridgeClient,
    chain_id: BridgeChainId,
    nonce: u64,
    status: BridgeActionStatus,
) -> Result<(), anyhow::Error> {
    // Wait for the bridge action to be approved
    let now = std::time::Instant::now();
    info!(
        "Waiting for onchain status {:?}. chain: {:?}, nonce: {nonce}",
        status, chain_id as u8
    );
    loop {
        let timer = std::time::Instant::now();
        let res = sui_bridge_client
            .get_token_transfer_action_onchain_status_until_success(chain_id as u8, nonce)
            .await;
        info!(
            "get_token_transfer_action_onchain_status_until_success took {:?}, status: {:?}",
            timer.elapsed(),
            res
        );

        if res == status {
            info!(
                "detected on chain status {:?}. chain: {:?}, nonce: {nonce}",
                status, chain_id as u8
            );
            return Ok(());
        }
        if now.elapsed().as_secs() > 60 {
            return Err(anyhow!(
                "Timeout waiting for token transfer action to be {:?}. chain_id: {chain_id:?}, nonce: {nonce}. Time elapsed: {:?}",
                status,
                now.elapsed(),
            ));
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}

pub(crate) async fn deposit_native_eth_to_sol_contract(
    signer: &EthSigner,
    contract_address: EthAddress,
    sui_recipient_address: SuiAddress,
    sui_chain_id: BridgeChainId,
    amount: u64,
) -> ContractCall<EthSigner, ()> {
    let contract = EthSuiBridge::new(contract_address, signer.clone().into());
    let sui_recipient_address = sui_recipient_address.to_vec().into();
    let amount = U256::from(amount) * U256::exp10(18); // 1 ETH
    contract
        .bridge_eth(sui_recipient_address, sui_chain_id as u8)
        .value(amount)
}
