// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use ethers::providers::Middleware;
use fastcrypto::encoding::{Encoding, Hex};
use shared_crypto::intent::Intent;
use shared_crypto::intent::IntentMessage;
use std::collections::HashMap;
use std::str::from_utf8;
use std::sync::Arc;
use std::time::Duration;
use sui_bridge::client::bridge_authority_aggregator::BridgeAuthorityAggregator;
use sui_bridge::crypto::{BridgeAuthorityPublicKey, BridgeAuthorityPublicKeyBytes};
use sui_bridge::eth_transaction_builder::build_eth_transaction;
use sui_bridge::sui_client::SuiClient;
use sui_bridge::sui_transaction_builder::build_sui_transaction;
use sui_bridge::utils::get_eth_contract_addresses;
use sui_bridge::utils::{
    examine_key, generate_bridge_authority_key_and_write_to_file,
    generate_bridge_client_key_and_write_to_file, generate_bridge_node_config_and_write_to_file,
};
use sui_bridge_cli::{
    make_action, select_contract_address, Args, BridgeCliConfig, BridgeCommand,
    LoadedBridgeCliConfig,
};
use sui_config::Config;
use sui_sdk::SuiClient as SuiSdkClient;
use sui_sdk::SuiClientBuilder;
use sui_types::bridge::BridgeChainId;
use sui_types::bridge::{MoveTypeCommitteeMember, MoveTypeCommitteeMemberRegistration};
use sui_types::committee::TOTAL_VOTING_POWER;
use sui_types::crypto::AuthorityPublicKeyBytes;
use sui_types::crypto::Signature;
use sui_types::crypto::ToFromBytes;
use sui_types::transaction::Transaction;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Init logging
    let (_guard, _filter_handle) = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();
    let args = Args::parse();

    match args.command {
        BridgeCommand::CreateBridgeValidatorKey { path } => {
            generate_bridge_authority_key_and_write_to_file(&path)?;
            println!("Bridge validator key generated at {}", path.display());
        }
        BridgeCommand::CreateBridgeClientKey { path, use_ecdsa } => {
            generate_bridge_client_key_and_write_to_file(&path, use_ecdsa)?;
            println!("Bridge client key generated at {}", path.display());
        }
        BridgeCommand::ExamineKey {
            path,
            is_validator_key,
        } => {
            examine_key(&path, is_validator_key)?;
        }
        BridgeCommand::CreateBridgeNodeConfigTemplate { path, run_client } => {
            generate_bridge_node_config_and_write_to_file(&path, run_client)?;
            println!(
                "Bridge node config template generated at {}",
                path.display()
            );
        }

        BridgeCommand::Governance {
            config_path,
            chain_id,
            cmd,
            dry_run,
        } => {
            let chain_id = BridgeChainId::try_from(chain_id).expect("Invalid chain id");
            println!("Chain ID: {:?}", chain_id);
            let config = BridgeCliConfig::load(config_path).expect("Couldn't load BridgeCliConfig");
            let config = LoadedBridgeCliConfig::load(config).await?;
            let sui_bridge_client = SuiClient::<SuiSdkClient>::new(&config.sui_rpc_url).await?;

            let (sui_key, sui_address, gas_object_ref) = config
                .get_sui_account_info()
                .await
                .expect("Failed to get sui account info");
            let bridge_summary = sui_bridge_client
                .get_bridge_summary()
                .await
                .expect("Failed to get bridge summary");
            let bridge_committee = Arc::new(
                sui_bridge_client
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
                let certified_action = agg
                    .request_committee_signatures(sui_action)
                    .await
                    .expect("Failed to request committee signatures");
                if dry_run {
                    println!("Dryrun succeeded.");
                    return Ok(());
                }
                let bridge_arg = sui_bridge_client
                    .get_mutable_bridge_object_arg_must_succeed()
                    .await;
                let rgp = sui_bridge_client
                    .get_reference_gas_price_until_success()
                    .await;
                let id_token_map = sui_bridge_client.get_token_id_map().await.unwrap();
                let tx = build_sui_transaction(
                    sui_address,
                    &gas_object_ref,
                    certified_action,
                    bridge_arg,
                    &id_token_map,
                    rgp,
                )
                .expect("Failed to build sui transaction");
                let sui_sig = Signature::new_secure(
                    &IntentMessage::new(Intent::sui_transaction(), tx.clone()),
                    &sui_key,
                );
                let tx = Transaction::from_data(tx, vec![sui_sig]);
                let resp = sui_bridge_client
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
            let eth_signer_client = config.eth_signer();
            // Create BridgeAction
            let eth_action = make_action(chain_id, &cmd);
            println!("Action to execute on Eth: {:?}", eth_action);
            // Create Eth Signer Client
            // TODO if a validator is blocklisted on eth, ignore their signatures?
            let certified_action = agg
                .request_committee_signatures(eth_action)
                .await
                .expect("Failed to request committee signatures");
            if dry_run {
                println!("Dryrun succeeded.");
                return Ok(());
            }
            let contract_address = select_contract_address(&config, &cmd);
            let tx = build_eth_transaction(
                contract_address,
                eth_signer_client.clone(),
                certified_action,
            )
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

        BridgeCommand::PrintEthBridgeAddresses {
            bridge_proxy,
            eth_rpc_url,
        } => {
            let provider = Arc::new(
                ethers::prelude::Provider::<ethers::providers::Http>::try_from(eth_rpc_url)
                    .unwrap()
                    .interval(std::time::Duration::from_millis(2000)),
            );
            let chain_id = provider.get_chainid().await?;
            let (committee_address, limiter_address, vault_address, config_address) =
                get_eth_contract_addresses(bridge_proxy, &provider).await?;
            println!("Chain ID: {:?}", chain_id);
            println!("Committee Proxy Address: {:?}", committee_address);
            println!("Limiter Proxy Address: {:?}", limiter_address);
            println!("Config Proxy Address: {:?}", config_address);
            println!("Vault Address: {:?}", vault_address);
            return Ok(());
        }

        BridgeCommand::PrintBridgeRegistrationInfo { sui_rpc_url } => {
            let sui_bridge_client = SuiClient::<SuiSdkClient>::new(&sui_rpc_url).await?;
            let bridge_summary = sui_bridge_client
                .get_bridge_summary()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get bridge summary: {:?}", e))?;
            let move_type_bridge_committee = bridge_summary.committee;
            let sui_client = SuiClientBuilder::default().build(sui_rpc_url).await?;
            let stakes = sui_client
                .governance_api()
                .get_committee_info(None)
                .await?
                .validators
                .into_iter()
                .collect::<HashMap<_, _>>();
            let names = sui_client
                .governance_api()
                .get_latest_sui_system_state()
                .await?
                .active_validators
                .into_iter()
                .map(|summary| {
                    let protocol_key =
                        AuthorityPublicKeyBytes::from_bytes(&summary.protocol_pubkey_bytes)
                            .unwrap();
                    (summary.sui_address, (protocol_key, summary.name))
                })
                .collect::<HashMap<_, _>>();
            let mut authorities = vec![];
            for (_, member) in move_type_bridge_committee.member_registration {
                let MoveTypeCommitteeMemberRegistration {
                    sui_address,
                    bridge_pubkey_bytes,
                    http_rest_url,
                } = member;
                let Ok(pubkey) = BridgeAuthorityPublicKey::from_bytes(&bridge_pubkey_bytes) else {
                    println!(
                        "Invalid bridge pubkey for validator {}: {:?}",
                        sui_address, bridge_pubkey_bytes
                    );
                    continue;
                };
                let eth_address = BridgeAuthorityPublicKeyBytes::from(&pubkey).to_eth_address();
                let Ok(url) = from_utf8(&http_rest_url) else {
                    println!(
                        "Invalid bridge http url for validator: {}: {:?}",
                        sui_address, http_rest_url
                    );
                    continue;
                };
                let url = url.to_string();

                let (protocol_key, name) = names.get(&sui_address).unwrap();
                let stake = stakes.get(protocol_key).unwrap();
                authorities.push((name, sui_address, pubkey, eth_address, url, stake));
            }
            let total_stake = authorities
                .iter()
                .map(|(_, _, _, _, _, stake)| **stake)
                .sum::<u64>();
            println!(
                "Total registered stake: {}%",
                total_stake as f32 / TOTAL_VOTING_POWER as f32 * 100.0
            );
            println!("Name, SuiAddress, EthAddress, Pubkey, URL, Stake");
            for (name, sui_address, pubkey, eth_address, url, stake) in authorities {
                println!(
                    "{}, {}, {}, {}, {}, {}",
                    name,
                    sui_address,
                    eth_address,
                    Hex::encode(pubkey.as_bytes()),
                    url,
                    stake
                );
            }
        }

        BridgeCommand::PrintBridgeCommitteeInfo {
            sui_rpc_url,
            hex,
            ping,
        } => {
            let sui_bridge_client = SuiClient::<SuiSdkClient>::new(&sui_rpc_url).await?;
            let bridge_summary = sui_bridge_client
                .get_bridge_summary()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get bridge summary: {:?}", e))?;
            let move_type_bridge_committee = bridge_summary.committee;
            let sui_client = SuiClientBuilder::default().build(sui_rpc_url).await?;
            let names = sui_client
                .governance_api()
                .get_latest_sui_system_state()
                .await?
                .active_validators
                .into_iter()
                .map(|summary| (summary.sui_address, summary.name))
                .collect::<HashMap<_, _>>();
            let mut authorities = vec![];
            let mut ping_tasks = vec![];
            let client = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap();
            for (_, member) in move_type_bridge_committee.members {
                let MoveTypeCommitteeMember {
                    sui_address,
                    bridge_pubkey_bytes,
                    voting_power,
                    http_rest_url,
                    blocklisted,
                } = member;
                let Ok(pubkey) = BridgeAuthorityPublicKey::from_bytes(&bridge_pubkey_bytes) else {
                    println!(
                        "Invalid bridge pubkey for validator {}: {:?}",
                        sui_address, bridge_pubkey_bytes
                    );
                    continue;
                };
                let eth_address = BridgeAuthorityPublicKeyBytes::from(&pubkey).to_eth_address();
                let Ok(url) = from_utf8(&http_rest_url) else {
                    println!(
                        "Invalid bridge http url for validator: {}: {:?}",
                        sui_address, http_rest_url
                    );
                    continue;
                };
                let url = url.to_string();

                let name = names.get(&sui_address).unwrap();
                if ping {
                    let client_clone = client.clone();
                    ping_tasks.push(client_clone.get(url.clone()).send());
                }
                authorities.push((
                    name,
                    sui_address,
                    pubkey,
                    eth_address,
                    url,
                    voting_power,
                    blocklisted,
                ));
            }
            let total_stake = authorities
                .iter()
                .map(|(_, _, _, _, _, stake, _)| *stake)
                .sum::<u64>();
            println!(
                "Total stake (static): {}%",
                total_stake as f32 / TOTAL_VOTING_POWER as f32 * 100.0
            );
            let ping_tasks_resp = if !ping_tasks.is_empty() {
                futures::future::join_all(ping_tasks)
                    .await
                    .into_iter()
                    .map(|resp| {
                        Some(match resp {
                            Ok(resp) => resp.status().is_success(),
                            Err(_e) => false,
                        })
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![None; authorities.len()]
            };
            if ping {
                println!(
                    "Name, SuiAddress, EthAddress, Pubkey, URL, Stake, Blocklisted, PingStatus"
                );
            } else {
                println!("Name, SuiAddress, EthAddress, Pubkey, URL, Stake, Blocklisted");
            }
            let mut total_online_stake = 0;
            for ((name, sui_address, pubkey, eth_address, url, stake, blocklisted), ping_resp) in
                authorities.into_iter().zip(ping_tasks_resp)
            {
                let pubkey = if hex {
                    Hex::encode(pubkey.as_bytes())
                } else {
                    pubkey.to_string()
                };
                match ping_resp {
                    Some(resp) => {
                        if resp {
                            total_online_stake += stake;
                        }
                        println!(
                            "{}, {}, 0x{:x}, {}, {}, {}, {}, {}",
                            name,
                            sui_address,
                            eth_address,
                            pubkey,
                            url,
                            stake,
                            blocklisted,
                            if resp { "online" } else { "offline" }
                        );
                    }
                    None => println!(
                        "{}, {}, 0x{:x}, {}, {}, {}, {}",
                        name, sui_address, eth_address, pubkey, url, stake, blocklisted
                    ),
                }
            }
            if ping {
                println!(
                    "Total online stake (static): {}%",
                    total_online_stake as f32 / TOTAL_VOTING_POWER as f32 * 100.0
                );
            }
        }
        BridgeCommand::Client { config_path, cmd } => {
            let config = BridgeCliConfig::load(config_path).expect("Couldn't load BridgeCliConfig");
            let config = LoadedBridgeCliConfig::load(config).await?;
            let sui_bridge_client = SuiClient::<SuiSdkClient>::new(&config.sui_rpc_url).await?;
            cmd.handle(&config, sui_bridge_client).await?;
            return Ok(());
        }
    }

    Ok(())
}
