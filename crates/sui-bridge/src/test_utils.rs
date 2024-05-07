// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::abi::EthToSuiTokenBridgeV1;
use crate::eth_mock_provider::EthMockProvider;
use crate::events::SuiBridgeEvent;
use crate::server::mock_handler::run_mock_server;
use crate::sui_transaction_builder::{
    get_bridge_package_id, get_root_bridge_object_arg, get_sui_token_type_tag,
};
use crate::types::BridgeInnerDynamicField;
use crate::{
    crypto::{BridgeAuthorityKeyPair, BridgeAuthorityPublicKey, BridgeAuthoritySignInfo},
    events::EmittedSuiToEthTokenBridgeV1,
    server::mock_handler::BridgeRequestMockHandler,
    types::{
        BridgeAction, BridgeAuthority, BridgeChainId, EthToSuiBridgeAction, SignedBridgeAction,
        SuiToEthBridgeAction, TokenId,
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
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::path::PathBuf;
use sui_config::local_ip_utils;
use sui_json_rpc_types::ObjectChange;
use sui_sdk::wallet_context::WalletContext;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::ObjectRef;
use sui_types::object::Owner;
use sui_types::transaction::{CallArg, ObjectArg};
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::{base_types::SuiAddress, crypto::get_key_pair, digests::TransactionDigest};
use tokio::task::JoinHandle;

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

pub fn get_test_sui_to_eth_bridge_action(
    sui_tx_digest: Option<TransactionDigest>,
    sui_tx_event_index: Option<u16>,
    nonce: Option<u64>,
    amount: Option<u64>,
) -> BridgeAction {
    BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
        sui_tx_digest: sui_tx_digest.unwrap_or_else(TransactionDigest::random),
        sui_tx_event_index: sui_tx_event_index.unwrap_or(0),
        sui_bridge_event: EmittedSuiToEthTokenBridgeV1 {
            nonce: nonce.unwrap_or_default(),
            sui_chain_id: BridgeChainId::SuiLocalTest,
            sui_address: SuiAddress::random_for_testing_only(),
            eth_chain_id: BridgeChainId::EthLocalTest,
            eth_address: EthAddress::random(),
            token_id: TokenId::Sui,
            amount: amount.unwrap_or(100_000),
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
    let token_code = 3u8;
    let amount = 10000000u64;
    let source_address = EthAddress::random();
    let sui_address: SuiAddress = SuiAddress::random_for_testing_only();
    let target_address = Hex::decode(&sui_address.to_string()).unwrap();
    // Note: must use `encode` rather than `encode_packged`
    let encoded = ethers::abi::encode(&[
        // u8 is encoded as u256 in abi standard
        ethers::abi::Token::Uint(ethers::types::U256::from(token_code)),
        ethers::abi::Token::Uint(ethers::types::U256::from(amount)),
        ethers::abi::Token::Address(source_address),
        ethers::abi::Token::Bytes(target_address.clone()),
    ]);
    let log = Log {
        address: contract_address,
        topics: vec![
            long_signature(
                "TokensBridgedToSui",
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
            token_id: TokenId::try_from(token_code).unwrap(),
            amount,
            sui_address,
            eth_address: source_address,
        },
    });
    (log, bridge_action)
}

pub async fn publish_bridge_package(context: &WalletContext) -> BTreeMap<TokenId, ObjectRef> {
    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let gas_price = context.get_reference_gas_price().await.unwrap();

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["..", "..", "examples", "move", "bridge"]);

    let txn = context.sign_transaction(
        &TestTransactionBuilder::new(sender, gas_object, gas_price)
            .publish(path)
            .build(),
    );
    let resp = context.execute_transaction_must_succeed(txn).await;
    let object_changes = resp.object_changes.unwrap();
    let package_id = object_changes
        .iter()
        .find(|change| matches!(change, ObjectChange::Published { .. }))
        .map(|change| change.object_id())
        .unwrap();

    let mut treasury_caps = BTreeMap::new();
    object_changes.iter().for_each(|change| {
        if let ObjectChange::Created { object_type, .. } = change {
            let object_type_str = object_type.to_string();
            if object_type_str.contains("TreasuryCap") {
                if object_type_str.contains("BTC") {
                    treasury_caps.insert(TokenId::BTC, change.object_ref());
                } else if object_type_str.contains("ETH") {
                    treasury_caps.insert(TokenId::ETH, change.object_ref());
                } else if object_type_str.contains("USDC") {
                    treasury_caps.insert(TokenId::USDC, change.object_ref());
                } else if object_type_str.contains("USDT") {
                    treasury_caps.insert(TokenId::USDT, change.object_ref());
                }
            }
        }
    });

    let root_bridge_object_ref = object_changes
        .iter()
        .find(|change| match change {
            ObjectChange::Created {
                object_type, owner, ..
            } => {
                object_type.to_string().contains("Bridge") && matches!(owner, Owner::Shared { .. })
            }
            _ => false,
        })
        .map(|change| change.object_ref())
        .unwrap();

    let bridge_inner_object_ref = object_changes
        .iter()
        .find(|change| match change {
            ObjectChange::Created { object_type, .. } => {
                object_type.to_string().contains("BridgeInner")
            }
            _ => false,
        })
        .map(|change| change.object_ref())
        .unwrap();

    let client = context.get_client().await.unwrap();
    let bcs_bytes = client
        .read_api()
        .get_move_object_bcs(bridge_inner_object_ref.0)
        .await
        .unwrap();
    let bridge_inner_object: BridgeInnerDynamicField = bcs::from_bytes(&bcs_bytes).unwrap();
    let bridge_record_id = bridge_inner_object.value.bridge_records.id;

    // TODO: remove once we don't rely on env var to get package id
    std::env::set_var("BRIDGE_PACKAGE_ID", package_id.to_string());
    std::env::set_var("BRIDGE_RECORD_ID", bridge_record_id.to_string());
    std::env::set_var(
        "ROOT_BRIDGE_OBJECT_ID",
        root_bridge_object_ref.0.to_string(),
    );
    std::env::set_var(
        "ROOT_BRIDGE_OBJECT_INITIAL_SHARED_VERSION",
        u64::from(root_bridge_object_ref.1).to_string(),
    );
    std::env::set_var("BRIDGE_OBJECT_ID", bridge_inner_object_ref.0.to_string());

    treasury_caps
}

pub async fn mint_tokens(
    context: &mut WalletContext,
    treasury_cap_ref: ObjectRef,
    amount: u64,
    token_id: TokenId,
) -> (ObjectRef, ObjectRef) {
    let rgp = context.get_reference_gas_price().await.unwrap();
    let sender = context.active_address().unwrap();
    let gas_obj_ref = context.get_one_gas_object().await.unwrap().unwrap().1;
    let tx = TestTransactionBuilder::new(sender, gas_obj_ref, rgp)
        .move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            "coin",
            "mint_and_transfer",
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(treasury_cap_ref)),
                CallArg::Pure(bcs::to_bytes(&amount).unwrap()),
                CallArg::Pure(sender.to_vec()),
            ],
        )
        .with_type_args(vec![get_sui_token_type_tag(token_id)])
        .build();
    let signed_tn = context.sign_transaction(&tx);
    let resp = context.execute_transaction_must_succeed(signed_tn).await;
    let object_changes = resp.object_changes.unwrap();

    let treasury_cap_obj_ref = object_changes
        .iter()
        .find(|change| matches!(change, ObjectChange::Mutated { object_type, .. } if object_type.to_string().contains("TreasuryCap")))
        .map(|change| change.object_ref())
        .unwrap();

    let minted_coin_obj_ref = object_changes
        .iter()
        .find(|change| matches!(change, ObjectChange::Created { .. }))
        .map(|change| change.object_ref())
        .unwrap();

    (treasury_cap_obj_ref, minted_coin_obj_ref)
}

