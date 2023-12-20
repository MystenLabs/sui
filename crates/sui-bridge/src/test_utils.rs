// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::server::mock_handler::run_mock_server;
use crate::{
    crypto::{BridgeAuthorityKeyPair, BridgeAuthorityPublicKey, BridgeAuthoritySignInfo},
    events::EmittedSuiToEthTokenBridgeV1,
    server::mock_handler::BridgeRequestMockHandler,
    types::{
        BridgeAction, BridgeAuthority, BridgeChainId, SignedBridgeAction, SuiToEthBridgeAction,
        TokenId,
    },
};
use ethers::types::Address as EthAddress;
use fastcrypto::traits::KeyPair;
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
    sui_tx_digest: TransactionDigest,
    sui_tx_event_index: u16,
    nonce: u64,
    amount: u128,
) -> BridgeAction {
    BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
        sui_tx_digest,
        sui_tx_event_index,
        sui_bridge_event: EmittedSuiToEthTokenBridgeV1 {
            nonce,
            sui_chain_id: BridgeChainId::SuiTestnet,
            sui_address: SuiAddress::random_for_testing_only(),
            eth_chain_id: BridgeChainId::EthSepolia,
            eth_address: EthAddress::random(),
            token_id: TokenId::Sui,
            amount,
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
