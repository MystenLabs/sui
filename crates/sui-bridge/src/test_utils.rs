// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    crypto::{BridgeAuthorityKeyPair, BridgeAuthorityPublicKey},
    events::EmittedSuiToEthTokenBridgeV1,
    types::{BridgeAction, BridgeAuthority, BridgeChainId, SuiToEthBridgeAction, TokenId},
};
use ethers::types::Address as EthAddress;
use fastcrypto::traits::KeyPair;
use std::{pin::Pin, sync::Arc};
use sui_types::{
    base_types::SuiAddress, crypto::get_key_pair, digests::TransactionDigest, multiaddr::Multiaddr,
};

pub fn get_test_authority_and_key(
    voting_power: u64,
    port: u16,
) -> (
    BridgeAuthority,
    BridgeAuthorityPublicKey,
    Pin<Arc<BridgeAuthorityKeyPair>>,
) {
    let (_, kp): (_, fastcrypto::secp256k1::Secp256k1KeyPair) = get_key_pair();
    let pubkey = kp.public().clone();
    let authority = BridgeAuthority {
        pubkey: pubkey.clone(),
        voting_power,
        bridge_network_address: Multiaddr::try_from(format!("/ip4/127.0.0.1/tcp/{}/http", port))
            .unwrap(),
        is_blocklisted: false,
    };
    let secret = Arc::pin(kp);

    (authority, pubkey, secret)
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
