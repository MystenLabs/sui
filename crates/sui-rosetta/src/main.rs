// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::anyhow;
use clap::Parser;
use fastcrypto::encoding::{Encoding, Hex};
use serde_json::{json, Value};
use tracing::info;
use tracing::log::warn;

use sui_config::{sui_config_dir, Config, NodeConfig, SUI_FULLNODE_CONFIG, SUI_KEYSTORE_FILENAME};
use sui_node::{metrics, SuiNode};
use sui_rosetta::types::{CurveType, PrefundedAccount, SuiEnv};
use sui_rosetta::{RosettaOfflineServer, RosettaOnlineServer, SUI};
use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{EncodeDecodeBase64, KeypairTraits, SuiKeyPair, ToFromBytes};

#[derive(Parser)]
#[clap(name = "sui-rosetta", rename_all = "kebab-case", author, version)]
pub enum RosettaServerCommand {
    GenerateRosettaCLIConfig {
        #[clap(long)]
        keystore_path: Option<PathBuf>,
        #[clap(long, default_value = "localnet")]
        env: SuiEnv,
        #[clap(long, default_value = "http://rosetta-online:9002")]
        online_url: String,
        #[clap(long, default_value = "http://rosetta-offline:9003")]
        offline_url: String,
    },
    StartOnlineRemoteServer {
        #[clap(long, default_value = "localnet")]
        env: SuiEnv,
        #[clap(long, default_value = "0.0.0.0:9002")]
        addr: SocketAddr,
        #[clap(long)]
        full_node_url: String,
        #[clap(long, default_value = "/data")]
        data_path: PathBuf,
    },
    StartOnlineServer {
        #[clap(long, default_value = "localnet")]
        env: SuiEnv,
        #[clap(long, default_value = "0.0.0.0:9002")]
        addr: SocketAddr,
        #[clap(long)]
        node_config: Option<PathBuf>,
        #[clap(long, default_value = "/data")]
        data_path: PathBuf,
    },
    StartOfflineServer {
        #[clap(long, default_value = "localnet")]
        env: SuiEnv,
        #[clap(long, default_value = "0.0.0.0:9003")]
        addr: SocketAddr,
    },
}

impl RosettaServerCommand {
    async fn execute(self) -> Result<(), anyhow::Error> {
        match self {
            RosettaServerCommand::GenerateRosettaCLIConfig {
                keystore_path,
                env,
                online_url,
                offline_url,
            } => {
                let path = keystore_path
                    .unwrap_or_else(|| sui_config_dir().unwrap().join(SUI_KEYSTORE_FILENAME));

                let prefunded_accounts = read_prefunded_account(&path)?;

                info!(
                    "Retrieved {} Sui address from keystore file {:?}",
                    prefunded_accounts.len(),
                    &path
                );

                let mut config: Value =
                    serde_json::from_str(include_str!("../resources/rosetta_cli.json"))?;

                config
                    .as_object_mut()
                    .unwrap()
                    .insert("online_url".into(), json!(online_url));

                // Set network.
                let network = config.pointer_mut("/network").ok_or_else(|| {
                    anyhow!("Cannot find construction config in default config file.")
                })?;
                network
                    .as_object_mut()
                    .unwrap()
                    .insert("network".into(), json!(env));

                // Add prefunded accounts.
                let construction = config.pointer_mut("/construction").ok_or_else(|| {
                    anyhow!("Cannot find construction config in default config file.")
                })?;

                let construction = construction.as_object_mut().unwrap();
                construction.insert("prefunded_accounts".into(), json!(prefunded_accounts));
                construction.insert("offline_url".into(), json!(offline_url));

                let config_path = PathBuf::from(".").join("rosetta_cli.json");
                fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;
                info!(
                    "Rosetta CLI configuration file is stored in {:?}",
                    config_path
                );

                let dsl_path = PathBuf::from(".").join("sui.ros");
                let dsl = include_str!("../resources/sui.ros");
                fs::write(
                    &dsl_path,
                    dsl.replace("{{sui.env}}", json!(env).as_str().unwrap()),
                )?;
                info!("Rosetta DSL file is stored in {:?}", dsl_path);
            }
            RosettaServerCommand::StartOfflineServer { env, addr } => {
                info!("Starting Rosetta Offline Server.");
                let server = RosettaOfflineServer::new(env);
                server.serve(addr).await??;
            }
            RosettaServerCommand::StartOnlineRemoteServer {
                env,
                addr,
                full_node_url,
                data_path,
            } => {
                info!(
                    "Starting Rosetta Online Server with remove Sui full node [{full_node_url}]."
                );
                let sui_client = wait_for_sui_client(full_node_url).await;
                let rosetta_path = data_path.join("rosetta_db");
                info!("Rosetta db path : {rosetta_path:?}");
                let rosetta = RosettaOnlineServer::new(env, sui_client, &rosetta_path);
                rosetta.serve(addr).await??;
            }

            RosettaServerCommand::StartOnlineServer {
                env,
                addr,
                node_config,
                data_path,
            } => {
                info!("Starting Rosetta Online Server with embedded Sui full node.");
                info!("Data directory path: {data_path:?}");

                let node_config = node_config.unwrap_or_else(|| {
                    let path = sui_config_dir().unwrap().join(SUI_FULLNODE_CONFIG);
                    info!("Using default node config from {path:?}");
                    path
                });

                let mut config = NodeConfig::load(&node_config)?;
                config.db_path = data_path.join("sui_db");
                info!("Overriding Sui db path to : {:?}", config.db_path);

                let registry_service = metrics::start_prometheus_server(config.metrics_address);
                // Staring a full node for the rosetta server.
                let rpc_address = format!("http://127.0.0.1:{}", config.json_rpc_address.port());
                let _node = SuiNode::start(&config, registry_service, None).await?;

                let sui_client = wait_for_sui_client(rpc_address).await;

                let rosetta_path = data_path.join("rosetta_db");
                info!("Rosetta db path : {rosetta_path:?}");
                let rosetta = RosettaOnlineServer::new(env, sui_client, &rosetta_path);
                rosetta.serve(addr).await??;
            }
        };
        Ok(())
    }
}

