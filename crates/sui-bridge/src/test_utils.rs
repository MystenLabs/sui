// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::abi::EthBridgeCommittee;
use crate::abi::EthToSuiTokenBridgeV1;
use crate::crypto::BridgeAuthorityPublicKeyBytes;
use crate::eth_mock_provider::EthMockProvider;
use crate::events::SuiBridgeEvent;
use crate::server::mock_handler::run_mock_server;
use crate::sui_transaction_builder::build_sui_transaction;
use crate::types::{
    BridgeCommitteeValiditySignInfo, CertifiedBridgeAction, VerifiedCertifiedBridgeAction,
};
use crate::utils::get_eth_signer_client;
use crate::utils::EthSigner;
use crate::{
    crypto::{BridgeAuthorityKeyPair, BridgeAuthorityPublicKey, BridgeAuthoritySignInfo},
    events::EmittedSuiToEthTokenBridgeV1,
    server::mock_handler::BridgeRequestMockHandler,
    types::{
        BridgeAction, BridgeAuthority, EthToSuiBridgeAction, SignedBridgeAction,
        SuiToEthBridgeAction,
    },
};
use ethers::abi::{long_signature, ParamType};
use ethers::types::Address as EthAddress;
use ethers::types::{
    Block, BlockNumber, Filter, FilterBlockOption, Log, TransactionReceipt, TxHash, ValueOrArray,
    U64,
};
use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::traits::KeyPair;
use hex_literal::hex;
use move_core_types::language_storage::TypeTag;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{Read, Write};
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::process::Command;
use std::str::FromStr;
use sui_config::local_ip_utils;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_sdk::wallet_context::WalletContext;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SequenceNumber;
use sui_types::bridge::{BridgeChainId, TOKEN_ID_USDC};
use sui_types::committee::TOTAL_VOTING_POWER;
use sui_types::object::Owner;
use sui_types::transaction::{CallArg, ObjectArg};
use sui_types::{base_types::SuiAddress, crypto::get_key_pair, digests::TransactionDigest};
use sui_types::{BRIDGE_PACKAGE_ID, SUI_BRIDGE_OBJECT_ID};
use tokio::task::JoinHandle;
use tracing::error;
use tracing::info;

pub const BRIDGE_COMMITTEE_NAME: &str = "BridgeCommittee";
pub const SUI_BRIDGE_NAME: &str = "SuiBridge";
pub const BRIDGE_CONFIG_NAME: &str = "BridgeConfig";
pub const BRIDGE_LIMITER_NAME: &str = "BridgeLimiter";
pub const BRIDGE_VAULT_NAME: &str = "BridgeVault";
pub const BTC_NAME: &str = "BTC";
pub const ETH_NAME: &str = "ETH";
pub const USDC_NAME: &str = "USDC";
pub const USDT_NAME: &str = "USDT";

pub const DUMMY_MUTALBE_BRIDGE_OBJECT_ARG: ObjectArg = ObjectArg::SharedObject {
    id: SUI_BRIDGE_OBJECT_ID,
    initial_shared_version: SequenceNumber::from_u64(1),
    mutable: true,
};

pub fn get_test_authority_and_key(
    voting_power: u64,
    port: u16,
) -> (
    BridgeAuthority,
    BridgeAuthorityPublicKey,
    BridgeAuthorityKeyPair,
) {
    let (_, kp): (_, fastcrypto::secp256k1::Secp256k1KeyPair) = get_key_pair();
    let pubkey = kp.public().clone();
    let authority = BridgeAuthority {
        pubkey: pubkey.clone(),
        voting_power,
        base_url: format!("http://127.0.0.1:{}", port),
        is_blocklisted: false,
    };

    (authority, pubkey, kp)
}

