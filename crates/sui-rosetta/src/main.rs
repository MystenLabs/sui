// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::anyhow;
use clap::Parser;
use fastcrypto::encoding::{Encoding, Hex};
use serde_json::{json, Value};
use tracing::info;

use sui_config::{sui_config_dir, Config, NodeConfig, SUI_FULLNODE_CONFIG, SUI_KEYSTORE_FILENAME};
use sui_node::{metrics, SuiNode};
use sui_rosetta::types::{AccountIdentifier, CurveType, PrefundedAccount, SuiEnv};
use sui_rosetta::{RosettaOfflineServer, RosettaOnlineServer, SUI};
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
    },
    StartOnlineServer {
        #[clap(long, default_value = "localnet")]
        env: SuiEnv,
        #[clap(long, default_value = "0.0.0.0:9002")]
        addr: SocketAddr,
        #[clap(long)]
        node_config: Option<PathBuf>,
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
            RosettaServerCommand::GenerateRosettaCLIConfig { keystore_path, env } => {
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

                // Set network.
                let network = config.pointer_mut("/network").ok_or_else(|| {
                    anyhow!("Cannot find construction config in default config file.")
                })?;
                network
                    .as_object_mut()
                    .unwrap()
                    .insert("network".to_string(), json!(env));

                // Add prefunded accounts.
                let construction = config.pointer_mut("/construction").ok_or_else(|| {
                    anyhow!("Cannot find construction config in default config file.")
                })?;

                construction
                    .as_object_mut()
                    .unwrap()
                    .insert("prefunded_accounts".to_string(), json!(prefunded_accounts));

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
                let server = RosettaOfflineServer::new(env);
                server.serve(addr).await??;
            }
            RosettaServerCommand::StartOnlineServer {
                env,
                addr,
                node_config,
            } => {
                let node_config = node_config.unwrap_or_else(|| {
                    let path = sui_config_dir().unwrap().join(SUI_FULLNODE_CONFIG);
                    info!("Using default node config from {path:?}");
                    path
                });

                let config = NodeConfig::load(&node_config)?;
                let prometheus_registry = metrics::start_prometheus_server(config.metrics_address);
                // Staring a full node for the rosetta server.
                let node = SuiNode::start(&config, prometheus_registry).await?;
                let quorum_driver = node
                    .transaction_orchestrator()
                    .ok_or_else(|| anyhow!("Quorum driver is None"))?
                    .quorum_driver()
                    .clone();

                let rosetta =
                    RosettaOnlineServer::new(env, node.state(), quorum_driver, config.genesis()?);
                rosetta.serve(addr).await??;
            }
        };
        Ok(())
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
                SuiKeyPair::Ed25519SuiKeyPair(k) => {
                    (Hex::encode(k.private().as_bytes()), CurveType::Edwards25519)
                }
                SuiKeyPair::Secp256k1SuiKeyPair(k) => {
                    (Hex::encode(k.private().as_bytes()), CurveType::Secp256k1)
                }
            };
            PrefundedAccount {
                privkey,
                account_identifier: AccountIdentifier { address },
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
    let key1 = ks.generate_new_key(SignatureScheme::ED25519, None).unwrap();
    let key2 = ks
        .generate_new_key(SignatureScheme::Secp256k1, None)
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

    let (_guard, _) = telemetry_subscribers::TelemetryConfig::new(env!("CARGO_BIN_NAME"))
        .with_env()
        .init();

    cmd.execute().await
}
