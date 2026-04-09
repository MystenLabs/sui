// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_light_client::authenticated_events::{AuthenticatedEventsClient, ClientConfig};
use sui_rpc::field::{FieldMask, FieldMaskUtil};
use sui_rpc_api::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc_api::proto::sui::rpc::v2::GetEpochRequest;
use sui_sdk::rpc_types::{SuiObjectDataOptions, SuiTransactionBlockResponseOptions};
use sui_sdk::SuiClientBuilder;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::committee::Committee;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{Transaction, TransactionData};

const TESTNET_RPC_URL: &str = "https://fullnode.testnet.sui.io:443";
const TESTNET_ARCHIVE_URL: &str = "https://checkpoints.testnet.sui.io";
const TESTNET_PACKAGE_ID: &str =
    "0xd458c0d4e6d8daff94ee4fcfa178d1562cddc1d4d0fc1c4c63474dee1ca88df3";

async fn get_genesis_committee() -> Committee {
    let mut ledger_client = LedgerServiceClient::connect(TESTNET_RPC_URL)
        .await
        .unwrap();

    let response = ledger_client
        .get_epoch(GetEpochRequest::new(0).with_read_mask(FieldMask::from_paths(["committee"])))
        .await
        .unwrap()
        .into_inner();

    let proto_committee = response.epoch.unwrap().committee.unwrap();
    let sdk_committee = sui_sdk_types::ValidatorCommittee::try_from(&proto_committee).unwrap();
    Committee::from(sdk_committee)
}

async fn get_current_epoch() -> u64 {
    let mut ledger_client = LedgerServiceClient::connect(TESTNET_RPC_URL)
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

fn load_keystore() -> Keystore {
    let home = std::env::var("HOME").expect("HOME env var not set");
    let keystore_path = std::path::PathBuf::from(home).join(".sui/sui_config/sui.keystore");
    Keystore::File(
        FileBasedKeystore::load_or_create(&keystore_path).expect("Failed to load keystore"),
    )
}

fn sign_transaction(
    tx_data: TransactionData,
    keystore: &Keystore,
    address: &SuiAddress,
) -> Transaction {
    let keypair = keystore
        .export(address)
        .expect("Key not found in keystore");
    let sig = Transaction::signature_from_signer(
        tx_data.clone(),
        shared_crypto::intent::Intent::sui_transaction(),
        keypair,
    );
    Transaction::from_data(tx_data, vec![sig])
}

#[tokio::test]
#[ignore]
async fn test_testnet_authenticated_events_e2e() {
    let keystore = load_keystore();
    let addresses = keystore.addresses();
    assert!(
        !addresses.is_empty(),
        "No addresses in keystore. Run `sui client active-address` to check."
    );

    let sui_client = SuiClientBuilder::default()
        .build(TESTNET_RPC_URL)
        .await
        .unwrap();

    let address = addresses[0];
    let package_id =
        ObjectID::from_hex_literal(TESTNET_PACKAGE_ID).expect("Invalid package ID");
    println!("Using address: {address}");
    println!("Using package: {package_id}");

    let coins = sui_client
        .coin_read_api()
        .get_coins(address, None, None, None)
        .await
        .unwrap();
    assert!(
        !coins.data.is_empty(),
        "No SUI coins for {address}. Fund this address on testnet first."
    );
    println!("Balance: {} MIST", coins.data[0].balance);

    let rgp = sui_client
        .governance_api()
        .get_reference_gas_price()
        .await
        .unwrap();

    let current_epoch = get_current_epoch().await;
    println!("Current testnet epoch: {current_epoch}");

    let genesis_committee = get_genesis_committee().await;
    println!("Got genesis committee for epoch 0");

    let start = std::time::Instant::now();
    let client = Arc::new(
        AuthenticatedEventsClient::new_with_config(
            TESTNET_RPC_URL,
            Some(TESTNET_ARCHIVE_URL),
            genesis_committee,
            ClientConfig::default(),
        )
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
            .build(TESTNET_RPC_URL)
            .await
            .unwrap();
        let keystore = load_keystore();

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

                let tx = sign_transaction(tx_data, &keystore, &address);

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