// TODO: make a builder for this
pub fn get_test_sui_to_eth_bridge_action(
    sui_tx_digest: Option<TransactionDigest>,
    sui_tx_event_index: Option<u16>,
    nonce: Option<u64>,
    amount_sui_adjusted: Option<u64>,
    sender_address: Option<SuiAddress>,
    recipient_address: Option<EthAddress>,
    token_id: Option<u8>,
) -> BridgeAction {
    BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
        sui_tx_digest: sui_tx_digest.unwrap_or_else(TransactionDigest::random),
        sui_tx_event_index: sui_tx_event_index.unwrap_or(0),
        sui_bridge_event: EmittedSuiToEthTokenBridgeV1 {
            nonce: nonce.unwrap_or_default(),
            sui_chain_id: BridgeChainId::SuiCustom,
            sui_address: sender_address.unwrap_or_else(SuiAddress::random_for_testing_only),
            eth_chain_id: BridgeChainId::EthCustom,
            eth_address: recipient_address.unwrap_or_else(EthAddress::random),
            token_id: token_id.unwrap_or(TOKEN_ID_USDC),
            amount_sui_adjusted: amount_sui_adjusted.unwrap_or(100_000),
        },
    })
}

pub fn get_test_eth_to_sui_bridge_action(
    nonce: Option<u64>,
    amount: Option<u64>,
    sui_address: Option<SuiAddress>,
) -> BridgeAction {
    BridgeAction::EthToSuiBridgeAction(EthToSuiBridgeAction {
        eth_tx_hash: TxHash::random(),
        eth_event_index: 0,
        eth_bridge_event: EthToSuiTokenBridgeV1 {
            eth_chain_id: BridgeChainId::EthCustom,
            nonce: nonce.unwrap_or_default(),
            sui_chain_id: BridgeChainId::SuiCustom,
            token_id: TOKEN_ID_USDC,
            sui_adjusted_amount: amount.unwrap_or(100_000),
            sui_address: sui_address.unwrap_or_else(SuiAddress::random_for_testing_only),
            eth_address: EthAddress::random(),
        },
    })
}

pub fn run_mock_bridge_server(
    mock_handlers: Vec<BridgeRequestMockHandler>,
) -> (Vec<JoinHandle<()>>, Vec<u16>) {
    let mut handles = vec![];
    let mut ports = vec![];
    for mock_handler in mock_handlers {
        let localhost = local_ip_utils::localhost_for_testing();
        let port = local_ip_utils::get_available_port(&localhost);
        // start server
        let server_handle = run_mock_server(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port),
            mock_handler.clone(),
        );
        ports.push(port);
        handles.push(server_handle);
    }
    (handles, ports)
}

pub fn get_test_authorities_and_run_mock_bridge_server(
    voting_power: Vec<u64>,
    mock_handlers: Vec<BridgeRequestMockHandler>,
) -> (
    Vec<JoinHandle<()>>,
    Vec<BridgeAuthority>,
    Vec<BridgeAuthorityKeyPair>,
) {
    assert_eq!(voting_power.len(), mock_handlers.len());
    let (handles, ports) = run_mock_bridge_server(mock_handlers);
    let mut authorites = vec![];
    let mut secrets = vec![];
    for (port, vp) in ports.iter().zip(voting_power) {
        let (authority, _, secret) = get_test_authority_and_key(vp, *port);
        authorites.push(authority);
        secrets.push(secret);
    }

    (handles, authorites, secrets)
}

pub fn sign_action_with_key(
    action: &BridgeAction,
    secret: &BridgeAuthorityKeyPair,
) -> SignedBridgeAction {
    let sig = BridgeAuthoritySignInfo::new(action, secret);
    SignedBridgeAction::new_from_data_and_sig(action.clone(), sig)
}

pub fn mock_last_finalized_block(mock_provider: &EthMockProvider, block_number: u64) {
    let block = Block::<ethers::types::TxHash> {
        number: Some(U64::from(block_number)),
        ..Default::default()
    };
    mock_provider
        .add_response("eth_getBlockByNumber", ("finalized", false), block)
        .unwrap();
}

