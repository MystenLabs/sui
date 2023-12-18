// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::start_prometheus_server;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use sui_bridge::server::run_server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Init metrics server
    let metrics_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9184);
    let registry_service = start_prometheus_server(metrics_address);
    let prometheus_registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&prometheus_registry);

    // Init logging
    let (_guard, _filter_handle) = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .with_prom_registry(&prometheus_registry)
        .init();

    // TODO: allow configuration of port
    let socket_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9000);
    run_server(&socket_address).await;
    Ok(())
}

#[test]
fn create_test_data_for_move_code() {
    use ethers::core::rand::rngs::StdRng;
    use ethers::core::rand::SeedableRng;
    use ethers::utils::hex;
    use fastcrypto::encoding::decode_bytes_hex;
    use fastcrypto::hash::Keccak256;
    use fastcrypto::secp256k1::Secp256k1KeyPair;
    use fastcrypto::traits::RecoverableSigner;
    use fastcrypto::traits::{KeyPair, ToFromBytes};

    // Bridge message bytes created by move
    let bridge_msg = "0x00010a0000000000000000200000000000000000000000000000000000000000000000000000000000000064012000000000000000000000000000000000000000000000000000000000000000c8033930000000000000";
    let mut msg = "SUI_BRIDGE_MESSAGE".as_bytes().to_vec();
    msg.append(&mut decode_bytes_hex::<Vec<u8>>(bridge_msg).unwrap());

    let keypair1 = Secp256k1KeyPair::generate(&mut StdRng::from_seed([0; 32]));
    let keypair2 = Secp256k1KeyPair::generate(&mut StdRng::from_seed([1; 32]));
    let keypair3 = Secp256k1KeyPair::generate(&mut StdRng::from_seed([2; 32]));

    for keypair in [keypair1, keypair2, keypair3] {
        let sig = keypair.sign_recoverable_with_hash::<Keccak256>(&msg);
        println!(
            "{:?} : {:?}",
            hex::encode(keypair.public.as_bytes()),
            hex::encode(sig.as_bytes())
        );
    }
}

#[tokio::test]
async fn test_localnet() {
    use ethers::core::rand::rngs::StdRng;
    use ethers::core::rand::SeedableRng;
    use fastcrypto::secp256k1::Secp256k1KeyPair;
    use fastcrypto::traits::KeyPair;
    use std::str::FromStr;
    use sui_keys::keystore::AccountKeystore;

    use fastcrypto::hash::Keccak256;
    use fastcrypto::traits::{RecoverableSigner, ToFromBytes};
    use move_core_types::ident_str;
    use move_core_types::language_storage::TypeTag;
    use serde::Serialize;

    use shared_crypto::intent::Intent;
    use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
    use sui_keys::keystore::{InMemKeystore, Keystore};
    use sui_sdk::SuiClientBuilder;
    use sui_types::base_types::{SequenceNumber, SuiAddress};
    use sui_types::crypto::SignatureScheme;
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::transaction::{ObjectArg, Transaction, TransactionData};
    use sui_types::{BRIDGE_PACKAGE_ID, SUI_BRIDGE_OBJECT_ID};

    let client = SuiClientBuilder::default()
        .build("http://localhost:9000")
        .await
        .unwrap();

    #[derive(Serialize)]
    #[serde(untagged)]
    enum Payload {
        Token {
            sender_address: Vec<u8>,
            target_chain: u8,
            target_address: Vec<u8>,
            token_type: u8,
            amount: u64,
        },
    }
    #[derive(Serialize)]
    struct BridgeMessage {
        message_type: u8,
        message_version: u8,
        seq_num: u64,
        source_chain: u8,
        payload: Payload,
    }
    let sender_address = SuiAddress::random_for_testing_only().to_vec();
    let target_address =
        SuiAddress::from_str("0xea34c66bd61aae5032b9f8e6c6a47d55859fa3a7dc0be0693ec616410a1b4dbf")
            .unwrap()
            .to_vec();
    let msg = BridgeMessage {
        message_type: 0,
        message_version: 1,
        seq_num: 10,
        source_chain: 11,
        payload: Payload::Token {
            // dummy address
            sender_address: sender_address.clone(),
            target_chain: 0,
            target_address: target_address.clone(),
            token_type: 1,
            amount: 12345,
        },
    };

    let mut msg_bytes = "SUI_BRIDGE_MESSAGE".as_bytes().to_vec();
    msg_bytes.append(&mut bcs::to_bytes(&msg).unwrap());
    let keypair1 = Secp256k1KeyPair::generate(&mut StdRng::from_seed([0; 32]));
    let signature = keypair1.sign_recoverable_with_hash::<Keccak256>(&msg_bytes);

    let mut builder = ProgrammableTransactionBuilder::new();
    let source_chain = builder.pure(11u8).unwrap();
    let seq_num = builder.pure(10u64).unwrap();
    let sender = builder.pure(sender_address).unwrap();
    let target_chain = builder.pure(0u8).unwrap();
    let target = builder.pure(target_address).unwrap();
    let token_type = builder.pure(1u8).unwrap();
    let amount = builder.pure(12345u64).unwrap();

    let msg_arg = builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("message").to_owned(),
        ident_str!("create_token_bridge_message").to_owned(),
        vec![],
        vec![
            source_chain,
            seq_num,
            sender,
            target_chain,
            target,
            token_type,
            amount,
        ],
    );

    let bridge = builder
        .obj(ObjectArg::SharedObject {
            id: SUI_BRIDGE_OBJECT_ID,
            initial_shared_version: SequenceNumber::from_u64(5),
            mutable: true,
        })
        .unwrap();
    let signatures = builder.pure(vec![signature.as_bytes().to_vec()]).unwrap();

    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("bridge").to_owned(),
        ident_str!("approve_bridge_message").to_owned(),
        vec![],
        vec![bridge, msg_arg, signatures],
    );

    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("bridge").to_owned(),
        ident_str!("claim_and_transfer_token").to_owned(),
        vec![TypeTag::from_str("0xb::btc::BTC").unwrap()],
        vec![bridge, source_chain, seq_num],
    );

    // key for 0xea34c66bd61aae5032b9f8e6c6a47d55859fa3a7dc0be0693ec616410a1b4dbf
    let phrase =
        "panic lunch engine occur situate morning ranch copper wood kangaroo twelve junior";
    let mut keystore = Keystore::InMem(InMemKeystore::new_insecure_for_tests(0));
    let address = keystore
        .import_from_mnemonic(phrase, SignatureScheme::ED25519, None)
        .unwrap();

    let ptb = builder.finish();

    let coin = client
        .coin_read_api()
        .select_coins(address, None, 100000000, vec![])
        .await
        .unwrap();

    let gas_price = client.read_api().get_reference_gas_price().await.unwrap();
    let data = TransactionData::new_programmable(
        address,
        vec![coin.first().unwrap().object_ref()],
        ptb,
        100000000,
        gas_price,
    );
    let signature = keystore
        .sign_secure(&address, &data, Intent::sui_transaction())
        .unwrap();
    let tx = Transaction::from_data(data, vec![signature]);

    let response = client
        .quorum_driver_api()
        .execute_transaction_block(tx, SuiTransactionBlockResponseOptions::full_content(), None)
        .await
        .unwrap();

    println!("{:?}", response)
}
