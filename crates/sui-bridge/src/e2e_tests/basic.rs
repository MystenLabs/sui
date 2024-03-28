// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::abi::{eth_sui_bridge, EthBridgeCommittee, EthBridgeEvent, EthSuiBridge};
use crate::config::BridgeNodeConfig;
use crate::crypto::{BridgeAuthorityKeyPair, BridgeAuthorityPublicKeyBytes};
use crate::events::SuiBridgeEvent;
use crate::node::run_bridge_node;
use crate::sui_client::SuiBridgeClient;
use crate::sui_transaction_builder::get_sui_token_type_tag;
use crate::types::{BridgeAction, BridgeActionStatus, BridgeActionType, SuiToEthBridgeAction};
use crate::utils::EthSigner;
use crate::BRIDGE_ENABLE_PROTOCOL_VERSION;
use eth_sui_bridge::EthSuiBridgeEvents;
use ethers::prelude::*;
use ethers::types::Address as EthAddress;
use move_core_types::ident_str;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::process::Command;
use std::str::FromStr;
use sui_config::local_ip_utils::get_available_port;
use sui_json_rpc_types::{SuiObjectDataOptions, SuiTransactionBlockResponse};
use sui_sdk::wallet_context::WalletContext;
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::bridge::{
    BridgeChainId, MoveTypeBridgeMessageKey, MoveTypeBridgeRecord, BRIDGE_MODULE_NAME, TOKEN_ID_ETH,
};
use sui_types::collection_types::LinkedTableNode;
use sui_types::committee::TOTAL_VOTING_POWER;
use sui_types::crypto::EncodeDecodeBase64;
use sui_types::crypto::KeypairTraits;
use sui_types::dynamic_field::{DynamicFieldName, Field};
use sui_types::object::Object;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{ObjectArg, TransactionData};
use sui_types::{TypeTag, BRIDGE_PACKAGE_ID};
use tempfile::tempdir;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use tracing::info;

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

