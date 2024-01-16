// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use ethers::abi::RawLog;
use ethers::prelude::*;
use ethers::utils::keccak256;
use fastcrypto::encoding::Encoding;
use fastcrypto::encoding::Hex;
use fastcrypto::hash::Keccak256;
use fastcrypto::secp256k1::recoverable::Secp256k1RecoverableSignature;
use fastcrypto::traits::RecoverableSignature;
use fastcrypto::traits::ToFromBytes;
use mysten_metrics::start_prometheus_server;
use serde::Deserialize;
use ethers::prelude::*;
use ethers::abi::AbiEncode;
use serde::Serialize;
use serde_json::json;
use sui_bridge::abi::Message;
use sui_bridge::types::BridgeActionType;
use sui_bridge::types::BridgeChainId;
use sui_bridge::types::MoveTypeBridgeRecord;
use sui_bridge::types::TOKEN_TRANSFER_MESSAGE_VERSION;
use sui_json_rpc_types::SuiData;
use sui_json_rpc_types::SuiObjectDataOptions;
use sui_bridge::types::MoveTypeBridgeMessageKey;
use sui_sdk::SuiClientBuilder;
use sui_types::TypeTag;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SUI_ADDRESS_LENGTH;
use sui_types::base_types::SuiAddress;
use sui_types::collection_types::LinkedTable;
use sui_types::dynamic_field::DynamicFieldName;
use sui_types::dynamic_field::Field;
use sui_types::object::Object;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use sui_bridge::abi::EthSuiBridge;
use sui_bridge::abi::EthSuiBridgeEvents;
use sui_bridge::eth_client::EthClient;

/// Rust version of the Move sui::linked_table::Node type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct LinkedTableNode<K, V> {
    pub prev: Option<K>,
    pub next: Option<K>,
    pub value: V,
}

// #[tokio::main]
// async fn main() -> anyhow::Result<()> {
//     let private_key = std::env::var("BRIDGE_TEST_PRIVATE_KEY").unwrap();
//     let url = "https://ethereum-sepolia.publicnode.com";
//     let contract_address: Address = "0xe0641F92180666337e47597Dbf94f3d4A170B9B1".parse()?;
//     println!("hi: {:?}", contract_address);

//     // Init metrics server
//     let metrics_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9192);
//     let registry_service = start_prometheus_server(metrics_address);
//     let prometheus_registry = registry_service.default_registry();
//     mysten_metrics::init_metrics(&prometheus_registry);
//     tracing::info!("Metrics server started at port {}", 9192);

//     // Init logging
//     let (_guard, _filter_handle) = telemetry_subscribers::TelemetryConfig::new()
//         .with_env()
//         .with_prom_registry(&prometheus_registry)
//         .init();
//     let provider =
//         Provider::<Http>::try_from(url)?.interval(std::time::Duration::from_millis(2000));
//     let provider = Arc::new(provider);
//     let wallet = Wallet::from_str(&private_key)?.with_chain_id(11155111u64);
//     let address = wallet.address();
//     println!("address: {:?}", address);
//     let client = SignerMiddleware::new(provider, wallet);
//     let contract = EthSuiBridge::new(contract_address, client.into());


//     let recipient_address = Address::from_str("0xb18f79Fe671db47393315fFDB377Da4Ea1B7AF96").unwrap();
//     let sender_address = SuiAddress::from_str("0x80ab1ee086210a3a37355300ca24672e81062fcdb5ced6618dab203f6a3b291c").unwrap();
//     let token_id = 1;
//     let mut payload = Vec::new();
//     // Add source address length
//     payload.push(SUI_ADDRESS_LENGTH as u8);
//     // Add source address
//     payload.extend_from_slice(&sender_address.to_vec());
//     // Add dest chain id
//     payload.push(BridgeChainId::EthSepolia as u8);
//     // Add dest address length
//     payload.push(Address::len_bytes() as u8);
//     // Add dest address
//     payload.extend_from_slice(recipient_address.as_bytes());

//     // Add token id
//     payload.push(token_id as u8);

//     // Add token amount
//     payload.extend_from_slice(&400000000u64.to_le_bytes());