// Mocks eth_getLogs and eth_getTransactionReceipt for the given address and block range.
// The input log needs to have transaction_hash set.
pub fn mock_get_logs(
    mock_provider: &EthMockProvider,
    address: EthAddress,
    from_block: u64,
    to_block: u64,
    logs: Vec<Log>,
) {
    mock_provider.add_response::<[ethers::types::Filter; 1], Vec<ethers::types::Log>, Vec<ethers::types::Log>>(
        "eth_getLogs",
        [
            Filter {
                block_option: FilterBlockOption::Range {
                    from_block: Some(BlockNumber::Number(U64::from(from_block))),
                    to_block: Some(BlockNumber::Number(U64::from(to_block))),
                },
                address: Some(ValueOrArray::Value(address)),
                topics: [None, None, None, None],
            }
        ],
        logs.clone(),
    ).unwrap();

    for log in logs {
        mock_provider
            .add_response::<[TxHash; 1], TransactionReceipt, TransactionReceipt>(
                "eth_getTransactionReceipt",
                [log.transaction_hash.unwrap()],
                TransactionReceipt {
                    block_number: log.block_number,
                    logs: vec![log],
                    ..Default::default()
                },
            )
            .unwrap();
    }
}

/// Returns a test Log and corresponding BridgeAction
// Refernece: https://github.com/rust-ethereum/ethabi/blob/master/ethabi/src/event.rs#L192
pub fn get_test_log_and_action(
    contract_address: EthAddress,
    tx_hash: TxHash,
    event_index: u16,
) -> (Log, BridgeAction) {
    let token_id = 3u8;
    let sui_adjusted_amount = 10000000u64;
    let source_address = EthAddress::random();
    let sui_address: SuiAddress = SuiAddress::random_for_testing_only();
    let target_address = Hex::decode(&sui_address.to_string()).unwrap();
    // Note: must use `encode` rather than `encode_packged`
    let encoded = ethers::abi::encode(&[
        // u8/u64 is encoded as u256 in abi standard
        ethers::abi::Token::Uint(ethers::types::U256::from(token_id)),
        ethers::abi::Token::Uint(ethers::types::U256::from(sui_adjusted_amount)),
        ethers::abi::Token::Address(source_address),
        ethers::abi::Token::Bytes(target_address.clone()),
    ]);
    let log = Log {
        address: contract_address,
        topics: vec![
            long_signature(
                "TokensDeposited",
                &[
                    ParamType::Uint(8),
                    ParamType::Uint(64),
                    ParamType::Uint(8),
                    ParamType::Uint(8),
                    ParamType::Uint(64),
                    ParamType::Address,
                    ParamType::Bytes,
                ],
            ),
            hex!("0000000000000000000000000000000000000000000000000000000000000001").into(), // chain id: sui testnet
            hex!("0000000000000000000000000000000000000000000000000000000000000010").into(), // nonce: 16
            hex!("000000000000000000000000000000000000000000000000000000000000000b").into(), // chain id: sepolia
        ],
        data: encoded.into(),
        block_hash: Some(TxHash::random()),
        block_number: Some(1.into()),
        transaction_hash: Some(tx_hash),
        log_index: Some(0.into()),
        ..Default::default()
    };
    let topic_1: [u8; 32] = log.topics[1].into();
    let topic_3: [u8; 32] = log.topics[3].into();

    let bridge_action = BridgeAction::EthToSuiBridgeAction(EthToSuiBridgeAction {
        eth_tx_hash: tx_hash,
        eth_event_index: event_index,
        eth_bridge_event: EthToSuiTokenBridgeV1 {
            eth_chain_id: BridgeChainId::try_from(topic_1[topic_1.len() - 1]).unwrap(),
            nonce: u64::from_be_bytes(log.topics[2].as_ref()[24..32].try_into().unwrap()),
            sui_chain_id: BridgeChainId::try_from(topic_3[topic_3.len() - 1]).unwrap(),
            token_id,
            sui_adjusted_amount,
            sui_address,
            eth_address: source_address,
        },
    });
    (log, bridge_action)
}

