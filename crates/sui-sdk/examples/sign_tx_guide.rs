// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod utils;
use crate::utils::request_tokens_from_faucet;
use fastcrypto::encoding::Encoding;
use fastcrypto::hash::HashFunction;
use fastcrypto::{
    ed25519::Ed25519KeyPair,
    encoding::Base64,
    secp256k1::Secp256k1KeyPair,
    secp256r1::Secp256r1KeyPair,
    traits::{EncodeDecodeBase64, KeyPair},
};
use rand::{rngs::StdRng, SeedableRng};
use shared_crypto::intent::{Intent, IntentMessage};
use sui_sdk::{
    rpc_types::SuiTransactionBlockResponseOptions,
    types::{
        programmable_transaction_builder::ProgrammableTransactionBuilder,
        transaction::TransactionData,
    },
    SuiClientBuilder,
};
use sui_types::crypto::Signer;
use sui_types::crypto::ToFromBytes;
use sui_types::signature::GenericSignature;
use sui_types::{
    base_types::SuiAddress,
    crypto::{get_key_pair_from_rng, SuiKeyPair},
};

// This example walks through the Rust SDK usecase in sign-txn.mdx.

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // set up sui client for the desired network.
    let sui_client = SuiClientBuilder::default().build_devnet().await?;

    // deterministically generate a keypair, testing only, do not use for mainnet.
    let skp_determ_0 =
        SuiKeyPair::Ed25519(Ed25519KeyPair::generate(&mut StdRng::from_seed([0; 32])));
    let _skp_determ_1 =
        SuiKeyPair::Secp256k1(Secp256k1KeyPair::generate(&mut StdRng::from_seed([0; 32])));
    let _skp_determ_2 =
        SuiKeyPair::Secp256r1(Secp256r1KeyPair::generate(&mut StdRng::from_seed([0; 32])));

    // randomly generate a keypair.
    let _skp_rand_0 = SuiKeyPair::Ed25519(get_key_pair_from_rng(&mut rand::rngs::OsRng).1);
    let _skp_rand_1 = SuiKeyPair::Secp256k1(get_key_pair_from_rng(&mut rand::rngs::OsRng).1);
    let _skp_rand_2 = SuiKeyPair::Secp256r1(get_key_pair_from_rng(&mut rand::rngs::OsRng).1);

    // import a keypair from a base64 encoded 32-byte `private key`.
    let _skp_import_no_flag_0 = SuiKeyPair::Ed25519(
        Ed25519KeyPair::from_bytes(
            &Base64::decode("1GPhHHkVlF6GrCty2IuBkM+tj/e0jn64ksJ1pc8KPoI=").unwrap(),
        )
        .unwrap(),
    );
    let _skp_import_no_flag_1 = SuiKeyPair::Ed25519(
        Ed25519KeyPair::from_bytes(
            &Base64::decode("1GPhHHkVlF6GrCty2IuBkM+tj/e0jn64ksJ1pc8KPoI=").unwrap(),
        )
        .unwrap(),
    );
    let _skp_import_no_flag_2 = SuiKeyPair::Ed25519(
        Ed25519KeyPair::from_bytes(
            &Base64::decode("1GPhHHkVlF6GrCty2IuBkM+tj/e0jn64ksJ1pc8KPoI=").unwrap(),
        )
        .unwrap(),
    );

    // import a keypair from a base64 encoded 33-byte `flag || private key`. The signature scheme is determined by the flag.
    let _skp_import_with_flag_0 =
        SuiKeyPair::decode_base64("ANRj4Rx5FZRehqwrctiLgZDPrY/3tI5+uJLCdaXPCj6C").unwrap();
    let _skp_import_with_flag_1 =
        SuiKeyPair::decode_base64("AdRj4Rx5FZRehqwrctiLgZDPrY/3tI5+uJLCdaXPCj6C").unwrap();
    let _skp_import_with_flag_2 =
        SuiKeyPair::decode_base64("AtRj4Rx5FZRehqwrctiLgZDPrY/3tI5+uJLCdaXPCj6C").unwrap();

    // replace `skp_determ_0` with the variable names above
    let pk = skp_determ_0.public();
    let sender = SuiAddress::from(&pk);
    println!("Sender: {:?}", sender);

    // make sure the sender has a gas coin as an example.
    let coin = sui_client
        .coin_read_api()
        .get_coins(sender, None, None, None)
        .await?
        .data
        .into_iter()
        .next();

    if coin.is_none() {
        println!("Gas coin not found. Please check this doc for how to request coins: https://docs.sui.io/guides/developer/getting-started/get-coins");
        return Ok(());
    }
    let gas_coin = coin.unwrap();

    // construct an example programmable transaction.
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.pay_sui(vec![sender], vec![1]).unwrap();
        builder.finish()
    };

    let gas_budget = 5_000_000;
    let gas_price = sui_client.read_api().get_reference_gas_price().await?;

    // create the transaction data that will be sent to the network.
    let tx_data = TransactionData::new_programmable(
        sender,
        vec![gas_coin.object_ref()],
        pt,
        gas_budget,
        gas_price,
    );

    // derive the digest that the keypair should sign on.
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data);
    let raw_tx = bcs::to_bytes(&intent_msg).expect("bcs should not fail");
    let mut hasher = sui_types::crypto::DefaultHash::default();
    hasher.update(raw_tx.clone());
    let digest = hasher.finalize().digest;

    // use SuiKeyPair to sign the digest.
    let sui_sig = skp_determ_0.sign(&digest);

    // execute the transaction.
    let transaction_response = sui_client
        .quorum_driver_api()
        .execute_transaction_block(
            sui_types::transaction::Transaction::from_generic_sig_data(
                intent_msg.value,
                Intent::sui_transaction(),
                vec![GenericSignature::Signature(sui_sig)],
            ),
            SuiTransactionBlockResponseOptions::default(),
            None,
        )
        .await?;

    println!(
        "Transaction executed. Transaction digest: {}",
        transaction_response.digest.base58_encode()
    );
    println!("{transaction_response}");
    Ok(())
}
