mod utils;
use anyhow::anyhow;
use fastcrypto::hash::HashFunction;
use shared_crypto::intent::{Intent, IntentMessage};
use std::str::FromStr;
use sui_sdk::{
    rpc_types::SuiTransactionBlockResponseOptions,
    types::{
        programmable_transaction_builder::ProgrammableTransactionBuilder,
        quorum_driver_types::ExecuteTransactionRequestType,
        transaction::{Transaction, TransactionData},
    },
    SuiClientBuilder,
};
use sui_types::crypto::Signer;
use sui_types::{
    base_types::SuiAddress,
    crypto::{EncodeDecodeBase64, SuiKeyPair},
};
use utils::{fetch_coin, request_tokens_from_faucet};

// This example shows how to use programmable transactions with sponsor following the steps:
// 1. Initialize a Sui client.
// 2. Decode base64-encoded keys for both the sponsor and sender accounts.
// 3. Fetch or request SUI tokens for the sponsor account.
// 4. Construct a programmable transaction to transfer SUI tokens.
// 5. Sign and execute the transaction using both the sponsor's and sender's signatures.

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // 1. Initialize a Sui client.
    let sui_client = SuiClientBuilder::default().build_devnet().await?;

    // 2. Decode base64-encoded keys for both the sponsor and sender accounts.
    let sponser_skp = SuiKeyPair::decode_base64("AK9CMoIzS+IZXs+HfyZX8s3b3o2sVxsFbkc2XkHNPz80")
        .map_err(|_| anyhow!("Invalid base64"))?;
    let sponser = SuiAddress::from(&sponser_skp.public());

    let sender_skp = SuiKeyPair::decode_base64("AEZkTrfkp/g68mIRc09525133dKg2U6Hr2RZj/pSph18")
        .map_err(|_| anyhow!("Invalid base64"))?;
    let sender = SuiAddress::from(&sender_skp.public());

    let recipient =
        SuiAddress::from_str("0x230f16fce2c80a873f12fb451b5475e4a0a5123451a3f2e00d894c2d29242561")?;

    // 3. Fetch or request SUI tokens for the sponsor account.
    let _sponser_coin = fetch_coin(&sui_client, &sponser).await?;
    if _sponser_coin.is_none() {
        request_tokens_from_faucet(sponser, &sui_client).await?;
    }

    let coins = sui_client
        .coin_read_api()
        .get_coins(sponser, None, None, None)
        .await?;

    let selected_gas_coins: Vec<_> = coins.data.iter().map(|coin| coin.object_ref()).collect();

    let amount = 100000000;
    // 4. Construct a programmable transaction to transfer SUI tokens.
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.pay_sui(vec![recipient], vec![amount])?;
        builder.finish()
    };

    let gas_budget = 5_000_000;
    let gas_price = sui_client.read_api().get_reference_gas_price().await?;

    // create the transaction data that will be sent to the network
    let tx_data = TransactionData::new_programmable_allow_sponsor(
        sender,
        selected_gas_coins,
        pt,
        gas_budget,
        gas_price,
        sponser,
    );

    // 5. Sign and execute the transaction using both the sponsor's and sender's signatures.
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let raw_tx = bcs::to_bytes(&intent_msg).expect("bcs should not fail");
    let mut hasher = sui_types::crypto::DefaultHash::default();
    hasher.update(raw_tx.clone());
    let digest = hasher.finalize().digest;

    // use SuiKeyPair to sign the digest.
    let sponser_signature = sponser_skp.sign(&digest);
    let sender_signature = sender_skp.sign(&digest);

    // execute the transaction
    print!("Executing the transaction...");
    let transaction_response = sui_client
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_data(tx_data, vec![sponser_signature, sender_signature]),
            SuiTransactionBlockResponseOptions::full_content(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;
    print!("done\n Transaction information: ");
    println!("{:?}", transaction_response);
    Ok(())
}