#[tokio::test]
async fn test_bridge_from_eth_to_sui_to_eth() {
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
    let eth_chain_id = BridgeChainId::EthLocalTest as u8;
    let sui_chain_id = BridgeChainId::SuiLocalTest as u8;
    let mut test_cluster: test_cluster::TestCluster = TestClusterBuilder::new()
        .with_protocol_version(BRIDGE_ENABLE_PROTOCOL_VERSION.into())
        .with_epoch_duration_ms(20000)
        .build_with_bridge()
        .await;
    let sui_client = test_cluster.fullnode_handle.sui_client.clone();
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

    let (eth_signer_0, eth_private_key_hex_0) = get_eth_signer_client_e2e_test_only(&anvil_url)
        .await
        .unwrap();
    let eth_address_0 = eth_signer_0.address();

    let eth_signer_clone = eth_signer_0.clone();
    tokio::task::spawn(async move {
        deploy_sol_contract(
            &anvil_url_clone,
            eth_signer_clone,
            bridge_authority_keys,
            tx_ack,
            eth_private_key_hex_0,
        )
        .await
    });
    let deployed_contracts = rx_ack.await.unwrap();
    println!("Deployed contracts: {:?}", deployed_contracts);

    let sui_bridge_client = SuiBridgeClient::new(&test_cluster.fullnode_handle.rpc_url)
        .await
        .unwrap();
    let sui_address = test_cluster.get_address_0();
    let amount = 42;
    // ETH coin has 8 decimals on Sui
    let sui_amount = amount * 100_000_000;

    init_eth_to_sui_bridge(
        &eth_signer_0,
        deployed_contracts.sui_bridge,
        sui_address,
        eth_address_0,
        eth_chain_id,
        sui_chain_id,
        amount,
        sui_amount,
        TOKEN_ID_ETH,
        0,
    )
    .await;

    start_bridge_cluster(
        &test_cluster,
        anvil_port,
        deployed_contracts.sui_bridge_addrress_hex(),
    )
    .await;

    wait_for_transfer_action_status(
        &sui_bridge_client,
        eth_chain_id,
        0,
        BridgeActionStatus::Claimed,
    )
    .await;

    let eth_coin = sui_client
        .coin_read_api()
        .get_all_coins(sui_address, None, None)
        .await
        .unwrap()
        .data
        .iter()
        .find(|c| c.coin_type.contains("ETH"))
        .expect("Recipient should have received ETH coin now")
        .clone();
    assert_eq!(eth_coin.balance, sui_amount);

    // Now let the recipient send the coin back to ETH
    let eth_address_1 = EthAddress::random();
    let bridge_obj_arg = sui_bridge_client
        .get_mutable_bridge_object_arg()
        .await
        .unwrap();
    let nonce = 0;

    let sui_to_eth_bridge_action = init_sui_to_eth_bridge(
        &sui_client,
        sui_address,
        test_cluster.wallet_mut(),
        eth_chain_id,
        sui_chain_id,
        eth_address_1,
        eth_coin.object_ref(),
        nonce,
        bridge_obj_arg,
        sui_amount,
    )
    .await;

    let message = eth_sui_bridge::Message::from(sui_to_eth_bridge_action);

    // Wait for the bridge action to be approved
    wait_for_transfer_action_status(
        &sui_bridge_client,
        sui_chain_id,
        nonce,
        BridgeActionStatus::Approved,
    )
    .await;

    // Now collect sigs from the bridge record and submit to eth to claim
    // TODO: add a function to grab validator sigs
    let summary = sui_bridge_client.get_bridge_summary().await.unwrap();
    let records_id = summary.bridge_records_id;
    let key = serde_json::json!(
        {
            // u64 is represented as string
            "bridge_seq_num": nonce.to_string(),
            "message_type": BridgeActionType::TokenTransfer as u8,
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

    let signatures: Vec<Bytes> = sigs
        .into_iter()
        .map(|sig: Vec<u8>| Bytes::from(sig))
        .collect();
    let eth_sui_bridge =
        EthSuiBridge::new(deployed_contracts.sui_bridge, eth_signer_0.clone().into());
    let tx = eth_sui_bridge.transfer_bridged_tokens_with_signatures(signatures, message);
    let _eth_claim_tx_receipt = tx.send().await.unwrap().await.unwrap().unwrap();
    // Assert eth_address_1 has received ETH
    assert_eq!(
        eth_signer_0.get_balance(eth_address_1, None).await.unwrap(),
        U256::from(amount) * U256::exp10(18)
    );
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
        println!("Solidity contract deployment finished successfully");
    } else {
        println!(
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

pub async fn get_eth_signer_client_e2e_test_only(
    eth_rpc_url: &str,
) -> anyhow::Result<(EthSigner, String)> {
    // This private key is derived from the default anvil setting.
    // Mnemonic:          test test test test test test test test test test test junk
    // Derivation path:   m/44'/60'/0'/0/
    // DO NOT USE IT ANYWHERE ELSE EXCEPT FOR RUNNING AUTOMATIC INTEGRATION TESTING
    let url = eth_rpc_url.to_string();
    let private_key_0 = "0x4bbbf85ce3377467afe5d46f804f221813b2bb87f24d81f60f1fcdbf7cbf4356";
    let signer_0 = get_eth_signer_client(&url, private_key_0).await?;
    Ok((signer_0, private_key_0.to_string()))
}

async fn deposit_native_eth_to_sol_contract(
    signer: &EthSigner,
    contract_address: EthAddress,
    sui_recipient_address: SuiAddress,
    sui_chain_id: u8,
    amount: u64,
) -> ContractCall<EthSigner, ()> {
    let contract = EthSuiBridge::new(contract_address, signer.clone().into());
    let sui_recipient_address = sui_recipient_address.to_vec().into();
    let amount = U256::from(amount) * U256::exp10(18); // 1 ETH
    contract
        .bridge_eth(sui_recipient_address, sui_chain_id)
        .value(amount)
}

async fn start_bridge_cluster(
    test_cluster: &TestCluster,
    anvil_port: u16,
    eth_bridge_contract_address: String,
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

    let eth_rpc_url = format!("http://127.0.0.1:{}", anvil_port);
    for (i, (kp, server_listen_port)) in bridge_authority_keys
        .iter()
        .zip(bridge_server_ports.iter())
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
            .transfer_sui_must_exceeed(client_sui_address, 1000000000)
            .await;

        let config = BridgeNodeConfig {
            server_listen_port: *server_listen_port,
            metrics_port: get_available_port("127.0.0.1"),
            bridge_authority_key_path_base64_raw: authority_key_path,
            sui_rpc_url: test_cluster.fullnode_handle.rpc_url.clone(),
            eth_rpc_url: eth_rpc_url.clone(),
            eth_addresses: vec![eth_bridge_contract_address.clone()],
            approved_governance_actions: vec![],
            run_client: true,
            bridge_client_key_path_base64_sui_key: None,
            bridge_client_gas_object: None,
            db_path: Some(db_path),
            eth_bridge_contracts_start_block_override: Some(BTreeMap::from_iter(vec![(
                eth_bridge_contract_address.clone(),
                0,
            )])),
            sui_bridge_module_last_processed_event_id_override: None,
        };
        // Spawn bridge node in memory
        let config_clone = config.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
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
    supported_tokens: Vec<String>,
    token_prices: Vec<u64>,
}

async fn deposit_eth_to_sui_package(
    sui_client: &SuiClient,
    sui_address: SuiAddress,
    wallet_context: &mut WalletContext,
    target_chain: u8,
    target_address: EthAddress,
    token: ObjectRef,
    bridge_object_arg: ObjectArg,
) -> SuiTransactionBlockResponse {
    let mut builder = ProgrammableTransactionBuilder::new();
    let arg_target_chain = builder.pure(target_chain).unwrap();
    let arg_target_address = builder.pure(target_address.as_bytes()).unwrap();
    let arg_token = builder.obj(ObjectArg::ImmOrOwnedObject(token)).unwrap();
    let arg_bridge = builder.obj(bridge_object_arg).unwrap();

    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        BRIDGE_MODULE_NAME.to_owned(),
        ident_str!("send_token").to_owned(),
        vec![get_sui_token_type_tag(TOKEN_ID_ETH).unwrap()],
        vec![arg_bridge, arg_target_chain, arg_target_address, arg_token],
    );

    let pt = builder.finish();
    let gas_object_ref = wallet_context
        .get_one_gas_object_owned_by_address(sui_address)
        .await
        .unwrap()
        .unwrap();
    let tx_data = TransactionData::new_programmable(
        sui_address,
        vec![gas_object_ref],
        pt,
        500_000_000,
        sui_client
            .governance_api()
            .get_reference_gas_price()
            .await
            .unwrap(),
    );
    let tx = wallet_context.sign_transaction(&tx_data);
    wallet_context.execute_transaction_must_succeed(tx).await
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct DeployedSolContracts {
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

async fn init_eth_to_sui_bridge(
    eth_signer: &EthSigner,
    sui_bridge_contract_address: EthAddress,
    sui_address: SuiAddress,
    eth_address: EthAddress,
    eth_chain_id: u8,
    sui_chain_id: u8,
    amount: u64,
    sui_amount: u64,
    token_id: u8,
    nonce: u64,
) {
    let eth_tx = deposit_native_eth_to_sol_contract(
        eth_signer,
        sui_bridge_contract_address,
        sui_address,
        sui_chain_id,
        amount,
    )
    .await;
    let pending_tx = eth_tx.send().await.unwrap();
    let tx_receipt = pending_tx.await.unwrap().unwrap();
    let eth_bridge_event = tx_receipt
        .logs
        .iter()
        .find_map(EthBridgeEvent::try_from_log)
        .unwrap();
    let EthBridgeEvent::EthSuiBridgeEvents(EthSuiBridgeEvents::TokensDepositedFilter(
        eth_bridge_event,
    )) = eth_bridge_event
    else {
        unreachable!();
    };
    // assert eth log matches
    assert_eq!(eth_bridge_event.source_chain_id, eth_chain_id);
    assert_eq!(eth_bridge_event.nonce, nonce);
    assert_eq!(eth_bridge_event.destination_chain_id, sui_chain_id);
    assert_eq!(eth_bridge_event.token_id, token_id);
    assert_eq!(eth_bridge_event.sui_adjusted_amount, sui_amount);
    assert_eq!(eth_bridge_event.sender_address, eth_address);
    assert_eq!(eth_bridge_event.recipient_address, sui_address.to_vec());
}

async fn init_sui_to_eth_bridge(
    sui_client: &SuiClient,
    sui_address: SuiAddress,
    wallet_context: &mut WalletContext,
    eth_chain_id: u8,
    sui_chain_id: u8,
    eth_address: EthAddress,
    token: ObjectRef,
    nonce: u64,
    bridge_object_arg: ObjectArg,
    sui_amount: u64,
) -> SuiToEthBridgeAction {
    let resp = deposit_eth_to_sui_package(
        sui_client,
        sui_address,
        wallet_context,
        eth_chain_id,
        eth_address,
        token,
        bridge_object_arg,
    )
    .await;
    let sui_events = resp.events.unwrap().data;
    let bridge_event = sui_events
        .iter()
        .filter_map(|e| {
            let sui_bridge_event = SuiBridgeEvent::try_from_sui_event(e).unwrap()?;
            sui_bridge_event.try_into_bridge_action(e.id.tx_digest, e.id.event_seq as u16)
        })
        .find_map(|e| {
            if let BridgeAction::SuiToEthBridgeAction(a) = e {
                Some(a)
            } else {
                None
            }
        })
        .unwrap();

    assert_eq!(bridge_event.sui_bridge_event.nonce, nonce);
    assert_eq!(
        bridge_event.sui_bridge_event.sui_chain_id as u8,
        sui_chain_id
    );
    assert_eq!(
        bridge_event.sui_bridge_event.eth_chain_id as u8,
        eth_chain_id
    );
    assert_eq!(bridge_event.sui_bridge_event.sui_address, sui_address);
    assert_eq!(bridge_event.sui_bridge_event.eth_address, eth_address);
    assert_eq!(bridge_event.sui_bridge_event.token_id, TOKEN_ID_ETH);
    assert_eq!(
        bridge_event.sui_bridge_event.amount_sui_adjusted,
        sui_amount
    );
    bridge_event
}

async fn wait_for_transfer_action_status(
    sui_bridge_client: &SuiBridgeClient,
    chain_id: u8,
    nonce: u64,
    status: BridgeActionStatus,
) {
    // Wait for the bridge action to be approved
    let now = std::time::Instant::now();
    loop {
        let res = sui_bridge_client
            .get_token_transfer_action_onchain_status_until_success(chain_id, nonce)
            .await;
        if res == status {
            break;
        }
        if now.elapsed().as_secs() > 30 {
            panic!(
                "Timeout waiting for token transfer action to be {:?}",
                status
            );
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
