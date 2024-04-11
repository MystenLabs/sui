// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::abi::EthBridgeCommittee;
use crate::config::{BridgeNodeConfig, EthConfig, SuiConfig};
use crate::crypto::{BridgeAuthorityKeyPair, BridgeAuthorityPublicKeyBytes};
use crate::node::run_bridge_node;
use crate::sui_client::SuiBridgeClient;
use crate::types::BridgeAction;
use crate::utils::EthSigner;
use crate::BRIDGE_ENABLE_PROTOCOL_VERSION;
use ethers::prelude::*;
use ethers::types::Address as EthAddress;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::process::{Child, Command};
use std::str::FromStr;
use sui_config::local_ip_utils::get_available_port;
use sui_json_rpc_types::SuiObjectDataOptions;
use sui_sdk::SuiClient;
use sui_types::base_types::SuiAddress;
use sui_types::bridge::{BridgeChainId, MoveTypeBridgeMessageKey, MoveTypeBridgeRecord};
use sui_types::collection_types::LinkedTableNode;
use sui_types::committee::TOTAL_VOTING_POWER;
use sui_types::crypto::EncodeDecodeBase64;
use sui_types::crypto::KeypairTraits;
use sui_types::dynamic_field::{DynamicFieldName, Field};
use sui_types::object::Object;
use sui_types::TypeTag;
use tempfile::tempdir;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use tracing::{error, info};

use crate::utils::get_eth_signer_client;

pub const BRIDGE_COMMITTEE_NAME: &str = "BridgeCommittee";
pub const SUI_BRIDGE_NAME: &str = "SuiBridge";
pub const BRIDGE_CONFIG_NAME: &str = "BridgeConfig";
pub const BRIDGE_LIMITER_NAME: &str = "BridgeLimiter";
pub const BRIDGE_VAULT_NAME: &str = "BridgeVault";
pub const BTC_NAME: &str = "BTC";
pub const ETH_NAME: &str = "ETH";
pub const USDC_NAME: &str = "USDC";
pub const USDT_NAME: &str = "USDT";

pub const TEST_PK: &str = "0x4bbbf85ce3377467afe5d46f804f221813b2bb87f24d81f60f1fcdbf7cbf4356";

pub async fn initialize_bridge_environment() -> (TestCluster, EthBridgeEnvironment) {
    let anvil_port = get_available_port("127.0.0.1");
    let anvil_url = format!("http://127.0.0.1:{anvil_port}");
    let mut eth_environment = EthBridgeEnvironment::new(&anvil_url, anvil_port)
        .await
        .unwrap();

    // Deploy solidity contracts in a separate task
    let anvil_url_clone = anvil_url.clone();
    let (tx_ack, rx_ack) = tokio::sync::oneshot::channel();

    let mut server_ports = vec![];
    for _ in 0..3 {
        server_ports.push(get_available_port("127.0.0.1"));
    }

    let test_cluster: test_cluster::TestCluster = TestClusterBuilder::new()
        .with_protocol_version(BRIDGE_ENABLE_PROTOCOL_VERSION.into())
        .build_with_bridge(true)
        .await;
    info!("Test cluster built");
    test_cluster
        .trigger_reconfiguration_if_not_yet_and_assert_bridge_committee_initialized()
        .await;
    info!("Bridge committee is finalized");
    // TODO: do not block on `build_with_bridge`, return with bridge keys immediately
    // to parallize the setup.
    let bridge_authority_keys = test_cluster
        .bridge_authority_keys
        .as_ref()
        .unwrap()
        .iter()
        .map(|k| k.copy())
        .collect::<Vec<_>>();

    let (eth_signer, eth_pk_hex) = eth_environment.get_signer(TEST_PK).await.unwrap();
    tokio::task::spawn(async move {
        deploy_sol_contract(
            &anvil_url_clone,
            eth_signer,
            bridge_authority_keys,
            tx_ack,
            eth_pk_hex,
        )
        .await
    });
    let deployed_contracts = rx_ack.await.unwrap();
    info!("Deployed contracts: {:?}", deployed_contracts);
    eth_environment.contracts = Some(deployed_contracts);

    start_bridge_cluster(
        &test_cluster,
        &eth_environment,
        vec![vec![], vec![], vec![], vec![]],
    )
    .await;
    info!("Started bridge cluster");

    (test_cluster, eth_environment)
}

