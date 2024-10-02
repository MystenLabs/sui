// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use fastcrypto::ed25519::Ed25519KeyPair;
use fastcrypto::encoding::{Base64, Encoding};
use fastcrypto::jwt_utils::parse_and_validate_jwt;
use fastcrypto::traits::{EncodeDecodeBase64, KeyPair};
use fastcrypto_zkp::bn254::utils::get_proof;
use fastcrypto_zkp::bn254::utils::{gen_address_seed, get_salt};
use fastcrypto_zkp::bn254::zk_login::ZkLoginInputs;
use rand::rngs::StdRng;
use rand::SeedableRng;
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
use sui_types::crypto::{PublicKey, SuiKeyPair};
use sui_types::multisig::{MultiSig, MultiSigPublicKey};
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
    test_multisig: bool, // if true, put zklogin in a multisig address with another traditional pubkey.
    sign_with_sk: bool, // if true, submit tx with the traditional sig, otherwise submit with zklogin sig.
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
    .map_err(|e| anyhow!("Failed to get proof {e}"))?;
    println!("ZkLogin inputs:");
    println!("{:?}", serde_json::to_string(&reader).unwrap());

    let (sub, aud, _) = parse_and_validate_jwt(parsed_token)?;
    let address_seed = gen_address_seed(&user_salt, "sub", &sub, &aud)?;
    let zk_login_inputs = ZkLoginInputs::from_reader(reader, &address_seed)?;

    let skp1 = SuiKeyPair::Ed25519(Ed25519KeyPair::generate(&mut StdRng::from_seed([1; 32])));
    let multisig_pk = MultiSigPublicKey::new(
        vec![
            PublicKey::from_zklogin_inputs(&zk_login_inputs)?,
            skp1.public(),
        ],
        vec![1, 1],
        1,
    )?;

    let sender = if test_multisig {
        keystore.add_key(None, skp1)?;
        println!("Use multisig address as sender");
        SuiAddress::from(&multisig_pk)
    } else {
        println!("Use single zklogin address as sender");
        SuiAddress::try_from_unpadded(&zk_login_inputs)?
    };
    println!("Sender: {:?}", sender);

    // Request some coin from faucet and build a test transaction.
    let sui = SuiClientBuilder::default().build(fullnode_url).await?;
    request_tokens_from_faucet(sender, gas_url).await?; // transfer coin
    request_tokens_from_faucet(sender, gas_url).await?; // gas coin
    sleep(Duration::from_secs(10));

    let response = sui
        .coin_read_api()
        .get_coins(sender, None, None, Some(2))
        .await?;

    if response.data.len() != 2 {
        panic!("Faucet did not work correctly and the provided Sui address has no coins")
    }

    let transfer_coin = response.data[0].coin_object_id;
    let gas_coin = response.data[1].coin_object_id;

    let txb_res = sui
        .transaction_builder()
        .transfer_object(
            sender,
            transfer_coin,
            Some(gas_coin),
            5000000,
            SuiAddress::ZERO, // as a demo, send to a dummy address
        )
        .await?;
    println!(
        "Faucet requested and created test transaction: {:?}",
        Base64::encode(bcs::to_bytes(&txb_res).unwrap())
    );

    let final_sig = if test_multisig {
        let sig = if sign_with_sk {
            // Create a generic sig from the traditional keypair
            GenericSignature::Signature(keystore.sign_secure(
                &ephemeral_key_identifier,
                &txb_res,
                Intent::sui_transaction(),
            )?)
        } else {
            // Sign transaction with the ephemeral key
            let signature = keystore.sign_secure(
                &ephemeral_key_identifier,
                &txb_res,
                Intent::sui_transaction(),
            )?;

            GenericSignature::from(ZkLoginAuthenticator::new(
                zk_login_inputs,
                max_epoch,
                signature,
            ))
        };

        let multisig = GenericSignature::MultiSig(MultiSig::combine(vec![sig], multisig_pk)?);
        println!("Multisig Serialized: {:?}", multisig.encode_base64());
        multisig
    } else {
        // Sign transaction with the ephemeral key
        let signature = keystore.sign_secure(
            &ephemeral_key_identifier,
            &txb_res,
            Intent::sui_transaction(),
        )?;

        let single_sig = GenericSignature::from(ZkLoginAuthenticator::new(
            zk_login_inputs,
            max_epoch,
            signature,
        ));
        println!(
            "Single zklogin sig Serialized: {:?}",
            single_sig.encode_base64()
        );
        single_sig
    };
    let transaction_response = sui
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_generic_sig_data(txb_res, vec![final_sig]),
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
