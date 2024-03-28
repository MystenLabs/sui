// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use clap::*;
use fastcrypto::ed25519::Ed25519KeyPair;
use fastcrypto::secp256k1::Secp256k1KeyPair;
use fastcrypto::traits::EncodeDecodeBase64;
use shared_crypto::intent::Intent;
use shared_crypto::intent::IntentMessage;
use std::path::PathBuf;
use std::sync::Arc;
use sui_bridge::client::bridge_authority_aggregator::BridgeAuthorityAggregator;
use sui_bridge::config::BridgeNodeConfig;
use sui_bridge::crypto::BridgeAuthorityKeyPair;
use sui_bridge::crypto::BridgeAuthorityPublicKeyBytes;
use sui_bridge::eth_transaction_builder::build_eth_transaction;
use sui_bridge::sui_client::SuiClient;
use sui_bridge::sui_transaction_builder::build_sui_transaction;
use sui_bridge::tools::{
    make_action, select_contract_address, Args, BridgeCliConfig, BridgeValidatorCommand,
};
use sui_config::Config;
use sui_sdk::SuiClient as SuiSdkClient;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::bridge::BridgeChainId;
use sui_types::crypto::get_key_pair;
use sui_types::crypto::Signature;
use sui_types::crypto::SuiKeyPair;
use sui_types::transaction::Transaction;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        BridgeValidatorCommand::CreateBridgeValidatorKey { path } => {
            generate_bridge_authority_key_and_write_to_file(&path)?;
            println!("Bridge validator key generated at {}", path.display());
        }
        BridgeValidatorCommand::CreateBridgeClientKey { path, use_ecdsa } => {
            generate_bridge_client_key_and_write_to_file(&path, use_ecdsa)?;
            println!("Bridge client key generated at {}", path.display());
        }
        BridgeValidatorCommand::CreateBridgeNodeConfigTemplate { path, run_client } => {
            generate_bridge_node_config_and_write_to_file(&path, run_client)?;
            println!(
                "Bridge node config template generated at {}",
                path.display()
            );
        }

        BridgeValidatorCommand::GovernanceClient { config_path, cmd } => {
            let config = BridgeCliConfig::load(config_path).expect("Couldn't load BridgeCliConfig");
            let sui_client = SuiClient::<SuiSdkClient>::new(&config.sui_rpc_url).await?;
            let eth_signer_client = config
                .get_eth_signer_client()
                .await
                .expect("Failed to get eth signer client");
            let (sui_key, sui_address, gas_object_ref) = config
                .get_sui_account_info()
                .await
                .expect("Failed to get sui account info");
            let bridge_summary = sui_client
                .get_bridge_summary()
                .await
                .expect("Failed to get bridge summary");
            let sui_chain_id = BridgeChainId::try_from(bridge_summary.chain_id).unwrap();
            println!("Sui Chain ID: {:?}", sui_chain_id);
            // TODO: get eth chain id from eth client
            let eth_chain_id = BridgeChainId::EthSepolia;
            println!("Eth Chain ID: {:?}", eth_chain_id);

            // Handle Sui Side
            // Create BridgeAction
            let bridge_committee = Arc::new(
                sui_client
                    .get_bridge_committee()
                    .await
                    .expect("Failed to get bridge committee"),
            );
            let agg = BridgeAuthorityAggregator::new(bridge_committee);
            let sui_action = make_action(sui_chain_id, &cmd);
            let threshold = sui_action.approval_threshold();
            let certified_action = agg
                .request_committee_signatures(sui_action, threshold)
                .await
                .expect("Failed to request committee signatures");
            let bridge_arg = sui_client
                .get_mutable_bridge_object_arg()
                .await
                .expect("Failed to get mutable bridge object arg");
            let tx =
                build_sui_transaction(sui_address, &gas_object_ref, certified_action, bridge_arg)
                    .expect("Failed to build sui transaction");
            let sui_sig = Signature::new_secure(
                &IntentMessage::new(Intent::sui_transaction(), tx.clone()),
                &sui_key,
            );
            let tx = Transaction::from_data(tx, vec![sui_sig]);
            let resp = sui_client
                .execute_transaction_block_with_effects(tx)
                .await
                .expect("Failed to execute transaction block with effects");
            if resp.status_ok().unwrap() {
                println!("Sui Transaction succeeded: {:?}", resp.digest);
            } else {
                println!(
                    "Sui Transaction failed: {:?}. Effects: {:?}",
                    resp.digest, resp.effects
                );
            }

            // Handle Eth Side
            // Create BridgeAction
            let eth_action = make_action(eth_chain_id, &cmd);
            // Create Eth Signer Client
            let threshold = eth_action.approval_threshold();
            let certified_action = agg
                .request_committee_signatures(eth_action, threshold)
                .await
                .expect("Failed to request committee signatures");
            let contract_address = select_contract_address(&config, &cmd);
            let tx = build_eth_transaction(contract_address, eth_signer_client, certified_action)
                .await
                .expect("Failed to build eth transaction");
            println!("sending Eth tx: {:?}", tx);
            match tx.send().await {
                Ok(tx_hash) => {
                    println!("Transaction sent with hash: {:?}", tx_hash);
                }
                Err(err) => {
                    let revert = err.as_revert();
                    println!("Transaction reverted: {:?}", revert);
                }
            };
        }
    }

    Ok(())
}