async fn deploy_sol_contract(
    anvil_url: &str,
    eth_signer: EthSigner,
    bridge_authority_keys: Vec<BridgeAuthorityKeyPair>,
    tx: tokio::sync::oneshot::Sender<DeployedSolContracts>,
    eth_private_key_hex: String,
) {
    let sol_path = format!("{}/../../bridge/evm", env!("CARGO_MANIFEST_DIR"));

    // Write the deploy config to a temp file then provide it to the forge late
    let deploy_config_path = tempfile::tempdir()
        .unwrap()
        .into_path()
        .join("sol_deploy_config.json");
    let node_len = bridge_authority_keys.len();
    let stake = TOTAL_VOTING_POWER / (node_len as u64);
    let committee_members = bridge_authority_keys
        .iter()
        .map(|k| {
            format!(
                "0x{:x}",
                BridgeAuthorityPublicKeyBytes::from(&k.public).to_eth_address()
            )
        })
        .collect::<Vec<_>>();
    let committee_member_stake = vec![stake; node_len];
    let deploy_config = SolDeployConfig {
        committee_member_stake: committee_member_stake.clone(),
        committee_members: committee_members.clone(),
        min_committee_stake_required: 10000,
        source_chain_id: 12,
        supported_chain_ids: vec![1, 2, 3],
        supported_chain_limits_in_dollars: vec![
            1000000000000000,
            1000000000000000,
            1000000000000000,
        ],
        supported_tokens: vec![], // this is set up in the deploy script
        token_prices: vec![12800, 432518900, 25969600, 10000, 10000],
    };

    let serialized_config = serde_json::to_string_pretty(&deploy_config).unwrap();
    tracing::debug!(
        "Serialized config written to {:?}: {:?}",
        deploy_config_path,
        serialized_config
    );
    let mut file = File::create(deploy_config_path.clone()).unwrap();
    file.write_all(serialized_config.as_bytes()).unwrap();

    // override for the deploy script
    std::env::set_var("OVERRIDE_CONFIG_PATH", deploy_config_path.to_str().unwrap());
    std::env::set_var("PRIVATE_KEY", eth_private_key_hex);
    std::env::set_var("ETHERSCAN_API_KEY", "n/a");

    info!("Deploying solidity contracts");
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
        .arg("--chain")
        .arg("31337")
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
        info!("Solidity contract deployment finished successfully");
    } else {
        error!(
            "Solidity contract deployment exited with code: {:?}",
            status.code()
        );
    }
    println!("Stdout: {}", s);
    println!("Stdout: {}", e);

    let mut deployed_contracts = BTreeMap::new();
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

    let contracts = DeployedSolContracts {
        sui_bridge: *deployed_contracts.get(SUI_BRIDGE_NAME).unwrap(),
        bridge_committee: *deployed_contracts.get(BRIDGE_COMMITTEE_NAME).unwrap(),
        bridge_config: *deployed_contracts.get(BRIDGE_CONFIG_NAME).unwrap(),
        bridge_limiter: *deployed_contracts.get(BRIDGE_LIMITER_NAME).unwrap(),
        bridge_vault: *deployed_contracts.get(BRIDGE_VAULT_NAME).unwrap(),
        btc: *deployed_contracts.get(BTC_NAME).unwrap(),
        eth: *deployed_contracts.get(ETH_NAME).unwrap(),
        usdc: *deployed_contracts.get(USDC_NAME).unwrap(),
        usdt: *deployed_contracts.get(USDT_NAME).unwrap(),
    };
    let eth_bridge_committee =
        EthBridgeCommittee::new(contracts.bridge_committee, eth_signer.clone().into());
    for (i, (m, s)) in committee_members
        .iter()
        .zip(committee_member_stake.iter())
        .enumerate()
    {
        let eth_address = EthAddress::from_str(m).unwrap();
        assert_eq!(
            eth_bridge_committee
                .committee_index(eth_address)
                .await
                .unwrap(),
            i as u8
        );
        assert_eq!(
            eth_bridge_committee
                .committee_stake(eth_address)
                .await
                .unwrap(),
            *s as u16
        );
        assert!(!eth_bridge_committee.blocklist(eth_address).await.unwrap());
    }
    tx.send(contracts).unwrap();
}

#[derive(Debug)]
pub(crate) struct EthBridgeEnvironment {
    rpc_url: String,
    process: Child,
    contracts: Option<DeployedSolContracts>,
}

impl EthBridgeEnvironment {
    async fn new(anvil_url: &str, anvil_port: u16) -> anyhow::Result<EthBridgeEnvironment> {
        // Start eth node with anvil
        let eth_environment_process = std::process::Command::new("anvil")
            .arg("--port")
            .arg(anvil_port.to_string())
            .arg("--block-time")
            .arg("1") // 1 second block time
            .arg("--slots-in-an-epoch")
            .arg("3") // 3 slots in an epoch
            .spawn()
            .expect("Failed to start anvil");

        Ok(EthBridgeEnvironment {
            rpc_url: anvil_url.to_string(),
            process: eth_environment_process,
            contracts: None,
        })
    }

    pub(crate) async fn get_signer(
        &self,
        private_key: &str,
    ) -> anyhow::Result<(EthSigner, String)> {
        let signer = get_eth_signer_client(&self.rpc_url, private_key).await?;
        Ok((signer, private_key.to_string()))
    }

    pub(crate) fn contracts(&self) -> &DeployedSolContracts {
        self.contracts.as_ref().unwrap()
    }
}