async fn wait_for_sui_client(rpc_address: String) -> SuiClient {
    loop {
        match SuiClientBuilder::default()
            .max_concurrent_requests(usize::MAX)
            .build(&rpc_address)
            .await
        {
            Ok(client) => return client,
            Err(e) => {
                warn!("Error connecting to Sui RPC server [{rpc_address}]: {e}, retrying in 5 seconds.");
                tokio::time::sleep(Duration::from_millis(5000)).await;
            }
        }
    }
}

/// This method reads the keypairs from the Sui keystore to create the PrefundedAccount objects,
/// PrefundedAccount will be written to the rosetta-cli config file for testing.
///
fn read_prefunded_account(path: &Path) -> Result<Vec<PrefundedAccount>, anyhow::Error> {
    let reader = BufReader::new(File::open(path).unwrap());
    let kp_strings: Vec<String> = serde_json::from_reader(reader).unwrap();
    let keys = kp_strings
        .iter()
        .map(|kpstr| {
            let key = SuiKeyPair::decode_base64(kpstr);
            key.map(|k| (Into::<SuiAddress>::into(&k.public()), k))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()
        .unwrap();

    Ok(keys
        .into_iter()
        .map(|(address, key)| {
            let (privkey, curve_type) = match key {
                SuiKeyPair::Ed25519(k) => {
                    (Hex::encode(k.private().as_bytes()), CurveType::Edwards25519)
                }
                SuiKeyPair::Secp256k1(k) => {
                    (Hex::encode(k.private().as_bytes()), CurveType::Secp256k1)
                }
                SuiKeyPair::Secp256r1(k) => {
                    (Hex::encode(k.private().as_bytes()), CurveType::Secp256r1)
                }
            };
            PrefundedAccount {
                privkey,
                account_identifier: address.into(),
                curve_type,
                currency: SUI.clone(),
            }
        })
        .collect())
}

#[test]
fn test_read_keystore() {
    use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
    use sui_types::crypto::SignatureScheme;

    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("sui.keystore");
    let mut ks = Keystore::from(FileBasedKeystore::new(&path).unwrap());
    let key1 = ks
        .generate_and_add_new_key(SignatureScheme::ED25519, None, None)
        .unwrap();
    let key2 = ks
        .generate_and_add_new_key(SignatureScheme::Secp256k1, None, None)
        .unwrap();

    let accounts = read_prefunded_account(&path).unwrap();
    let acc_map = accounts
        .into_iter()
        .map(|acc| (acc.account_identifier.address, acc))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(2, acc_map.len());
    assert!(acc_map.contains_key(&key1.0));
    assert!(acc_map.contains_key(&key2.0));

    let acc1 = acc_map[&key1.0].clone();
    let acc2 = acc_map[&key2.0].clone();

    let schema1: SignatureScheme = acc1.curve_type.into();
    let schema2: SignatureScheme = acc2.curve_type.into();
    assert!(matches!(schema1, SignatureScheme::ED25519));
    assert!(matches!(schema2, SignatureScheme::Secp256k1));
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cmd: RosettaServerCommand = RosettaServerCommand::parse();

    let (_guard, _) = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    cmd.execute().await
}
