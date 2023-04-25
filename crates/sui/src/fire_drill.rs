// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A tool to semi automate fire drills. It still requires some manual work today. For example,
//! 1. update iptables for new tpc/udp ports
//! 2. restart the node in a new epoch when config file will be reloaded and take effects
//!
//! Example usage:
//! sui fire-drill metadata-rotation \
//! --sui-node-config-path validator.yaml \
//! --account-key-path account.key \
//! --fullnode-rpc-url http://fullnode-my-local-net:9000

use anyhow::bail;
use clap::*;
use fastcrypto::ed25519::Ed25519KeyPair;
use fastcrypto::traits::{KeyPair, ToFromBytes};
use move_core_types::ident_str;
use shared_crypto::intent::Intent;
use std::path::{Path, PathBuf};
use sui_config::node::KeyPairWithPath;
use sui_config::utils;
use sui_config::{node::AuthorityKeyPairWithPath, Config, NodeConfig, PersistedConfig};
use sui_json_rpc_types::{SuiExecutionStatus, SuiTransactionBlockResponseOptions};
use sui_keys::keypair_file::read_keypair_from_file;
use sui_sdk::{rpc_types::SuiTransactionBlockEffectsAPI, SuiClient, SuiClientBuilder};
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::crypto::{generate_proof_of_possession, get_key_pair, SuiKeyPair};
use sui_types::messages::{
    CallArg, ObjectArg, Transaction, TransactionData, TEST_ONLY_GAS_UNIT_FOR_GENERIC,
};
use sui_types::multiaddr::{Multiaddr, Protocol};
use sui_types::{committee::EpochId, crypto::get_authority_key_pair, SUI_SYSTEM_OBJECT_ID};
use sui_types::{SUI_SYSTEM_STATE_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION};
use tracing::info;

#[derive(Parser)]
pub enum FireDrill {
    MetadataRotation(MetadataRotation),
}

#[derive(Parser)]
pub struct MetadataRotation {
    /// Path to sui node config.
    #[clap(long = "sui-node-config-path")]
    sui_node_config_path: PathBuf,
    /// Path to account key file.
    #[clap(long = "account-key-path")]
    account_key_path: PathBuf,
    /// Jsonrpc url for a reliable fullnode.
    #[clap(long = "fullnode-rpc-url")]
    fullnode_rpc_url: String,
}

pub async fn run_fire_drill(fire_drill: FireDrill) -> anyhow::Result<()> {
    match fire_drill {
        FireDrill::MetadataRotation(metadata_rotation) => {
            run_metadata_rotation(metadata_rotation).await?;
        }
    }
    Ok(())
}

async fn run_metadata_rotation(metadata_rotation: MetadataRotation) -> anyhow::Result<()> {
    let MetadataRotation {
        sui_node_config_path,
        account_key_path,
        fullnode_rpc_url,
    } = metadata_rotation;
    let account_key = read_keypair_from_file(&account_key_path)?;
    let config: NodeConfig = PersistedConfig::read(&sui_node_config_path).map_err(|err| {
        err.context(format!(
            "Cannot open Sui Node Config file at {:?}",
            sui_node_config_path
        ))
    })?;

    let sui_client = SuiClientBuilder::default().build(fullnode_rpc_url).await?;
    let sui_address = SuiAddress::from(&account_key.public());
    let starting_epoch = current_epoch(&sui_client).await?;
    info!("Running Metadata Rotation fire drill for validator address {sui_address} in epoch {starting_epoch}.");

    // Prepare new metadata for next epoch
    let new_config_path =
        update_next_epoch_metadata(&sui_node_config_path, &config, &sui_client, &account_key)
            .await?;

    let current_epoch = current_epoch(&sui_client).await?;
    if current_epoch > starting_epoch {
        bail!("Epoch already advanced to {current_epoch}");
    }
    let target_epoch = starting_epoch + 1;
    wait_for_next_epoch(&sui_client, target_epoch).await?;
    info!("Just advanced to epoch {target_epoch}");

    // Replace new config
    std::fs::rename(new_config_path, sui_node_config_path)?;
    info!("Updated Sui Node config.");

    Ok(())
}

// TODO move this to a shared lib
pub async fn get_gas_obj_ref(
    sui_address: SuiAddress,
    sui_client: &SuiClient,
    minimal_gas_balance: u64,
) -> anyhow::Result<ObjectRef> {
    let coins = sui_client
        .coin_read_api()
        .get_coins(sui_address, Some("0x2::sui::SUI".into()), None, None)
        .await?
        .data;
    let gas_obj = coins.iter().find(|c| c.balance >= minimal_gas_balance);
    if gas_obj.is_none() {
        bail!("Validator doesn't have enough Sui coins to cover transaction fees.");
    }
    Ok(gas_obj.unwrap().object_ref())
}

