// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::traits::EncodeDecodeBase64;
use fastcrypto_zkp::bn254::utils::get_enoki_address;
use fastcrypto_zkp::bn254::zk_login::ZkLoginInputs;
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use shared_crypto::intent::Intent;
use std::io;
use std::io::Write;
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_keys::keystore::{AccountKeystore, Keystore};
use sui_sdk::SuiClientBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::committee::EpochId;
use sui_types::signature::GenericSignature;
use sui_types::transaction::Transaction;
use sui_types::zk_login_authenticator::ZkLoginAuthenticator;

const GAS_URL: &str = "http://127.0.0.1:9123/gas";
const SALT_SERVER_URL: &str = "http://salt.api-devnet.mystenlabs.com/get_salt";
const PROVER_SERVER_URL: &str = "http://185.209.177.123:8000/zkp";

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
pub(crate) async fn request_tokens_from_faucet(address: SuiAddress) -> Result<(), anyhow::Error> {
    let client = Client::new();
    client
        .post(GAS_URL)
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

/// Call the salt server to get the salt based on the given JWT token.
pub async fn get_salt(jwt_token: &str) -> Result<String, anyhow::Error> {
    let client = Client::new();
    let body = json!({ "token": jwt_token });
    println!("body: {:?}", body);
    let response = client
        .post(SALT_SERVER_URL)
        .json(&body)
        .header("Content-Type", "application/json")
        .send()
        .await?;
    let full_bytes = response.bytes().await?;
    let res: GetSaltResponse = serde_json::from_slice(&full_bytes)?;
    Ok(res.salt)
}

/// Call the prover backend to get the zklogin inputs based on jwt_token, max_epoch, jwt_randomness, eph_pubkey and salt.
pub async fn get_proof(
    jwt_token: &str,
    max_epoch: EpochId,
    jwt_randomness: &str,
    eph_pubkey: &str,
    salt: &str,
) -> Result<ZkLoginInputs, anyhow::Error> {
    let client = Client::new();
    let body = json!({
        "jwt": jwt_token,
        "eph_public_key": eph_pubkey,
        "max_epoch": max_epoch,
        "jwt_randomness": jwt_randomness,
        "subject_pin": salt,
        "key_claim_name": "sub"
    });
    let response = client
        .post(PROVER_SERVER_URL.to_string())
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;
    let full_bytes = response.bytes().await?;
    let get_proof_response: ZkLoginInputs = serde_json::from_slice(&full_bytes)
        .map_err(|e| anyhow::anyhow!("json deser failed with bytes {:?}: {e}", full_bytes))?;
    Ok(get_proof_response)
}

#[derive(Deserialize)]
struct GetSaltResponse {
    salt: String,
}

/// A helper function that performs a zklogin test transaction based on the provided parameters.
pub async fn perform_zk_login_test_tx(
    parsed_token: &str,
    max_epoch: EpochId,
    jwt_randomness: &str,
    kp_bigint: &str,
    ephemeral_key_identifier: SuiAddress,
    keystore: &mut Keystore,
) -> Result<String, anyhow::Error> {
    let user_salt = get_salt(parsed_token).await?;
    println!("User salt: {user_salt}");
    let mut zk_login_inputs = get_proof(
        parsed_token,
        max_epoch,
        jwt_randomness,
        kp_bigint,
        &user_salt,
    )
    .await?;
    println!("ZkLogin inputs:");
    println!("{:?}", serde_json::to_string(&zk_login_inputs).unwrap());
    zk_login_inputs.init()?;
    let zklogin_address = SuiAddress::from_bytes(get_enoki_address(
        zk_login_inputs.get_address_seed(),
        zk_login_inputs.get_address_params(),
    ))?;
    println!("ZkLogin Address: {:?}", zklogin_address);

    // Request some coin from faucet and build a test transaction.
    let sui = SuiClientBuilder::default()
        .build("http://127.0.0.1:9000")
        .await?;
    request_tokens_from_faucet(zklogin_address).await?;

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
            SuiAddress::random_for_testing_only(),
        )
        .await?;
    println!(
        "Faucet requested and created test transaction: {:?}",
        txb_res
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
