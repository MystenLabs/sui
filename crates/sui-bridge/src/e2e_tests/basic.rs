// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::abi::EthSuiBridge;
use crate::config::BridgeNodeConfig;
use crate::crypto::{BridgeAuthorityKeyPair, BridgeAuthorityPublicKeyBytes};
use crate::eth_client::EthClient;
use crate::node::run_bridge_node;
use crate::utils::EthSigner;
use ethers::prelude::*;
use ethers::types::Address as EthAddress;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::process::Command;
use std::str::FromStr;
use std::sync::Arc;
use sui_config::local_ip_utils::get_available_port;
use sui_types::base_types::SuiAddress;
use sui_types::bridge::BridgeChainId;
use sui_types::committee::TOTAL_VOTING_POWER;
use sui_types::crypto::get_account_key_pair;
use sui_types::crypto::EncodeDecodeBase64;
use sui_types::crypto::KeypairTraits;
use sui_types::digests::TransactionDigest;
use sui_types::event::EventID;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use tracing::info;

use crate::utils::get_eth_signer_client;

///
/// Deployed contracts: {"USDT": 0xcf7ed3acca5a467e9e704c703e8d87f634fb0fc9, "BridgeLimiter": 0x8a791620dd6260079bf849dc5567adc3f2fdc318, "USDC": 0x9fe46736679d2d9a65f0992f2272de9f3c7fa6e0, "SuiBridge": 0xb7f8bc63bbcad18155201308c8f3540b07f84f5e, "BridgeConfig": 0xdc64a140aa3e981100a9beca4e685f962f0cf6c9, "BridgeVault": 0xa513e6e4b8f2a923d98304ec87f64353c4d5c853, "ETH": 0x5fbdb2315678afecb367f032d93f642f64180aa3, "BTC": 0xe7f1725e7734ce288f8367e1bb143e90bb3f0512}
///
pub const BRIDGE_COMMITTEE_NAME: &str = "BridgeCommittee";

// start eth node
// start sui cluster/ init bridge committee
// publish solidity code
// start bridge cluster
// transfer eth to bridge contract

#[tokio::test]
async fn test_bridge_publish() {
    telemetry_subscribers::init_for_testing();

    // Start eth node with anvil
    let anvil_port = get_available_port("127.0.0.1");
    let _eth_node_process = std::process::Command::new("anvil")
        .arg("--port")
        .arg(anvil_port.to_string())
        .arg("--block-time")
        .arg("1") // 1 second block time
        .arg("--slots-in-an-epoch")
        .arg("3") // 3 slots in an epoch
        .spawn()
        .expect("Failed to start anvil");
    let anvil_url = format!("http://127.0.0.1:{}", anvil_port);
    info!("Anvil URL: {}", anvil_url);
    // Deploy solidity contracts in a separate task
    let anvil_url_clone = anvil_url.clone();
    let (tx_ack, rx_ack) = tokio::sync::oneshot::channel();

    let mut server_ports = vec![];
    for _ in 0..3 {
        server_ports.push(get_available_port("127.0.0.1"));
    }
    let test_cluster: test_cluster::TestCluster = TestClusterBuilder::new()
        .with_protocol_version(37.into())
        .with_epoch_duration_ms(10000)
        .build_with_bridge()
        .await;

    test_cluster
        .wait_for_next_epoch_and_assert_bridge_committee_initialized()
        .await;

    // TODO: do not block on `build_with_bridge`, return with bridge keys immediately
    // to parallize the setup.
    let bridge_authority_keys = test_cluster
        .bridge_authority_keys
        .as_ref()
        .unwrap()
        .iter()
        .map(|k| k.copy())
        .collect::<Vec<_>>();

    tokio::task::spawn_blocking(move || {
        deploy_sol_contract(&anvil_url_clone, bridge_authority_keys, tx_ack)
    });
    let deployed_contracts = rx_ack.await.unwrap();
    println!("Deployed contracts: {:?}", deployed_contracts);
    let bridge_contract_address = deployed_contracts.get("SuiBridge").unwrap().clone();
    let eth_bridge_client = EthClient::new(
        &anvil_url,
        HashSet::from_iter(vec![bridge_contract_address]),
    );
    let (sui_address, sui_kp) = get_account_key_pair();
    let eth_signer = get_eth_signer_client_e2e_test_only(&anvil_url)
        .await
        .unwrap();
    let eth_tx = deposit_native_eth_to_sol_contract(
        eth_signer,
        bridge_contract_address,
        sui_address,
        BridgeChainId::SuiLocalTest as u8,
    )
    .await;
    let a = eth_tx.send().await.unwrap();
    let foo = a.await.unwrap().unwrap();
    println!("foo: {:?}", foo);
}