async fn update_next_epoch_metadata(
    sui_node_config_path: &Path,
    config: &NodeConfig,
    sui_client: &SuiClient,
    account_key: &SuiKeyPair,
) -> anyhow::Result<PathBuf> {
    // Save backup config just in case
    let mut backup_config_path = sui_node_config_path.to_path_buf();
    backup_config_path.pop();
    backup_config_path.push("node_config_backup.yaml");
    let backup_config = config.clone();
    backup_config.persisted(&backup_config_path).save()?;

    let sui_address = SuiAddress::from(&account_key.public());

    let mut new_config = config.clone();

    // protocol key
    let new_protocol_key_pair = get_authority_key_pair().1;
    let new_protocol_key_pair_copy = new_protocol_key_pair.copy();
    let pop = generate_proof_of_possession(&new_protocol_key_pair, sui_address);
    new_config.protocol_key_pair = AuthorityKeyPairWithPath::new(new_protocol_key_pair);

    // network key
    let new_network_key_pair: Ed25519KeyPair = get_key_pair().1;
    let new_network_key_pair_copy = new_network_key_pair.copy();
    new_config.network_key_pair = KeyPairWithPath::new(SuiKeyPair::Ed25519(new_network_key_pair));

    // worker key
    let new_worker_key_pair: Ed25519KeyPair = get_key_pair().1;
    let new_worker_key_pair_copy = new_worker_key_pair.copy();
    new_config.worker_key_pair = KeyPairWithPath::new(SuiKeyPair::Ed25519(new_worker_key_pair));

    let validators = sui_client
        .governance_api()
        .get_latest_sui_system_state()
        .await?
        .active_validators;
    let self_validator = validators
        .iter()
        .find(|v| v.sui_address == sui_address)
        .unwrap();

    // Network address
    let mut new_network_address = Multiaddr::try_from(self_validator.net_address.clone()).unwrap();
    info!("Current network address: {:?}", new_network_address);
    let http = new_network_address.pop().unwrap();
    // pop out tcp
    new_network_address.pop().unwrap();
    let new_port = utils::get_available_port("127.0.0.1");
    new_network_address.push(Protocol::Tcp(new_port));
    new_network_address.push(http);
    info!("New network address: {:?}", new_network_address);
    new_config.network_address = new_network_address.clone();

    // p2p address
    let mut new_external_address = config.p2p_config.external_address.clone().unwrap();
    info!("Current P2P external address: {:?}", new_external_address);
    // pop out udp
    new_external_address.pop().unwrap();
    let new_port = utils::get_available_port("127.0.0.1");
    new_external_address.push(Protocol::Udp(new_port));
    info!("New P2P external address: {:?}", new_external_address);
    new_config.p2p_config.external_address = Some(new_external_address.clone());

    let mut new_listen_address = config.p2p_config.listen_address;
    info!("Current P2P local listen address: {:?}", new_listen_address);
    new_listen_address.set_port(new_port);
    info!("New P2P local listen address: {:?}", new_listen_address);
    new_config.p2p_config.listen_address = new_listen_address;

    // primary address
    let mut new_primary_addresses =
        Multiaddr::try_from(self_validator.primary_address.clone()).unwrap();
    info!("Current primary address: {:?}", new_primary_addresses);
    // pop out udp
    new_primary_addresses.pop().unwrap();
    let new_port = utils::get_available_port("127.0.0.1");
    new_primary_addresses.push(Protocol::Udp(new_port));
    info!("New primary address: {:?}", new_primary_addresses);

    // worker address
    let mut new_worker_addresses = Multiaddr::try_from(
        validators
            .iter()
            .find(|v| v.sui_address == sui_address)
            .unwrap()
            .worker_address
            .clone(),
    )
    .unwrap();
    info!("Current worker address: {:?}", new_worker_addresses);
    // pop out udp
    new_worker_addresses.pop().unwrap();
    let new_port = utils::get_available_port("127.0.0.1");
    new_worker_addresses.push(Protocol::Udp(new_port));
    info!("New worker address:: {:?}", new_worker_addresses);

    // Save new config
    let mut new_config_path = sui_node_config_path.to_path_buf();
    new_config_path.pop();
    new_config_path.push(
        String::from(sui_node_config_path.file_name().unwrap().to_str().unwrap()) + ".next_epoch",
    );
    new_config.persisted(&new_config_path).save()?;

    // update protocol pubkey on chain
    update_metadata_on_chain(
        account_key,
        "update_validator_next_epoch_protocol_pubkey",
        vec![
            CallArg::Pure(
                bcs::to_bytes(&new_protocol_key_pair_copy.public().as_bytes().to_vec()).unwrap(),
            ),
            CallArg::Pure(bcs::to_bytes(&pop.as_bytes().to_vec()).unwrap()),
        ],
        sui_client,
    )
    .await?;

    // update network pubkey on chain
    update_metadata_on_chain(
        account_key,
        "update_validator_next_epoch_network_pubkey",
        vec![CallArg::Pure(
            bcs::to_bytes(&new_network_key_pair_copy.public().as_bytes().to_vec()).unwrap(),
        )],
        sui_client,
    )
    .await?;

    // update worker pubkey on chain
    update_metadata_on_chain(
        account_key,
        "update_validator_next_epoch_worker_pubkey",
        vec![CallArg::Pure(
            bcs::to_bytes(&new_worker_key_pair_copy.public().as_bytes().to_vec()).unwrap(),
        )],
        sui_client,
    )
    .await?;

    // update network address
    update_metadata_on_chain(
        account_key,
        "update_validator_next_epoch_network_address",
        vec![CallArg::Pure(bcs::to_bytes(&new_network_address).unwrap())],
        sui_client,
    )
    .await?;

    // update p2p address
    update_metadata_on_chain(
        account_key,
        "update_validator_next_epoch_p2p_address",
        vec![CallArg::Pure(bcs::to_bytes(&new_external_address).unwrap())],
        sui_client,
    )
    .await?;

    // update primary address
    update_metadata_on_chain(
        account_key,
        "update_validator_next_epoch_primary_address",
        vec![CallArg::Pure(
            bcs::to_bytes(&new_primary_addresses).unwrap(),
        )],
        sui_client,
    )
    .await?;

    // update worker address
    update_metadata_on_chain(
        account_key,
        "update_validator_next_epoch_worker_address",
        vec![CallArg::Pure(bcs::to_bytes(&new_worker_addresses).unwrap())],
        sui_client,
    )
    .await?;

    Ok(new_config_path)
}

