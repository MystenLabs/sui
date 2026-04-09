// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use sui_light_client::authenticated_events::AuthenticatedEventsClient;
use sui_rpc::field::{FieldMask, FieldMaskUtil};
use sui_rpc_api::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc_api::proto::sui::rpc::v2::GetEpochRequest;
use sui_sdk::rpc_types::{
    SuiObjectDataOptions, SuiTransactionBlockResponseOptions,
};
use sui_sdk::SuiClientBuilder;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::committee::Committee;
use shared_crypto::intent::Intent;
use sui_types::crypto::{SuiKeyPair, get_key_pair};
use fastcrypto::ed25519::Ed25519KeyPair;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{Transaction, TransactionData};

const DEVNET_RPC_URL: &str = "https://fullnode.devnet.sui.io:443";
const DEVNET_FAUCET_URL: &str = "https://faucet.devnet.sui.io/v2/gas";

async fn request_devnet_tokens(address: SuiAddress) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let resp = client
        .post(DEVNET_FAUCET_URL)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "FixedAmountRequest": {
                "recipient": address.to_string()
            }
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("Faucet request failed: {}", resp.status());
    }

    tokio::time::sleep(Duration::from_secs(3)).await;
    Ok(())
}

async fn get_genesis_committee() -> Committee {
    let mut ledger_client = LedgerServiceClient::connect(DEVNET_RPC_URL)
        .await
        .unwrap();

    let response = ledger_client
        .get_epoch(GetEpochRequest::new(0).with_read_mask(FieldMask::from_paths(["committee"])))
        .await
        .unwrap()
        .into_inner();

    let proto_committee = response.epoch.unwrap().committee.unwrap();
    let sdk_committee =
        sui_sdk_types::ValidatorCommittee::try_from(&proto_committee).unwrap();
    Committee::from(sdk_committee)
}

async fn get_current_epoch() -> u64 {
    let mut ledger_client = LedgerServiceClient::connect(DEVNET_RPC_URL)
        .await
        .unwrap();

    let response = ledger_client
        .get_epoch(
            GetEpochRequest::default().with_read_mask(FieldMask::from_paths(["epoch"])),
        )
        .await
        .unwrap()
        .into_inner();

    response.epoch.unwrap().epoch.unwrap()
}

fn sign_transaction(tx_data: TransactionData, keypair: &SuiKeyPair) -> Transaction {
    let sig = Transaction::signature_from_signer(tx_data.clone(), Intent::sui_transaction(), keypair);
    Transaction::from_data(tx_data, vec![sig])
}

