// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use std::sync::Arc;

use crate::abi::eth_sui_bridge::Message;
use crate::crypto::BridgeAuthorityKeyPair;
use crate::types::BridgeActionType;
use crate::{
    abi::{EthSuiBridge, EthSuiBridgeEvents},
    types::{BridgeAction, EmergencyAction, EmergencyActionType},
};
use bcs::to_bytes;
use ethers::middleware::SignerMiddleware;
use ethers::prelude::*;
use ethers::providers::{Http, Provider};
use ethers::signers::{Signer, Wallet};
use ethers::types::Address as EthAddress;
use fastcrypto::encoding::{Encoding, Hex};
// use ethers::utils::keccak256;
use fastcrypto::hash::Keccak256;
use fastcrypto::traits::RecoverableSigner;
use sui_types::bridge::BridgeChainId;
use sui_types::crypto::ToFromBytes;

pub async fn foo() {
    // let private_key = std::env::var("BRIDGE_TEST_PRIVATE_KEY").unwrap();
    let private_key = "";
    let contract_address: EthAddress = "0x55567302a77fFFA2de4b98E3daFFA9561eA6ff7C"
        .parse()
        .unwrap();
    let url = "https://ethereum-sepolia-rpc.publicnode.com";
    let provider = Provider::<Http>::try_from(url)
        .unwrap()
        .interval(std::time::Duration::from_millis(2000));
    let provider = Arc::new(provider);
    let wallet = Wallet::from_str(&private_key)
        .unwrap()
        .with_chain_id(11155111u64);
    let address = wallet.address();
    println!("address: {:?}", address);
    let client = SignerMiddleware::new(provider, wallet);

    let contract = EthSuiBridge::new(contract_address, client.into());

    // let provider =
    //     Provider::<Http>::try_from(url)?.interval(std::time::Duration::from_millis(2000));
    // let provider = Arc::new(provider);
    // let wallet = Wallet::from_str(&private_key)?.with_chain_id(11155111u64);

    let action = EmergencyAction {
        nonce: 0,
        chain_id: BridgeChainId::EthSepolia,
        action_type: EmergencyActionType::Pause,
    };

    let msg_bytes = action.to_bytes();
    let message = Message {
        message_type: BridgeActionType::EmergencyButton as u8,
        version: 1,
        nonce: 0,
        chain_id: BridgeChainId::EthSepolia as u8,
        payload: vec![EmergencyActionType::Pause as u8].into(),
    };

    // let bytes_hash = keccak256(&msg_bytes);

    // FIXME: must remove before remote push
    let pk1 = "";
    let pk1 = BridgeAuthorityKeyPair::from_bytes(&Hex::decode(pk1).unwrap()).unwrap();
    let pk2 = "";
    let pk2 = BridgeAuthorityKeyPair::from_bytes(&Hex::decode(pk2).unwrap()).unwrap();
    let pk3 = "";
    let pk3 = BridgeAuthorityKeyPair::from_bytes(&Hex::decode(pk3).unwrap()).unwrap();
    let pk4 = "";
    let pk4 = BridgeAuthorityKeyPair::from_bytes(&Hex::decode(pk4).unwrap()).unwrap();

    let sig1 = pk1
        .sign_recoverable_with_hash::<Keccak256>(&msg_bytes)
        .as_bytes()
        .to_vec();
    let sig2 = pk2
        .sign_recoverable_with_hash::<Keccak256>(&msg_bytes)
        .as_bytes()
        .to_vec();
    let sig3 = pk3
        .sign_recoverable_with_hash::<Keccak256>(&msg_bytes)
        .as_bytes()
        .to_vec();
    let sig4 = pk4
        .sign_recoverable_with_hash::<Keccak256>(&msg_bytes)
        .as_bytes()
        .to_vec();
    let sigs = vec![sig1, sig2, sig3, sig4];

    let signatures = sigs
        .into_iter()
        .map(|sig: Vec<u8>| Bytes::from(sig))
        .collect::<Vec<_>>();
    let foo = contract.is_chain_supported(1).call().await.unwrap();
    println!("foo: {:?}", foo);
    let tx = contract.execute_emergency_op_with_signatures(signatures, message);
    println!("sending tx: {:?}", tx);
    let tx_hash = tx.send().await.unwrap_err();
    let bar = tx_hash.as_revert();
    println!("bar: {:?}", bar);
    println!("Transaction sent with hash: {:?}", tx_hash);
}
