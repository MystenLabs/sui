// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Benchmarks comparing ECDSA verification over P-256 (secp256r1) and P-384 (secp384r1),
//! used to ground the gas cost of the `ecdsa_p384` native relative to the already-priced
//! `ecdsa_r1` native.
//!
//! Both benchmarks exercise the exact code path the corresponding Move native uses:
//!   * `p256_verify_sha256` -> fastcrypto secp256r1 `verify_with_hash::<Sha256>` (ecdsa_r1)
//!   * `p384_verify_sha256` / `p384_verify_sha384` -> `verify_secp384r1` (ecdsa_p384)
//!
//! Methodology: the P-256 verify base cost is priced at 4225 gas. Scaling that by the
//! measured P-384/P-256 verification-time ratio yields a benchmarked P-384 base cost without
//! needing the absolute gas-per-time constant (which is set by reference-validator hardware).
//!
//! Run with: `cargo bench -p sui-types --bench ecdsa_p384_bench`

use criterion::*;
use fastcrypto::hash::Sha256 as FcSha256;
use fastcrypto::secp256r1::Secp256r1KeyPair;
use fastcrypto::traits::{KeyPair, Signer};
use p384::ecdsa::signature::Signer as _;
use p384::ecdsa::signature::hazmat::PrehashSigner;
use p384::ecdsa::{Signature as P384Signature, SigningKey as P384SigningKey};
use rand::SeedableRng;
use rand::rngs::StdRng;
use sha2::{Digest, Sha256};
use sui_types::ecdsa_p384::{P384Hash, verify_secp384r1};

fn ecdsa_verify_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("ecdsa_verify");
    let msg: &[u8] = b"benchmark message for ecdsa verification";

    // --- P-256: fastcrypto secp256r1, exactly the ecdsa_r1 native's verify path. ---
    let mut rng = StdRng::from_seed([13u8; 32]);
    let kp = Secp256r1KeyPair::generate(&mut rng);
    let p256_pk = kp.public().clone();
    let p256_sig = kp.sign(msg); // fastcrypto signs with the default hash (SHA-256)
    group.bench_function("p256_verify_sha256", |b| {
        b.iter(|| {
            p256_pk
                .verify_with_hash::<FcSha256>(black_box(msg), &p256_sig)
                .expect("p256 verify");
        })
    });

    // --- P-384: RustCrypto p384, exactly the ecdsa_p384 native's verify path. ---
    let p384_sk = P384SigningKey::from_slice(&[0x42u8; 48]).unwrap();
    let p384_pk = p384_sk
        .verifying_key()
        .to_encoded_point(true)
        .as_bytes()
        .to_vec();
    let p384_sig_sha384: P384Signature = p384_sk.sign(msg);
    let p384_sig_sha256: P384Signature = p384_sk
        .sign_prehash(Sha256::digest(msg).as_slice())
        .unwrap();
    let sig384 = p384_sig_sha384.to_bytes().to_vec();
    let sig256 = p384_sig_sha256.to_bytes().to_vec();

    group.bench_function("p384_verify_sha256", |b| {
        b.iter(|| {
            verify_secp384r1(&sig256, &p384_pk, black_box(msg), P384Hash::Sha256)
                .expect("p384 sha256 verify");
        })
    });
    group.bench_function("p384_verify_sha384", |b| {
        b.iter(|| {
            verify_secp384r1(&sig384, &p384_pk, black_box(msg), P384Hash::Sha384)
                .expect("p384 sha384 verify");
        })
    });

    group.finish();
}

criterion_group!(benches, ecdsa_verify_benchmark);
criterion_main!(benches);