#[tokio::test]
#[ignore]
async fn test_devnet_authenticated_events_e2e() {
    let sui_client = SuiClientBuilder::default()
        .build(DEVNET_RPC_URL)
        .await
        .unwrap();

    let (address, ed_keypair): (SuiAddress, Ed25519KeyPair) = get_key_pair();
    let keypair = SuiKeyPair::Ed25519(ed_keypair);
    println!("Generated address: {address}");

    println!("Requesting tokens from devnet faucet...");
    request_devnet_tokens(address).await.unwrap();

    let coins = sui_client
        .coin_read_api()
        .get_coins(address, None, None, None)
        .await
        .unwrap();
    assert!(!coins.data.is_empty(), "No coins after faucet request");
    println!(
        "Got gas coin {} with balance {}",
        coins.data[0].coin_object_id, coins.data[0].balance
    );

    let rgp = sui_client
        .governance_api()
        .get_reference_gas_price()
        .await
        .unwrap();

    let gas_ref = sui_client
        .read_api()
        .get_object_with_options(
            coins.data[0].coin_object_id,
            SuiObjectDataOptions::new().with_owner(),
        )
        .await
        .unwrap()
        .object_ref_if_exists()
        .unwrap();

    println!("Publishing auth_event package...");
    let package_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../sui-e2e-tests/tests/data/auth_event");

    let tx_data = sui_test_transaction_builder::TestTransactionBuilder::new(
        address, gas_ref, rgp,
    )
    .with_gas_budget(500_000_000)
    .publish(package_path)
    .build();

    let tx = sign_transaction(tx_data, &keypair);

    let publish_response = sui_client
        .quorum_driver_api()
        .execute_transaction_block(
            tx,
            SuiTransactionBlockResponseOptions::new()
                .with_object_changes()
                .with_effects(),
            None,
        )
        .await
        .unwrap();

    let package_id: ObjectID = publish_response
        .object_changes
        .unwrap()
        .iter()
        .find_map(|change| {
            if let sui_sdk::rpc_types::ObjectChange::Published { package_id, .. } = change {
                Some(*package_id)
            } else {
                None
            }
        })
        .expect("No published package found");

    println!("Published package: {package_id}");

    let current_epoch = get_current_epoch().await;
    println!("Current devnet epoch: {current_epoch}");

    let genesis_committee = get_genesis_committee().await;
    println!("Got genesis committee for epoch 0");

    let start = std::time::Instant::now();
    let client = Arc::new(
        AuthenticatedEventsClient::new(DEVNET_RPC_URL, genesis_committee)
            .await
            .unwrap(),
    );

    let stream_id = SuiAddress::from(package_id);
    let mut stream = Box::pin(client.clone().stream_events(stream_id).await.unwrap());
    let ratchet_elapsed = start.elapsed();

    println!(
        "Client connected and stream started (trust ratcheted through {} epochs) in {:?}",
        current_epoch, ratchet_elapsed
    );

    let num_events = 10u64;
    let gas_coin_id = coins.data[0].coin_object_id;

    let emit_times = Arc::new(tokio::sync::Mutex::new(Vec::<std::time::Instant>::new()));
    let emit_times_clone = emit_times.clone();

    let emit_handle = {
        let sui_client = SuiClientBuilder::default()
            .build(DEVNET_RPC_URL)
            .await
            .unwrap();
        let keypair = keypair.copy();

        tokio::spawn(async move {
            for i in 0..num_events {
                tokio::time::sleep(Duration::from_secs(1)).await;

                let gas_ref = sui_client
                    .read_api()
                    .get_object_with_options(
                        gas_coin_id,
                        SuiObjectDataOptions::new().with_owner(),
                    )
                    .await
                    .unwrap()
                    .object_ref_if_exists()
                    .unwrap();

                let mut ptb = ProgrammableTransactionBuilder::new();
                let val = ptb.pure(i).unwrap();
                ptb.programmable_move_call(
                    package_id,
                    move_core_types::identifier::Identifier::new("events").unwrap(),
                    move_core_types::identifier::Identifier::new("emit").unwrap(),
                    vec![],
                    vec![val],
                );

                let tx_data = TransactionData::new(
                    sui_types::transaction::TransactionKind::ProgrammableTransaction(
                        ptb.finish(),
                    ),
                    address,
                    gas_ref,
                    500_000_000,
                    rgp,
                );

                let tx = sign_transaction(tx_data, &keypair);

                sui_client
                    .quorum_driver_api()
                    .execute_transaction_block(
                        tx,
                        SuiTransactionBlockResponseOptions::new().with_effects(),
                        None,
                    )
                    .await
                    .unwrap();

                emit_times_clone.lock().await.push(std::time::Instant::now());
                println!("  Emitted event {}", i);
            }
        })
    };

    println!("Emitting {num_events} events (1/sec) while streaming...");
    let mut received = 0u64;
    let mut latencies = Vec::new();

    let timeout_result = tokio::time::timeout(Duration::from_secs(120), async {
        while received < num_events {
            match stream.next().await {
                Some(Ok(event)) => {
                    let receive_time = std::time::Instant::now();
                    let times = emit_times.lock().await;
                    if let Some(&emit_time) = times.get(received as usize) {
                        let latency = receive_time.duration_since(emit_time);
                        latencies.push(latency);
                        println!(
                            "  [{}/{}] checkpoint {} latency: {:?}",
                            received + 1,
                            num_events,
                            event.checkpoint,
                            latency,
                        );
                    }
                    received += 1;
                }
                Some(Err(e)) => panic!("Stream error: {:?}", e),
                None => panic!("Stream ended unexpectedly after {} events", received),
            }
        }
    })
    .await;

    emit_handle.await.unwrap();

    if timeout_result.is_err() {
        panic!(
            "Timed out after receiving {}/{} events",
            received, num_events,
        );
    }

    let total_elapsed = start.elapsed();

    let avg_latency = if latencies.is_empty() {
        Duration::ZERO
    } else {
        latencies.iter().sum::<Duration>() / latencies.len() as u32
    };
    let min_latency = latencies.iter().min().copied().unwrap_or(Duration::ZERO);
    let max_latency = latencies.iter().max().copied().unwrap_or(Duration::ZERO);

    println!("\n=== Results ===");
    println!("Epochs ratcheted: {current_epoch}");
    println!("Trust ratchet time: {ratchet_elapsed:?}");
    println!("Events: {received}/{num_events}");
    println!("Emit-to-verified-receive latency:");
    println!("  min: {min_latency:?}");
    println!("  max: {max_latency:?}");
    println!("  avg: {avg_latency:?}");
    println!("Total time: {total_elapsed:?}");
}