fn deploy_sol_contract(
    anvil_url: &str,
    bridge_authority_keys: Vec<BridgeAuthorityKeyPair>,
    tx: tokio::sync::oneshot::Sender<Arc<HashMap<String, EthAddress>>>,
) {
    let sol_path = format!("{}/../../bridge/evm", env!("CARGO_MANIFEST_DIR"));

    // Write the deploy config to a temp file then provide it to the forge late
    let deploy_config_path = tempfile::tempdir()
        .unwrap()
        .into_path()
        .join("sol_deploy_config.json");
    let node_len = bridge_authority_keys.len();
    let stake = (TOTAL_VOTING_POWER as u64 / node_len as u64) as u64;
    let committee_members = bridge_authority_keys
        .iter()
        .map(|k| {
            format!(
                "{:x}",
                BridgeAuthorityPublicKeyBytes::from(&k.public).to_eth_address()
            )
        })
        .collect::<Vec<_>>();
    let deploy_config = SolDeployConfig {
        committee_member_stake: vec![stake; node_len],
        committee_members,
        min_committee_stake_required: 10000,
        source_chain_id: 12,
        supported_chain_ids: vec![1, 2, 3],
        supported_chain_limits_in_dollars: vec![1000000000000000, 1000000000000000, 1000000000000000],
        supported_tokens: vec![], // this is set up in the deploy script
        token_prices: vec![12800, 432518900, 25969600, 10000],
    };

    let serialized_config = serde_json::to_string_pretty(&deploy_config).unwrap();
    println!("@@@@@@@@@@@@@@@@@@ Serialized config: {}", serialized_config);
    let mut file = File::create(deploy_config_path.clone()).unwrap();
    file.write_all(serialized_config.as_bytes()).unwrap();

    // override for the deploy script
    std::env::set_var("OVERRIDE_CONFIG_PATH", deploy_config_path.to_str().unwrap());

    Command::new("forge")
        .current_dir(sol_path.clone())
        .arg("clean")
        .status()
        .expect("Failed to execute `forge clean`");

    let mut child = Command::new("forge")
        .current_dir(sol_path)
        .arg("script")
        .arg("script/deploy_bridge.s.sol")
        .arg("--fork-url")
        .arg(anvil_url)
        .arg("--broadcast")
        .arg("--ffi")
        .stdout(std::process::Stdio::piped()) // Capture stdout
        .stderr(std::process::Stdio::piped()) // Capture stderr
        .spawn()
        .unwrap();

    let mut stdout = child.stdout.take().expect("Failed to open stdout");
    let mut stderr = child.stderr.take().expect("Failed to open stderr");

    // Read stdout/stderr to String
    let mut s = String::new();
    stdout.read_to_string(&mut s).unwrap();
    let mut e = String::new();
    stderr.read_to_string(&mut e).unwrap();

    // Wait for the child process to finish and collect its status
    let status = child.wait().unwrap();
    if status.success() {
        println!("Solidity contract deployment finished successfully");
    } else {
        println!(
            "Solidity contract deployment exited with code: {:?}",
            status.code()
        );
        println!("Stdout: {}", s);
        println!("Stdout: {}", e);
    }

    let mut deployed_contracts = HashMap::new();
    // Process the stdout to parse contract addresses
    for line in s.lines() {
        if line.contains("[Deployed]") {
            let replaced_line = line.replace("[Deployed]", "");
            let trimmed_line = replaced_line.trim();
            let parts: Vec<&str> = trimmed_line.split(':').collect();
            if parts.len() == 2 {
                let contract_name = parts[0].to_string().trim().to_string();
                let contract_address = EthAddress::from_str(parts[1].to_string().trim()).unwrap();
                deployed_contracts.insert(contract_name, contract_address);
            }
        }
    }
    tx.send(Arc::new(deployed_contracts)).unwrap();
}

