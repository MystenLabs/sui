// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Benchmark: unverified vs. formally-verified signature verification.
//!
//! Both functions perform identical cryptographic work. The verified version
//! adds a thin abstraction layer (VerifiableSig wrapper + address pre-derivation
//! pass). This benchmark confirms there is no meaningful overhead.

use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use fastcrypto::traits::KeyPair as KeypairTraits;
use rand::SeedableRng;
use rand::rngs::StdRng;

use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::{AccountKeyPair, SuiKeyPair, get_key_pair_from_rng};
use sui_types::digests::ZKLoginInputsDigest;
use sui_types::object::Object;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::signature::VerifyParams;
use sui_types::signature_verification::{
    VerifiedDigestCache, verify_sender_signed_data_message_signatures,
    verify_sender_signed_data_message_signatures_verified,
};
use sui_types::transaction::{
    SenderSignedData, TEST_ONLY_GAS_UNIT_FOR_TRANSFER, Transaction, TransactionData,
};

/// Build a `SenderSignedData` signed by a fresh Ed25519 key.
fn make_signed_data(rng: &mut StdRng) -> SenderSignedData {
    let (sender_addr, kp): (SuiAddress, AccountKeyPair) = get_key_pair_from_rng(rng);
    let sui_kp = SuiKeyPair::Ed25519(kp);

    let recipient = SuiAddress::from(ObjectID::from_bytes([1u8; 32]).unwrap());
    let gas_object = Object::immutable_with_id_for_testing(ObjectID::random_from_rng(rng));

    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.transfer_sui(recipient, None);
        builder.finish()
    };

    let tx_data = TransactionData::new_programmable(
        sender_addr,
        vec![gas_object.compute_object_reference()],
        pt,
        TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        1,
    );

    Transaction::from_data_and_signer(tx_data, vec![&sui_kp]).into_data()
}

/// Minimal VerifyParams for Ed25519 (no zklogin JWKs needed).
fn make_verify_params() -> VerifyParams {
    use fastcrypto_zkp::bn254::zk_login::OIDCProvider;
    use fastcrypto_zkp::bn254::zk_login_api::ZkLoginEnv;
    VerifyParams::new(
        Default::default(),
        vec![OIDCProvider::Google],
        ZkLoginEnv::Prod,
        false,
        false,
        false,
        None,
        false,
        false,
    )
}

fn sig_verification_benchmark(c: &mut Criterion) {
    let mut rng = StdRng::from_seed([42u8; 32]);
    let txn = make_signed_data(&mut rng);
    let verify_params = make_verify_params();
    let cache: Arc<VerifiedDigestCache<ZKLoginInputsDigest>> =
        Arc::new(VerifiedDigestCache::new_empty());
    const EPOCH: u64 = 0;

    let mut group = c.benchmark_group("sig_verification/1-signer-ed25519");

    group.bench_function("unverified", |b| {
        b.iter(|| {
            verify_sender_signed_data_message_signatures(
                &txn,
                EPOCH,
                &verify_params,
                cache.clone(),
                vec![],
            )
            .unwrap()
        })
    });

    group.bench_function("verified", |b| {
        b.iter(|| {
            verify_sender_signed_data_message_signatures_verified(
                &txn,
                EPOCH,
                &verify_params,
                cache.clone(),
                vec![],
            )
            .unwrap()
        })
    });

    group.finish();
}

criterion_group!(benches, sig_verification_benchmark);
criterion_main!(benches);
