// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use test_utils::network::setup_network_and_wallet;
use test_utils::network::publish_package_and_make_counter;
use test_utils::network::increment_counter;
use test_utils::network::wait_for_all_txes;
use test_utils::network::move_transaction;
use sui::client_commands::{SuiClientCommandResult, SuiClientCommands, WalletContext};
use std::sync::Arc;
use tokio::sync::Mutex;
use sui_json_rpc_api::rpc_types::SuiExecutionStatus;
use sui_json_rpc_api::rpc_types::SplitCoinResponse;
use sui_node::SuiNode;
use sui_types::base_types::TransactionDigest;
use sui_json::SuiJsonValue;
use sui_json_rpc_api::rpc_types::TransactionResponse;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};

#[tokio::main]
async fn main() {
    test_full_node_sync_flood().await;
}

async fn test_full_node_sync_flood() -> Result<(), anyhow::Error> {
    let (swarm, context, _) = setup_network_and_wallet().await?;
    let config = swarm.config().generate_fullnode_config();
    let node = SuiNode::start(&config).await?;
    let mut futures = Vec::new();

    let sender = context.config.accounts.get(0).cloned().unwrap();
    let (package_ref, counter_id) = publish_package_and_make_counter(&context, sender).await;

    let context: Arc<Mutex<WalletContext>> = Arc::new(Mutex::new(context));
    let counter = Arc::new(AtomicU64::new(0));
    // Start up 5 different tasks that all spam txs at the authorities.
    for i in 0..2 {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let context = context.clone();
        let cloned_counter = counter.clone();
        tokio::task::spawn(async move {
            
            /* 
            let (sender, object_to_split) = {
                let context = &mut context.lock().await;
                let address = context.config.accounts[i];
                SuiClientCommands::SyncClientState {
                    address: Some(address),
                }
                .execute(context)
                .await
                .unwrap();

                let sender = context.config.accounts.get(0).cloned().unwrap();

                let coins = context.gas_objects(sender).await.unwrap();
                let object_to_split = coins.first().unwrap().1.reference.to_object_ref();
                (sender, object_to_split)
            };
            */

            //let mut owned_tx_digest = None;
            let mut shared_tx_digest = None;
            let mut gas_object = None;
            
            for _ in 0..9 {
                /* 
                let res = {
                    let context = &mut context.lock().await;
                    
                    SuiClientCommands::SplitCoin {
                        amounts: vec![1],
                        coin_id: object_to_split.0,
                        gas: gas_object,
                        gas_budget: 50000,
                    }
                    .execute(context)
                    .await
                    .unwrap()
                };
                

                owned_tx_digest = if let SuiClientCommandResult::SplitCoin(SplitCoinResponse {
                    certificate,
                    updated_gas,
                    updated_coin,
                    new_coins,
                }) = res
                {
                    // Re-use the same gas id next time to avoid O(n^2) fetches due to automatic
                    // gas selection.
                    gas_object = Some(updated_gas.id());
                    Some(certificate.transaction_digest)
                } else {
                    panic!("transfer command did not return WalletCommandResult::Transfer");
                };
                */
                let context = &context.lock().await;
                let sender = context.config.accounts[i];
                shared_tx_digest = Some(
                    increment_counter(context, sender, gas_object, package_ref, counter_id).await,
                );
                cloned_counter.fetch_add(1, Ordering::SeqCst);
            }
            tx.send((shared_tx_digest.unwrap()))
                .unwrap();
        });
        futures.push(rx);
    }

    // make sure the node syncs up to the last digest sent by each task.
    let digests: Vec<TransactionDigest> = futures::future::join_all(futures)
        .await
        .iter()
        .map(|r| r.clone().unwrap())
        //.flat_map(|(a, b)| std::iter::once(a).chain(std::iter::once(b)))
        .collect();
    //wait_for_all_txes(digests, node.state().clone()).await;

    {
        let context = &mut context.lock().await;
        let address = context.active_address()?;
        let value = counter.load(Ordering::SeqCst);
        let resp = move_transaction(
            context,
        "counter",
        "assert_value",
        package_ref,
        vec![SuiJsonValue::new(json!(counter_id.to_hex_literal())).unwrap(), SuiJsonValue::new(json!(value)).unwrap()],
        address,
        None,
    ).await;
    if let TransactionResponse::EffectResponse(effects) = resp {
        assert!(matches!(effects.effects.status, SuiExecutionStatus::Success { .. }));
        let value = counter.load(Ordering::SeqCst);
        eprintln!("success ...{value}");
    } else {
        panic!()
    };
    }
    Ok(())
}