async fn update_metadata_on_chain(
    account_key: &SuiKeyPair,
    function: &'static str,
    call_args: Vec<CallArg>,
    sui_client: &SuiClient,
) -> anyhow::Result<()> {
    let sui_address = SuiAddress::from(&account_key.public());
    let gas_obj_ref = get_gas_obj_ref(sui_address, sui_client, 10000 * 100).await?;
    let rgp = sui_client
        .governance_api()
        .get_reference_gas_price()
        .await?;
    let mut args = vec![CallArg::Object(ObjectArg::SharedObject {
        id: SUI_SYSTEM_STATE_OBJECT_ID,
        initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
        mutable: true,
    })];
    args.extend(call_args);
    let tx_data = TransactionData::new_move_call(
        sui_address,
        SUI_SYSTEM_OBJECT_ID,
        ident_str!("sui_system").to_owned(),
        ident_str!(function).to_owned(),
        vec![],
        gas_obj_ref,
        args,
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        rgp,
    )
    .unwrap();
    execute_tx(account_key, sui_client, tx_data, function).await?;
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    Ok(())
}

async fn execute_tx(
    account_key: &SuiKeyPair,
    sui_client: &SuiClient,
    tx_data: TransactionData,
    action: &str,
) -> anyhow::Result<()> {
    let tx =
        Transaction::from_data_and_signer(tx_data, Intent::sui_transaction(), vec![account_key])
            .verify()?;
    info!("Executing {:?}", tx.digest());
    let tx_digest = *tx.digest();
    let resp = sui_client
        .quorum_driver_api()
        .execute_transaction_block(
            tx,
            SuiTransactionBlockResponseOptions::full_content(),
            Some(sui_types::messages::ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .unwrap();
    if *resp.effects.unwrap().status() != SuiExecutionStatus::Success {
        anyhow::bail!("Tx to update metadata {:?} failed", tx_digest);
    }
    info!("{action} succeeded");
    Ok(())
}

async fn wait_for_next_epoch(sui_client: &SuiClient, target_epoch: EpochId) -> anyhow::Result<()> {
    loop {
        let epoch_id = current_epoch(sui_client).await?;
        if epoch_id > target_epoch {
            bail!(
                "Current epoch ID {} is higher than target {}, likely something is off.",
                epoch_id,
                target_epoch
            );
        }
        if epoch_id == target_epoch {
            return Ok(());
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

async fn current_epoch(sui_client: &SuiClient) -> anyhow::Result<EpochId> {
    Ok(sui_client.read_api().get_committee_info(None).await?.epoch)
}