/// Generate Bridge Authority key (Secp256k1KeyPair) and write to a file as base64 encoded `privkey`.
fn generate_bridge_authority_key_and_write_to_file(path: &PathBuf) -> Result<(), anyhow::Error> {
    let (_, kp): (_, BridgeAuthorityKeyPair) = get_key_pair();
    let eth_address = BridgeAuthorityPublicKeyBytes::from(&kp.public).to_eth_address();
    println!(
        "Corresponding Ethereum address by this ecdsa key: {:?}",
        eth_address
    );
    let sui_address = SuiAddress::from(&kp.public);
    println!(
        "Corresponding Sui address by this ecdsa key: {:?}",
        sui_address
    );
    let base64_encoded = kp.encode_base64();
    std::fs::write(path, base64_encoded)
        .map_err(|err| anyhow!("Failed to write encoded key to path: {:?}", err))
}

/// Generate Bridge Client key (Secp256k1KeyPair or Ed25519KeyPair) and write to a file as base64 encoded `flag || privkey`.
fn generate_bridge_client_key_and_write_to_file(
    path: &PathBuf,
    use_ecdsa: bool,
) -> Result<(), anyhow::Error> {
    let kp = if use_ecdsa {
        let (_, kp): (_, Secp256k1KeyPair) = get_key_pair();
        let eth_address = BridgeAuthorityPublicKeyBytes::from(&kp.public).to_eth_address();
        println!(
            "Corresponding Ethereum address by this ecdsa key: {:?}",
            eth_address
        );
        SuiKeyPair::from(kp)
    } else {
        let (_, kp): (_, Ed25519KeyPair) = get_key_pair();
        SuiKeyPair::from(kp)
    };
    let sui_address = SuiAddress::from(&kp.public());
    println!("Corresponding Sui address by this key: {:?}", sui_address);

    let contents = kp.encode_base64();
    std::fs::write(path, contents)
        .map_err(|err| anyhow!("Failed to write encoded key to path: {:?}", err))
}

/// Generate Bridge Node Config template and write to a file.
fn generate_bridge_node_config_and_write_to_file(
    path: &PathBuf,
    run_client: bool,
) -> Result<(), anyhow::Error> {
    let mut config = BridgeNodeConfig {
        server_listen_port: 9191,
        metrics_port: 9184,
        bridge_authority_key_path_base64_raw: PathBuf::from("/path/to/your/bridge_authority_key"),
        sui_rpc_url: "your_sui_rpc_url".to_string(),
        eth_rpc_url: "your_eth_rpc_url".to_string(),
        eth_addresses: vec!["bridge_eth_proxy_address".into()],
        approved_governance_actions: vec![],
        run_client,
        bridge_client_key_path_base64_sui_key: None,
        bridge_client_gas_object: None,
        sui_bridge_modules: Some(vec!["modules_to_watch".into()]),
        db_path: None,
        eth_bridge_contracts_start_block_override: None,
        sui_bridge_modules_last_processed_event_id_override: None,
    };
    if run_client {
        config.bridge_client_key_path_base64_sui_key =
            Some(PathBuf::from("/path/to/your/bridge_client_key"));
        config.bridge_client_gas_object = Some(ObjectID::ZERO);
        config.db_path = Some(PathBuf::from("/path/to/your/client_db"));
    }
    config.save(path)
}
