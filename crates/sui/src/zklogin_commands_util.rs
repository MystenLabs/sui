// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use fastcrypto::encoding::{Base64, Encoding};
use fastcrypto::jwt_utils::parse_and_validate_jwt;
use fastcrypto::traits::EncodeDecodeBase64;
use fastcrypto_zkp::bn254::utils::{gen_address_seed, get_zk_login_address};
use fastcrypto_zkp::bn254::utils::{get_proof, get_salt};
use fastcrypto_zkp::bn254::zk_login::ZkLoginInputs;
use regex::Regex;
use reqwest::Client;
use serde_json::json;
use shared_crypto::intent::Intent;
use std::io;
use std::io::Write;
use std::thread::sleep;
use std::time::Duration;
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_keys::keystore::{AccountKeystore, Keystore};
use sui_sdk::SuiClientBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::committee::EpochId;
use sui_types::signature::GenericSignature;
use sui_types::transaction::Transaction;
use sui_types::zk_login_authenticator::ZkLoginAuthenticator;

/// Read a line from stdin, parse the id_token field and return.
pub fn read_cli_line() -> Result<String, anyhow::Error> {
    let mut s = String::new();
    let _ = io::stdout().flush();
    io::stdin().read_line(&mut s)?;
    let full_url = s.trim_end().to_string();
    let mut parsed_token = "";
    let re = Regex::new(r"id_token=([^&]+)").unwrap();
    if let Some(captures) = re.captures(&full_url) {
        if let Some(id_token) = captures.get(1) {
            parsed_token = id_token.as_str();
        }
    }
    Ok(parsed_token.to_string())
}

/// A util function to request gas token from faucet for the given address.
pub(crate) async fn request_tokens_from_faucet(
    address: SuiAddress,
    gas_url: &str,
) -> Result<(), anyhow::Error> {
    let client = Client::new();
    client
        .post(gas_url)
        .header("Content-Type", "application/json")
        .json(&json![{
            "FixedAmountRequest": {
                "recipient": &address.to_string()
            }
        }])
        .send()
        .await?;
    Ok(())
}

/// A helper function that performs a zklogin test transaction based on the provided parameters.
pub async fn perform_zk_login_test_tx(
    parsed_token: &str,
    max_epoch: EpochId,
    jwt_randomness: &str,
    kp_bigint: &str,
    ephemeral_key_identifier: SuiAddress,
    keystore: &mut Keystore,
    network: &str,
) -> Result<String, anyhow::Error> {
    let (gas_url, fullnode_url) = get_config(network);
    let user_salt = get_salt(parsed_token, "https://salt.api.mystenlabs.com/get_salt")
        .await
        .unwrap_or("129390038577185583942388216820280642146".to_string());
    println!("User salt: {user_salt}");
    let reader = get_proof(
        parsed_token,
        max_epoch,
        jwt_randomness,
        kp_bigint,
        &user_salt,
        "https://prover-dev.mystenlabs.com/v1",
    )
    .await
    .map_err(|_| anyhow!("Failed to get proof"))?;
    println!("ZkLogin inputs:");
    println!("{:?}", serde_json::to_string(&reader).unwrap());
    let (sub, aud) = parse_and_validate_jwt(parsed_token)?;
    let address_seed = gen_address_seed(&user_salt, "sub", &sub, &aud)?;
    let zk_login_inputs = ZkLoginInputs::from_reader(reader, &address_seed)?;
    let zklogin_address = SuiAddress::from_bytes(get_zk_login_address(
        zk_login_inputs.get_address_seed(),
        zk_login_inputs.get_iss(),
    )?)?;
    println!("ZkLogin Address: {:?}", zklogin_address);

    // Request some coin from faucet and build a test transaction.
    let sui = SuiClientBuilder::default().build(fullnode_url).await?;
    request_tokens_from_faucet(zklogin_address, gas_url).await?;
    sleep(Duration::from_secs(10));

    let Some(coin) = sui
        .coin_read_api()
        .get_coins(zklogin_address, None, None, None)
        .await?
        .next_cursor
    else {
        panic!("Faucet did not work correctly and the provided Sui address has no coins")
    };
    let txb_res = sui
        .transaction_builder()
        .transfer_object(
            zklogin_address,
            coin,
            None,
            5000000,
            SuiAddress::ZERO, // as a demo, send to a dummy address
        )
        .await?;
    println!(
        "Faucet requested and created test transaction: {:?}",
        Base64::encode(bcs::to_bytes(&txb_res).unwrap())
    );

    // Sign transaction with the ephemeral key
    let signature = keystore.sign_secure(
        &ephemeral_key_identifier,
        &txb_res,
        Intent::sui_transaction(),
    )?;

    let sig = GenericSignature::from(ZkLoginAuthenticator::new(
        zk_login_inputs,
        max_epoch,
        signature,
    ));
    println!(
        "ZkLogin Authenticator Signature Serialized: {:?}",
        sig.encode_base64()
    );

    let transaction_response = sui
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_generic_sig_data(txb_res, Intent::sui_transaction(), vec![sig]),
            SuiTransactionBlockResponseOptions::full_content(),
            None,
        )
        .await?;
    Ok(transaction_response.digest.base58_encode())
}

fn get_config(network: &str) -> (&str, &str) {
    match network {
        "devnet" => (
            "https://faucet.devnet.sui.io/gas",
            "https://rpc.devnet.sui.io:443",
        ),
        "localnet" => ("http://127.0.0.1:9123/gas", "http://127.0.0.1:9000"),
        _ => panic!("Invalid network"),
    }
}
