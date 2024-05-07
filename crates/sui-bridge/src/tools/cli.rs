// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use clap::*;
use fastcrypto::ed25519::Ed25519KeyPair;
use fastcrypto::secp256k1::Secp256k1KeyPair;
use fastcrypto::traits::EncodeDecodeBase64;
use std::path::PathBuf;
use sui_bridge::config::BridgeNodeConfig;
use sui_bridge::crypto::BridgeAuthorityKeyPair;
use sui_bridge::crypto::BridgeAuthorityPublicKeyBytes;
use sui_config::Config;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::get_key_pair;
use sui_types::crypto::SuiKeyPair;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
struct Args {
    #[clap(subcommand)]
    command: BridgeValidatorCommand,
}

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum BridgeValidatorCommand {
    #[clap(name = "create-bridge-validator-key")]
    CreateBridgeValidatorKey { path: PathBuf },
    #[clap(name = "create-bridge-client-key")]
    CreateBridgeClientKey {
        path: PathBuf,
        #[clap(name = "use-ecdsa", long)]
        use_ecdsa: bool,
    },
    #[clap(name = "create-bridge-node-config-template")]
    CreateBridgeNodeConfigTemplate {
        path: PathBuf,
        #[clap(name = "run-client", long)]
        run_client: bool,
    },
}

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