pub async fn get_eth_signer_client_e2e_test_only(eth_rpc_url: &str) -> anyhow::Result<EthSigner> {
    // This private key is derived from the default anvil setting.
    // Mnemonic:          test test test test test test test test test test test junk
    // Derivation path:   m/44'/60'/0'/0/
    // DO NOT USE IT ANYWHERE ELSE EXCEPT FOR RUNNING AUTOMATIC INTEGRATION TESTING
    let private_key = "0x4bbbf85ce3377467afe5d46f804f221813b2bb87f24d81f60f1fcdbf7cbf4356";
    let url = eth_rpc_url.to_string();
    let signer = get_eth_signer_client(&url, private_key).await?;
    println!("Using Eth address: {:?}", signer.address());
    Ok(signer)
}

async fn deposit_native_eth_to_sol_contract(
    signer: EthSigner,
    contract_address: EthAddress,
    sui_recipient_address: SuiAddress,
    sui_chain_id: u8,
) -> ContractCall<EthSigner, ()> {
    let contract = EthSuiBridge::new(contract_address, signer.into());
    let sui_recipient_address = sui_recipient_address.to_vec().into();
    let amount = U256::from(1) * U256::exp10(18); // 1 ETH
    contract
        .bridge_eth(sui_recipient_address, sui_chain_id)
        .value(amount)
}

async fn start_bridge_cluster(test_cluster: TestCluster, anvil_port: u16) {
    // TODO: move this to TestCluster
    let bridge_authority_keys = test_cluster
        .bridge_authority_keys
        .as_ref()
        .unwrap()
        .iter()
        .map(|k| k.copy())
        .collect::<Vec<_>>();
    let bridge_server_ports = test_cluster.bridge_server_ports.as_ref().unwrap();
    assert_eq!(bridge_authority_keys.len(), bridge_server_ports.len());

    let eth_rpc_url = format!("http://127.0.0.1:{}", anvil_port);
    for (i, (kp, server_listen_port)) in bridge_authority_keys
        .iter()
        .zip(bridge_server_ports.iter())
        .enumerate()
    {
        // prepare node config (server + client)
        let tmp_dir = std::env::temp_dir();
        let db_path = tmp_dir.join(i.to_string());

        // write authority key to file
        let authority_key_path = tmp_dir.join("bridge_authority_key");
        let base64_encoded = kp.encode_base64();
        std::fs::write(authority_key_path.clone(), base64_encoded).unwrap();

        let client_sui_address = SuiAddress::from(kp.public());
        // send some gas to this address
        test_cluster
            .transfer_sui_must_exceeed(client_sui_address, 1000000000)
            .await;

        let config = BridgeNodeConfig {
            server_listen_port: *server_listen_port,
            metrics_port: get_available_port("127.0.0.1"),
            bridge_authority_key_path_base64_raw: authority_key_path,
            sui_rpc_url: test_cluster.fullnode_handle.rpc_url.clone(),
            eth_rpc_url: eth_rpc_url.clone(),
            eth_addresses: vec!["0x1f9840a85d5af5bf1d1762f925bdaddc4201f984".into()],
            approved_governance_actions: vec![],
            run_client: true,
            bridge_client_key_path_base64_sui_key: None,
            bridge_client_gas_object: None,
            sui_bridge_modules: Some(vec!["bridge".into()]),
            db_path: Some(db_path),
            eth_bridge_contracts_start_block_override: Some(BTreeMap::from_iter(vec![(
                "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984".into(),
                0,
            )])),
            sui_bridge_module_last_processed_event_id_override: Some(BTreeMap::from_iter(vec![(
                "bridge".into(),
                EventID {
                    tx_digest: TransactionDigest::random(),
                    event_seq: 0,
                },
            )])),
        };
        // Spawn bridge node in memory
        let config_clone = config.clone();
        tokio::spawn(async move {
            run_bridge_node(config_clone).await.unwrap();
        });
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SolDeployConfig {
    committee_member_stake: Vec<u64>,
    committee_members: Vec<String>,
    min_committee_stake_required: u64,
    source_chain_id: u64,
    supported_chain_ids: Vec<u64>,
    supported_chain_limits_in_dollars: Vec<u64>,
    supported_tokens: Vec<String>, // Assuming this should be a vector of strings
    token_prices: Vec<u64>,
}
