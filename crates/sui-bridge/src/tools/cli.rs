// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use clap::*;
use fastcrypto::encoding::{Base64, Encoding};
use fastcrypto::secp256k1::{Secp256k1KeyPair, Secp256k1PrivateKey};
use fastcrypto::traits::{KeyPair, ToFromBytes};
use move_core_types::identifier::Identifier;
use shared_crypto::intent::Intent;
use shared_crypto::intent::IntentMessage;
use std::fs;
use std::sync::Arc;
use sui_bridge::client::bridge_authority_aggregator::BridgeAuthorityAggregator;
use sui_bridge::config::BridgeNodeConfig;
use sui_bridge::eth_transaction_builder::build_eth_transaction;
use sui_bridge::sui_client::SuiClient;
use sui_bridge::sui_transaction_builder::build_sui_transaction;
use sui_bridge::tools::{
    make_action, select_contract_address, Args, BridgeCliConfig, BridgeValidatorCommand,
};
use sui_bridge::utils::{
    generate_bridge_authority_key_and_write_to_file, generate_bridge_client_key_and_write_to_file,
    generate_bridge_node_config_and_write_to_file,
};
use sui_config::{Config, NodeConfig};
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_sdk::{SuiClient as SuiSdkClient, SuiClientBuilder};
use sui_types::base_types::SuiAddress;
use sui_types::bridge::{BridgeChainId, BRIDGE_MODULE_NAME};
use sui_types::crypto::Signature;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{ObjectArg, Transaction, TransactionData};
use sui_types::BRIDGE_PACKAGE_ID;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Init logging
    let (_guard, _filter_handle) = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();
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

        BridgeValidatorCommand::GovernanceClient {
            config_path,
            chain_id,
            cmd,
        } => {
            let chain_id = BridgeChainId::try_from(chain_id).expect("Invalid chain id");
            println!("Chain ID: {:?}", chain_id);
            let config = BridgeCliConfig::load(config_path).expect("Couldn't load BridgeCliConfig");
            let sui_client = SuiClient::<SuiSdkClient>::new(&config.sui_rpc_url).await?;

            let (sui_key, sui_address, gas_object_ref) = config
                .get_sui_account_info()
                .await
                .expect("Failed to get sui account info");
            let bridge_summary = sui_client
                .get_bridge_summary()
                .await
                .expect("Failed to get bridge summary");
            let bridge_committee = Arc::new(
                sui_client
                    .get_bridge_committee()
                    .await
                    .expect("Failed to get bridge committee"),
            );
            let agg = BridgeAuthorityAggregator::new(bridge_committee);

            // Handle Sui Side
            if chain_id.is_sui_chain() {
                let sui_chain_id = BridgeChainId::try_from(bridge_summary.chain_id).unwrap();
                assert_eq!(
                    sui_chain_id, chain_id,
                    "Chain ID mismatch, expected: {:?}, got from url: {:?}",
                    chain_id, sui_chain_id
                );
                // Create BridgeAction
                let sui_action = make_action(sui_chain_id, &cmd);
                println!("Action to execute on Sui: {:?}", sui_action);
                let threshold = sui_action.approval_threshold();
                let certified_action = agg
                    .request_committee_signatures(sui_action, threshold)
                    .await
                    .expect("Failed to request committee signatures");
                let bridge_arg = sui_client
                    .get_mutable_bridge_object_arg_must_succeed()
                    .await;
                let id_token_map = sui_client.get_token_id_map().await.unwrap();
                let tx = build_sui_transaction(
                    sui_address,
                    &gas_object_ref,
                    certified_action,
                    bridge_arg,
                    &id_token_map,
                )
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
                return Ok(());
            }

            // Handle eth side
            // TODO assert chain id returned from rpc matches chain_id
            let eth_signer_client = config
                .get_eth_signer_client()
                .await
                .expect("Failed to get eth signer client");
            println!("Using Eth address: {:?}", eth_signer_client.address());
            // Create BridgeAction
            let eth_action = make_action(chain_id, &cmd);
            println!("Action to execute on Eth: {:?}", eth_action);
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

            return Ok(());
        }
        BridgeValidatorCommand::CommitteeRegistration {
            node_config_path,
            bridge_node_config_path,
        } => {
            let node_config = NodeConfig::load(node_config_path).expect("Couldn't load NodeConfig");
            let bridge_config = BridgeNodeConfig::load(bridge_node_config_path)
                .expect("Couldn't load BridgeNodeConfig");

            // Read bridge keypair
            let key = fs::read_to_string(bridge_config.bridge_authority_key_path_base64_raw)?;
            let bytes = Base64::decode(&key).unwrap();
            let key = Secp256k1PrivateKey::from_bytes(&bytes).unwrap();
            let ecdsa_keypair = Secp256k1KeyPair::from(key);

            // Read sui node account key pair
            let keypair = node_config.account_key_pair.keypair();
            let address = SuiAddress::from(&keypair.public());
            println!("Starting bridge committee registration for Sui validator: {address}, with bridge public key: {}", ecdsa_keypair.public);

            let sui_client = SuiClientBuilder::default()
                .build(&bridge_config.sui_rpc_url)
                .await?;

            let bridge_client = SuiClient::new(&bridge_config.sui_rpc_url).await?;
            let bridge = bridge_client
                .get_mutable_bridge_object_arg()
                .await
                .expect("Cannot retrieve bridge object arg.");

            let coins = sui_client
                .coin_read_api()
                .get_coins(address, None, None, None)
                .await?;
            let gas = coins.data.first().unwrap().object_ref();
            let ref_gas_price = sui_client.read_api().get_reference_gas_price().await?;

            let mut ptb = ProgrammableTransactionBuilder::default();

            let bridge_arg = ptb.obj(bridge)?;
            let system_arg = ptb.obj(ObjectArg::SUI_SYSTEM_MUT)?;
            let pub_key_arg = ptb.pure(ecdsa_keypair.public().as_bytes())?;
            let url_arg = ptb.pure("")?;
            ptb.programmable_move_call(
                BRIDGE_PACKAGE_ID,
                BRIDGE_MODULE_NAME.into(),
                Identifier::new("committee_registration").unwrap(),
                vec![],
                vec![bridge_arg, system_arg, pub_key_arg, url_arg],
            );

            let tx_data = TransactionData::new_programmable(
                address,
                vec![gas],
                ptb.finish(),
                10000000,
                ref_gas_price,
            );
            let tx = Transaction::from_data_and_signer(tx_data, vec![keypair]);
            let response = sui_client
                .quorum_driver_api()
                .execute_transaction_block(
                    tx,
                    SuiTransactionBlockResponseOptions::new().with_effects(),
                    None,
                )
                .await?;
            if response.status_ok().unwrap() {
                println!(
                    "Committee registration successful. txn digest: {}",
                    response.digest
                )
            } else {
                return Err(anyhow!(
                    "Error returned from bridge committee registration transaction: {:?}",
                    response.effects.as_ref().map(|e| e.status())
                ));
            }
        }
    }

    Ok(())
}