//     println!("payload: {:?}", payload);

//     let message = Message {
//         message_type: BridgeActionType::TokenTransfer as u8,
//         version: 1,
//         nonce: 0,
//         chain_id: BridgeChainId::SuiTestnet as u8,
//         // payload: Bytes::from(vec![]),
//         payload: payload.into(),
//     };

//     // let public_key_bytes = Hex::decode("02321ede33d2c2d7a8a152f275a1484edef2098f034121a602cb7d767d38680aa4").unwrap();
//     // let address = Address::from_slice(&keccak256(&public_key_bytes)[12..]);
//     // println!("address 1: {:?}", Hex::encode(address));
//     // let public_key_bytes = Hex::decode("027f1178ff417fc9f5b8290bd8876f0a157a505a6c52db100a8492203ddd1d4279").unwrap();
//     // let address = Address::from_slice(&keccak256(&public_key_bytes)[12..]);
//     // println!("address 2: {:?}", Hex::encode(address));
//     // let public_key_bytes = Hex::decode("026f311bcd1c2664c14277c7a80e4857c690626597064f89edc33b8f67b99c6bc0").unwrap();
//     // let address = Address::from_slice(&keccak256(&public_key_bytes)[12..]);
//     // println!("address 3: {:?}", Hex::encode(address));
//     // let public_key_bytes = Hex::decode("03a57b85771aedeb6d31c808be9a6e73194e4b70e679608f2bca68bcc684773736").unwrap();
//     // let address = Address::from_slice(&keccak256(&public_key_bytes)[12..]);
//     // println!("address 4: {:?}", Hex::encode(address));

//     let sui_client = SuiClientBuilder::default()
//         .build("https://rpc.testnet.sui.io:443")
//         .await?;
//     // let field = sui_client.read_api().get_dynamic_field_object(
//     //     ObjectID::from_hex_literal("0x6e2ee01eab4b71fdfba00abfe4d63ab5c7b50200665e97d179d88930a478b1ba").unwrap(),
//     //     DynamicFieldName {
//     //         type_: TypeTag::from_str("0x51b2cbbab677fbf54171e86c0991ac08196733d6178dce009ba0291c38ce0ba3::message::BridgeMessageKey").unwrap(),
//     //         value: json!(
//     //             {
//     //                 "bridge_seq_num": "0",
//     //                 "message_type": 0,
//     //                 "source_chain": 1
//     //             }
//     //         )
//     //     }
//     // ).await?;
//     let object_resp = sui_client.read_api().get_object_with_options(
//         ObjectID::from_hex_literal("0xcb12287efc6f7a73ba6b8791cb5dcdb7c6ff180ceef8975e6bcb612ed21a4185").unwrap(),
//         SuiObjectDataOptions::full_content().with_bcs()
//     ).await?;
//     // let move_obj = object_resp.data.unwrap().bcs.unwrap();
//     // let bcs = move_obj.try_as_move().unwrap();
//     let object: Object = object_resp.into_object().unwrap().try_into().unwrap();
//     // println!("object: {:?}", object);
//     // let record: Field<MoveTypeBridgeMessageKey, LinkedTableNode<MoveTypeBridgeMessageKey, MoveTypeBridgeRecord>> = bcs::from_bytes(&bcs.bcs_bytes).unwrap();
//     let record: Field<MoveTypeBridgeMessageKey, LinkedTableNode<MoveTypeBridgeMessageKey, MoveTypeBridgeRecord>> = object.to_rust().unwrap();
//     let sigs = record.value.value.verified_signatures.unwrap();
//     println!("sigs: {:?}", sigs);

//     let mut message_bytes = Vec::new();
//     message_bytes.push(message.message_type);
//     message_bytes.push(message.version);
//     message_bytes.extend_from_slice(&message.nonce.to_le_bytes());
//     message_bytes.push(message.chain_id);
//     message_bytes.extend_from_slice(&message.payload.to_vec());