impl Drop for EthBridgeEnvironment {
    fn drop(&mut self) {
        self.process.kill().unwrap();
    }
}

async fn start_bridge_cluster(
    test_cluster: &TestCluster,
    eth_environment: &EthBridgeEnvironment,
    approved_governance_actions: Vec<Vec<BridgeAction>>,
) {
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
    assert_eq!(
        bridge_authority_keys.len(),
        approved_governance_actions.len()
    );

    let anvil_url = &eth_environment.rpc_url;
    let eth_bridge_contract_address = eth_environment
        .contracts
        .as_ref()
        .unwrap()
        .sui_bridge_addrress_hex();

    for (i, ((kp, server_listen_port), approved_governance_actions)) in bridge_authority_keys
        .iter()
        .zip(bridge_server_ports.iter())
        .zip(approved_governance_actions.into_iter())
        .enumerate()
    {
        // prepare node config (server + client)
        let tmp_dir = tempdir().unwrap().into_path().join(i.to_string());
        std::fs::create_dir_all(tmp_dir.clone()).unwrap();
        let db_path = tmp_dir.join("client_db");
        // write authority key to file
        let authority_key_path = tmp_dir.join("bridge_authority_key");
        let base64_encoded = kp.encode_base64();
        std::fs::write(authority_key_path.clone(), base64_encoded).unwrap();

        let client_sui_address = SuiAddress::from(kp.public());
        // send some gas to this address
        test_cluster
            .transfer_sui_must_exceed(client_sui_address, 1000000000)
            .await;

        let config = BridgeNodeConfig {
            server_listen_port: *server_listen_port,
            metrics_port: get_available_port("127.0.0.1"),
            bridge_authority_key_path_base64_raw: authority_key_path,
            approved_governance_actions,
            run_client: true,
            db_path: Some(db_path),
            eth: EthConfig {
                eth_rpc_url: eth_environment.rpc_url.clone(),
                eth_bridge_proxy_address: eth_bridge_contract_address.clone(),
                eth_bridge_chain_id: BridgeChainId::EthCustom as u8,
                eth_contracts_start_block_override: Some(0),
            },
            sui: SuiConfig {
                sui_rpc_url: test_cluster.fullnode_handle.rpc_url.clone(),
                sui_bridge_chain_id: BridgeChainId::SuiCustom as u8,
                bridge_client_key_path_base64_sui_key: None,
                bridge_client_gas_object: None,
                sui_bridge_module_last_processed_event_id_override: None,
            },
        };
        // Spawn bridge node in memory
        let config_clone = config.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            run_bridge_node(config_clone).await.unwrap();
        });
    }
}

pub(crate) async fn get_signatures(
    sui_bridge_client: &SuiBridgeClient,
    nonce: u64,
    sui_chain_id: u8,
    sui_client: &SuiClient,
    message_type: u8,
) -> Vec<Bytes> {
    // Now collect sigs from the bridge record and submit to eth to claim
    let summary = sui_bridge_client.get_bridge_summary().await.unwrap();
    let records_id = summary.bridge_records_id;
    let key = serde_json::json!(
        {
            // u64 is represented as string
            "bridge_seq_num": nonce.to_string(),
            "message_type": message_type,
            "source_chain": sui_chain_id,
        }
    );
    let status_object_id = sui_client.read_api().get_dynamic_field_object(records_id,
        DynamicFieldName {
            type_: TypeTag::from_str("0x000000000000000000000000000000000000000000000000000000000000000b::message::BridgeMessageKey").unwrap(),
            value: key.clone(),
        },
    ).await.unwrap().into_object().unwrap().object_id;

    let object_resp = sui_client
        .read_api()
        .get_object_with_options(
            status_object_id,
            SuiObjectDataOptions::full_content().with_bcs(),
        )
        .await
        .unwrap();

    let object: Object = object_resp.into_object().unwrap().try_into().unwrap();
    let record: Field<
        MoveTypeBridgeMessageKey,
        LinkedTableNode<MoveTypeBridgeMessageKey, MoveTypeBridgeRecord>,
    > = object.to_rust().unwrap();
    let sigs = record.value.value.verified_signatures.unwrap();

    sigs.into_iter()
        .map(|sig: Vec<u8>| Bytes::from(sig))
        .collect()
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct DeployedSolContracts {
    pub sui_bridge: EthAddress,
    pub bridge_committee: EthAddress,
    pub bridge_limiter: EthAddress,
    pub bridge_vault: EthAddress,
    pub bridge_config: EthAddress,
    pub btc: EthAddress,
    pub eth: EthAddress,
    pub usdc: EthAddress,
    pub usdt: EthAddress,
}

impl DeployedSolContracts {
    fn eth_adress_to_hex(addr: EthAddress) -> String {
        format!("{:x}", addr)
    }

    pub fn sui_bridge_addrress_hex(&self) -> String {
        Self::eth_adress_to_hex(self.sui_bridge)
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
    supported_tokens: Vec<String>,
    token_prices: Vec<u64>,
}
