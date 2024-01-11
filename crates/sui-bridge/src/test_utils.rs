// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::abi::EthToSuiTokenBridgeV1;
use crate::eth_mock_provider::EthMockProvider;
use crate::server::mock_handler::run_mock_server;
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
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use sui_config::local_ip_utils;
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
            sui_chain_id: BridgeChainId::SuiTestnet,
            sui_address: SuiAddress::random_for_testing_only(),
            eth_chain_id: BridgeChainId::EthSepolia,
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