pub async fn bridge_token(
    context: &mut WalletContext,
    recv_address: EthAddress,
    token_ref: ObjectRef,
    token_type: TypeTag,
    bridge_object_arg: ObjectArg,
) -> EmittedSuiToEthTokenBridgeV1 {
    let rgp = context.get_reference_gas_price().await.unwrap();
    let sender = context.active_address().unwrap();
    let gas_object = context.get_one_gas_object().await.unwrap().unwrap().1;
    let tx = TestTransactionBuilder::new(sender, gas_object, rgp)
        .move_call(
            BRIDGE_PACKAGE_ID,
            "bridge",
            "send_token",
            vec![
                CallArg::Object(bridge_object_arg),
                CallArg::Pure(bcs::to_bytes(&(BridgeChainId::EthCustom as u8)).unwrap()),
                CallArg::Pure(bcs::to_bytes(&recv_address.as_bytes()).unwrap()),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(token_ref)),
            ],
        )
        .with_type_args(vec![token_type])
        .build();
    let signed_tn = context.sign_transaction(&tx);
    let resp = context.execute_transaction_must_succeed(signed_tn).await;
    let events = resp.events.unwrap();
    let mut bridge_events = events
        .data
        .iter()
        .filter_map(|event| SuiBridgeEvent::try_from_sui_event(event).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(bridge_events.len(), 1);
    match bridge_events.remove(0) {
        SuiBridgeEvent::SuiToEthTokenBridgeV1(event) => event,
    }
}

/// Returns a VerifiedCertifiedBridgeAction with signatures from the given
/// BridgeAction and BridgeAuthorityKeyPair
pub fn get_certified_action_with_validator_secrets(
    action: BridgeAction,
    secrets: &Vec<BridgeAuthorityKeyPair>,
) -> VerifiedCertifiedBridgeAction {
    let mut sigs = BTreeMap::new();
    for secret in secrets {
        let signed_action = sign_action_with_key(&action, secret);
        sigs.insert(secret.public().into(), signed_action.into_sig().signature);
    }
    let certified_action = CertifiedBridgeAction::new_from_data_and_sig(
        action,
        BridgeCommitteeValiditySignInfo { signatures: sigs },
    );
    VerifiedCertifiedBridgeAction::new_from_verified(certified_action)
}

/// Approve a bridge action with the given validator secrets. Return the
/// newly created token object reference if `expected_token_receiver` is Some
/// (only relevant when the action is eth -> Sui transfer),
/// Otherwise return None.
/// Note: for sui -> eth transfers, the actual deposit needs to be recorded.
/// Use `bridge_token` to do it.
// TODO(bridge): It appears this function is very slow (particularly, `execute_transaction_must_succeed`).
// Investigate why.
pub async fn approve_action_with_validator_secrets(
    wallet_context: &mut WalletContext,
    bridge_obj_org: ObjectArg,
    // TODO: add `token_recipient()` for `BridgeAction` so we don't need `expected_token_receiver`
    action: BridgeAction,
    validator_secrets: &Vec<BridgeAuthorityKeyPair>,
    // Only relevant for eth -> sui transfers when token will be dropped to the recipient
    expected_token_receiver: Option<SuiAddress>,
    id_token_map: &HashMap<u8, TypeTag>,
) -> Option<ObjectRef> {
    let action_certificate = get_certified_action_with_validator_secrets(action, validator_secrets);
    let sui_address = wallet_context.active_address().unwrap();
    let gas_obj_ref = wallet_context
        .get_one_gas_object()
        .await
        .unwrap()
        .unwrap()
        .1;
    let tx_data = build_sui_transaction(
        sui_address,
        &gas_obj_ref,
        action_certificate,
        bridge_obj_org,
        id_token_map,
    )
    .unwrap();
    let signed_tx = wallet_context.sign_transaction(&tx_data);
    let resp = wallet_context
        .execute_transaction_must_succeed(signed_tx)
        .await;

    // If `expected_token_receiver` is None, return
    expected_token_receiver?;

    let expected_token_receiver = expected_token_receiver.unwrap();
    for created in resp.effects.unwrap().created() {
        if created.owner == Owner::AddressOwner(expected_token_receiver) {
            return Some(created.reference.to_object_ref());
        }
    }
    panic!(
        "Didn't find the creted object owned by {}",
        expected_token_receiver
    );
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

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DeployedSolContracts {
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
    pub fn eth_adress_to_hex(addr: EthAddress) -> String {
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

pub async fn deploy_sol_contract(
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
        source_chain_id: BridgeChainId::EthCustom as u64,
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