pub async fn transfer_treasury_cap(
    context: &mut WalletContext,
    treasury_cap_ref: ObjectRef,
    token_id: TokenId,
) {
    let rgp = context.get_reference_gas_price().await.unwrap();
    let sender = context.active_address().unwrap();
    let gas_object = context.get_one_gas_object().await.unwrap().unwrap().1;
    let tx = TestTransactionBuilder::new(sender, gas_object, rgp)
        .move_call(
            *get_bridge_package_id(),
            "bridge",
            "add_treasury_cap",
            vec![
                CallArg::Object(*get_root_bridge_object_arg()),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(treasury_cap_ref)),
            ],
        )
        .with_type_args(vec![get_sui_token_type_tag(token_id)])
        .build();
    let signed_tn = context.sign_transaction(&tx);
    context.execute_transaction_must_succeed(signed_tn).await;
}

pub async fn bridge_token(
    context: &mut WalletContext,
    recv_address: EthAddress,
    token_ref: ObjectRef,
    token_id: TokenId,
) -> EmittedSuiToEthTokenBridgeV1 {
    let rgp = context.get_reference_gas_price().await.unwrap();
    let sender = context.active_address().unwrap();
    let gas_object = context.get_one_gas_object().await.unwrap().unwrap().1;
    let tx = TestTransactionBuilder::new(sender, gas_object, rgp)
        .move_call(
            *get_bridge_package_id(),
            "bridge",
            "send_token",
            vec![
                CallArg::Object(*get_root_bridge_object_arg()),
                CallArg::Pure(bcs::to_bytes(&(BridgeChainId::EthLocalTest as u8)).unwrap()),
                CallArg::Pure(bcs::to_bytes(&recv_address.as_bytes()).unwrap()),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(token_ref)),
            ],
        )
        .with_type_args(vec![get_sui_token_type_tag(token_id)])
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
