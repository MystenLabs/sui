// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use anyhow::Result;
use clap::Parser;
use move_core_types::account_address::AccountAddress;
use rand::Rng;
use sui_indexer::new_rpc_client;
use sui_json_rpc_types::{CheckpointId, TransactionBytes};
use sui_types::base_types::{ObjectID, SuiAddress};

async fn test(url: &String) -> Result<()> {
    let fn_rpc_client = new_rpc_client(url).await?;

    let addy = AccountAddress::from_str(
        "0xbac5b67c7f35ee9006054976b2926c335eedbda97ef3a4f3ff8676e4f2f8d975",
    )?;
    let signer = SuiAddress::from(addy);

    let object_id =
        ObjectID::from_str("0xea55a257f903378574f6fd1cd1c58031c8bb652b1e29c4a11b2b24bddb53675e")?;

    let recipient_account = AccountAddress::from_str(
        "0xbac5b67c7f35ee9006054976b2926c335eedbda97ef3a4f3ff8676e4f2f8d975",
    )?;
    let recipient = SuiAddress::from(recipient_account);
    println!("constructing txn");

    let transaction = fn_rpc_client
        .transaction_builder()
        .transfer_object(signer, object_id, None, 5000, recipient)
        .await?;

    let gas_station_url =
        String::from("https://gas.shinami.com/api/v1/94E27290A4FF478F9688235370C5C398");

    println!("converting data to bytes");
    let bytes = TransactionBytes::from_data(transaction)?;

    let sponsored_bytes = fn_rpc_client
        .transaction_builder()
        .send_bytes_to_sponsor(gas_station_url, bytes, 5000)
        .await?;

    println!("bytes_response {:?}", sponsored_bytes);

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // info!("Running correctness check for indexer...");
    let test_config = TestConfig::parse();
    // let fn_rpc_client = new_rpc_client(&test_config.fn_rpc_client_url).await?;
    // let indexer_rpc_client = new_rpc_client(&test_config.indexer_rpc_client_url).await?;
    test(&test_config.fn_rpc_client_url).await?;

    // let latest_checkpoint = indexer_rpc_client
    //     .read_api()
    //     .get_latest_checkpoint_sequence_number()
    //     .await?;

    // let num = rand::thread_rng().gen_range(10..100);

    // let target_checkpoint = if latest_checkpoint - num > 0 {
    //     latest_checkpoint - num
    // } else {
    //     1
    // };

    // let fn_checkpoint = fn_rpc_client
    //     .read_api()
    //     .get_checkpoint(CheckpointId::SequenceNumber(target_checkpoint))
    //     .await?;

    // let indexer_checkpoint = indexer_rpc_client
    //     .read_api()
    //     .get_checkpoint(CheckpointId::SequenceNumber(target_checkpoint))
    //     .await?;

    // assert_eq!(
    //     fn_checkpoint.transactions.len(),
    //     indexer_checkpoint.transactions.len(),
    //     "Checkpoint number {} length is not the same for FN and Indexer",
    //     target_checkpoint
    // );

    // let fn_checkpoint_transactions = fn_checkpoint.transactions;
    // let indexer_checkpoint_transactions = indexer_checkpoint.transactions;

    // for i in 0..fn_checkpoint_transactions.len() {
    //     let fn_txn_digest = fn_checkpoint_transactions.get(i).cloned();
    //     let idx_txn_digest = indexer_checkpoint_transactions.get(i).cloned();
    //     assert_eq!(
    //         fn_txn_digest, idx_txn_digest,
    //         "Checkpoint transactions mismatch found in {}",
    //         target_checkpoint
    //     );

    //     if let (Some(fn_txn_digest), Some(idx_txn_digest)) = (fn_txn_digest, idx_txn_digest) {
    //         let fn_sui_txn_response = fn_rpc_client
    //             .read_api()
    //             .get_transaction(fn_txn_digest)
    //             .await?;
    //         let indexer_sui_txn_response = indexer_rpc_client
    //             .read_api()
    //             .get_transaction(idx_txn_digest)
    //             .await?;
    //         assert_eq!(
    //             fn_sui_txn_response, indexer_sui_txn_response,
    //             "Checkpoint transactions mismatch found in {}",
    //             target_checkpoint
    //         );
    //     }
    // }

    Ok(())
}

#[derive(Parser)]
#[clap(name = "Transactions Test")]
pub struct TestConfig {
    #[clap(long)]
    pub fn_rpc_client_url: String,
    #[clap(long)]
    pub indexer_rpc_client_url: String,
}