//     // let message_bytes = ethers::abi::encode(&[
//     //     // u8 is encoded as u256 in abi standard
//     //     // ethers::abi::Token::Uint(ethers::types::U256::from(message.message_type)),
//     //     // ethers::abi::Token::Uint(ethers::types::U256::from(message.version)),
//     //     // ethers::abi::Token::Uint(ethers::types::U256::from(message.nonce)),
//     //     // ethers::abi::Token::Uint(ethers::types::U256::from(message.chain_id)),
//     //     // ethers::abi::Token::Bytes(message.payload.to_vec()),
//     // ]);
//     println!("message_bytes: {:?}", message_bytes);
//     // message.clone().encode_packed().unwrap();
//     // let message_hash = keccak256(&message_bytes);

//     // FIXME, not in solidity code
//     let mut prefix_message = b"SUI_BRIDGE_MESSAGE".to_vec();
//     prefix_message.extend_from_slice(&message_bytes);
//     println!("prefixed message: {:?}", prefix_message);
//     println!("prefixed message: {:?}", Hex::encode(prefix_message.clone()));
//     let prefixed_message_hash = keccak256(&prefix_message);
//     println!("prefixed message hash: {:?}", Hex::encode(prefixed_message_hash.clone()));

//     for sig in sigs.clone() {
//         println!("sig: {:?}", sig);
//         println!("sig: {:?}", Hex::encode(sig.clone()));
//         let recoverable_sig = Secp256k1RecoverableSignature::from_bytes(&sig).unwrap();
//         let pk = recoverable_sig.recover_with_hash::<Keccak256>(&prefix_message).unwrap();
//         println!("recovered pk : {:?}", Hex::encode(pk.as_bytes()));
//         let signature = Signature::try_from(sig.as_slice()).unwrap();
//         println!("r: {:?}, s: {:?}, v: {:?}", signature.r, signature.s, signature.v);
//         let signer = signature.recover(RecoveryMessage::Hash(H256::from(prefixed_message_hash))).unwrap();
//         println!("recovered signer: {:?}", signer);
//     }

//     let signatures: Vec<Bytes> = sigs.into_iter().map(|sig: Vec<u8>| Bytes::from(sig)).collect();
//     // let signatures: Vec<Bytes> = vec![];
//     let tx = contract.transfer_tokens_with_signatures(signatures, message);
//     // let tx = contract
//     //     .bridge_eth_to_sui(Bytes::from(recipient_address.as_bytes().to_vec()), 0)
//     //     .value(1u64); // Amount in wei

//     // // let wallet = wallet.connect(provider.clone());

//     println!("sending tx: {:?}", tx);
//     let tx_hash = tx.send().await?;

//     println!("Transaction sent with hash: {:?}", tx_hash);
//     // tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;
//     // println!("wake up");

//     // let eth_client = EthClient::new(url).await?;
//     // let logs = eth_client
//     //     .get_events_in_range(contract_address, 5021533, 5022533)
//     //     .await
//     //     .unwrap();

//     // for log in logs {
//     //     let raw_log = RawLog {
//     //         topics: log.log.topics.clone(),
//     //         data: log.log.data.to_vec(),
//     //     };
//     //     if let Ok(decoded) = EthSuiBridgeEvents::decode_log(&raw_log) {
//     //         println!("decoded: {:?}", decoded);
//     //     } else {
//     //         println!("failed to decode log: {:?}", raw_log);
//     //     }
//     // }
//     Ok(())
// }



#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let private_key = std::env::var("BRIDGE_TEST_PRIVATE_KEY").unwrap();
    let url = "https://ethereum-sepolia.publicnode.com";
    let contract_address: Address = "0xe0641F92180666337e47597Dbf94f3d4A170B9B1".parse()?;
    println!("hi: {:?}", contract_address);

    // Init metrics server
    let metrics_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9192);
    let registry_service = start_prometheus_server(metrics_address);
    let prometheus_registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&prometheus_registry);
    tracing::info!("Metrics server started at port {}", 9192);

    // Init logging
    let (_guard, _filter_handle) = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .with_prom_registry(&prometheus_registry)
        .init();
    let provider =
        Provider::<Http>::try_from(url)?.interval(std::time::Duration::from_millis(2000));
    let provider = Arc::new(provider);
    let wallet = Wallet::from_str(&private_key)?.with_chain_id(11155111u64);
    let address = wallet.address();
    println!("address: {:?}", address);
    let client = SignerMiddleware::new(provider, wallet);
    let contract = EthSuiBridge::new(contract_address, client.into());


    let recipient_address = Address::from_str("0xb18f79Fe671db47393315fFDB377Da4Ea1B7AF96").unwrap();
    let sender_address = SuiAddress::from_str("0x80ab1ee086210a3a37355300ca24672e81062fcdb5ced6618dab203f6a3b291c").unwrap();
    let token_id = 2;
    let mut payload = Vec::new();
    // Add source address length
    payload.push(SUI_ADDRESS_LENGTH as u8);
    // Add source address
    payload.extend_from_slice(&sender_address.to_vec());
    // Add dest chain id
    payload.push(BridgeChainId::EthSepolia as u8);
    // Add dest address length
    payload.push(Address::len_bytes() as u8);
    // Add dest address
    payload.extend_from_slice(recipient_address.as_bytes());

    // Add token id
    payload.push(token_id as u8);

    // Add token amount
    payload.extend_from_slice(&250000u64.to_le_bytes());

    println!("payload: {:?}", payload);
    println!("payload hex: {:?}", Hex::encode(payload.clone()));

    let message = Message {
        message_type: BridgeActionType::TokenTransfer as u8,
        version: 1,
        nonce: 4,
        chain_id: BridgeChainId::SuiTestnet as u8,
        // payload: Bytes::from(vec![]),
        payload: payload.into(),
    };

    // let public_key_bytes = Hex::decode("02321ede33d2c2d7a8a152f275a1484edef2098f034121a602cb7d767d38680aa4").unwrap();
    // let address = Address::from_slice(&keccak256(&public_key_bytes)[12..]);
    // println!("address 1: {:?}", Hex::encode(address));
    // let public_key_bytes = Hex::decode("027f1178ff417fc9f5b8290bd8876f0a157a505a6c52db100a8492203ddd1d4279").unwrap();
    // let address = Address::from_slice(&keccak256(&public_key_bytes)[12..]);
    // println!("address 2: {:?}", Hex::encode(address));
    // let public_key_bytes = Hex::decode("026f311bcd1c2664c14277c7a80e4857c690626597064f89edc33b8f67b99c6bc0").unwrap();
    // let address = Address::from_slice(&keccak256(&public_key_bytes)[12..]);
    // println!("address 3: {:?}", Hex::encode(address));
    // let public_key_bytes = Hex::decode("03a57b85771aedeb6d31c808be9a6e73194e4b70e679608f2bca68bcc684773736").unwrap();
    // let address = Address::from_slice(&keccak256(&public_key_bytes)[12..]);
    // println!("address 4: {:?}", Hex::encode(address));

    let sui_client = SuiClientBuilder::default()
        .build("https://rpc.testnet.sui.io:443")
        .await?;
    // let field = sui_client.read_api().get_dynamic_field_object(
    //     ObjectID::from_hex_literal("0x6e2ee01eab4b71fdfba00abfe4d63ab5c7b50200665e97d179d88930a478b1ba").unwrap(),
    //     DynamicFieldName {
    //         type_: TypeTag::from_str("0x51b2cbbab677fbf54171e86c0991ac08196733d6178dce009ba0291c38ce0ba3::message::BridgeMessageKey").unwrap(),
    //         value: json!(
    //             {
    //                 "bridge_seq_num": "0",
    //                 "message_type": 0,
    //                 "source_chain": 1
    //             }
    //         )
    //     }
    // ).await?;
    let object_resp = sui_client.read_api().get_object_with_options(
        ObjectID::from_hex_literal("0xff8d21784c983e5f151a7d018f37e0b4f63dcf8f5d4fa44f2c5190dabf6e7ee3").unwrap(),
        SuiObjectDataOptions::full_content().with_bcs()
    ).await?;
    // let move_obj = object_resp.data.unwrap().bcs.unwrap();
    // let bcs = move_obj.try_as_move().unwrap();
    let object: Object = object_resp.into_object().unwrap().try_into().unwrap();
    // println!("object: {:?}", object);
    // let record: Field<MoveTypeBridgeMessageKey, LinkedTableNode<MoveTypeBridgeMessageKey, MoveTypeBridgeRecord>> = bcs::from_bytes(&bcs.bcs_bytes).unwrap();
    let record: Field<MoveTypeBridgeMessageKey, LinkedTableNode<MoveTypeBridgeMessageKey, MoveTypeBridgeRecord>> = object.to_rust().unwrap();
    let sigs = record.value.value.verified_signatures.unwrap();
    println!("sigs: {:?}", sigs);

    let mut message_bytes = Vec::new();
    message_bytes.push(message.message_type);
    message_bytes.push(message.version);
    message_bytes.extend_from_slice(&message.nonce.to_le_bytes());
    message_bytes.push(message.chain_id);
    message_bytes.extend_from_slice(&message.payload.to_vec());

    // let message_bytes = ethers::abi::encode(&[
    //     // u8 is encoded as u256 in abi standard
    //     // ethers::abi::Token::Uint(ethers::types::U256::from(message.message_type)),
    //     // ethers::abi::Token::Uint(ethers::types::U256::from(message.version)),
    //     // ethers::abi::Token::Uint(ethers::types::U256::from(message.nonce)),
    //     // ethers::abi::Token::Uint(ethers::types::U256::from(message.chain_id)),
    //     // ethers::abi::Token::Bytes(message.payload.to_vec()),
    // ]);
    println!("message_bytes: {:?}", message_bytes);
    // message.clone().encode_packed().unwrap();
    // let message_hash = keccak256(&message_bytes);

    // FIXME, not in solidity code
    let mut prefix_message = b"SUI_BRIDGE_MESSAGE".to_vec();
    prefix_message.extend_from_slice(&message_bytes);
    println!("prefixed message: {:?}", prefix_message);
    println!("prefixed message: {:?}", Hex::encode(prefix_message.clone()));
    let prefixed_message_hash = keccak256(&prefix_message);
    println!("prefixed message hash: {:?}", Hex::encode(prefixed_message_hash.clone()));

    for sig in sigs.clone() {
        println!("sig: {:?}", sig);
        println!("sig: {:?}", Hex::encode(sig.clone()));
        let recoverable_sig = Secp256k1RecoverableSignature::from_bytes(&sig).unwrap();
        let pk = recoverable_sig.recover_with_hash::<Keccak256>(&prefix_message).unwrap();
        println!("recovered pk : {:?}", Hex::encode(pk.as_bytes()));
        let signature = Signature::try_from(sig.as_slice()).unwrap();
        println!("r: {:?}, s: {:?}, v: {:?}", signature.r, signature.s, signature.v);
        let signer = signature.recover(RecoveryMessage::Hash(H256::from(prefixed_message_hash))).unwrap();
        println!("recovered signer: {:?}", signer);
    }

    let signatures: Vec<Bytes> = sigs.into_iter().map(|sig: Vec<u8>| Bytes::from(sig)).collect();
    // let signatures: Vec<Bytes> = vec![];
    let tx = contract.transfer_tokens_with_signatures(signatures, message);
    // let tx = contract
    //     .bridge_eth_to_sui(Bytes::from(recipient_address.as_bytes().to_vec()), 0)
    //     .value(1u64); // Amount in wei

    // // let wallet = wallet.connect(provider.clone());

    println!("sending tx: {:?}", tx);
    let tx_hash = tx.send().await?;

    println!("Transaction sent with hash: {:?}", tx_hash);
    // tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;
    // println!("wake up");

    // let eth_client = EthClient::new(url).await?;
    // let logs = eth_client
    //     .get_events_in_range(contract_address, 5021533, 5022533)
    //     .await
    //     .unwrap();

    // for log in logs {
    //     let raw_log = RawLog {
    //         topics: log.log.topics.clone(),
    //         data: log.log.data.to_vec(),
    //     };
    //     if let Ok(decoded) = EthSuiBridgeEvents::decode_log(&raw_log) {
    //         println!("decoded: {:?}", decoded);
    //     } else {
    //         println!("failed to decode log: {:?}", raw_log);
    //     }
    // }
    Ok(())
}