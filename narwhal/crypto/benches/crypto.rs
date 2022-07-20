// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#[macro_use]
extern crate criterion;
extern crate ed25519_dalek;
extern crate rand;

mod ed25519_benches {
    use super::*;
    use blake2::digest::Update;
    use criterion::*;
    use crypto::{
        bls12377::{BLS12377KeyPair, BLS12377Signature},
        bls12381::{BLS12381KeyPair, BLS12381Signature},
        ed25519::*,
        secp256k1::{Secp256k1KeyPair, Secp256k1Signature},
        traits::{KeyPair, VerifyingKey},
        Verifier,
    };
    use rand::{prelude::ThreadRng, thread_rng};
    use signature::Signer;

    fn sign(c: &mut Criterion) {
        let mut csprng: ThreadRng = thread_rng();
        let ed_keypair = Ed25519KeyPair::generate(&mut csprng);
        let bls_keypair = BLS12377KeyPair::generate(&mut csprng);
        let blst_keypair = BLS12381KeyPair::generate(&mut csprng);
        let secp256k1_keypair = Secp256k1KeyPair::generate(&mut csprng);
        let msg: &[u8] = b"";

        c.bench_function("Ed25519 signing", move |b| b.iter(|| ed_keypair.sign(msg)));
        c.bench_function("BLS12377 signing", move |b| {
            b.iter(|| bls_keypair.sign(msg))
        });
        c.bench_function("BLS12381 signing", move |b| {
            b.iter(|| blst_keypair.sign(msg))
        });
        c.bench_function("Secp256k1 signing", move |b| {
            b.iter(|| secp256k1_keypair.sign(msg))
        });
    }

    fn verify(c: &mut Criterion) {
        let mut csprng: ThreadRng = thread_rng();
        let ed_keypair = Ed25519KeyPair::generate(&mut csprng);
        let bls_keypair = BLS12377KeyPair::generate(&mut csprng);
        let blst_keypair = BLS12381KeyPair::generate(&mut csprng);
        let secp256k1_keypair = Secp256k1KeyPair::generate(&mut csprng);

        let ed_public = ed_keypair.public();
        let bls_public = bls_keypair.public();
        let blst_public = blst_keypair.public();
        let secp256k1_public = secp256k1_keypair.public();

        let msg: &[u8] = b"";
        let ed_sig: Ed25519Signature = ed_keypair.sign(msg);
        let bls_sig: BLS12377Signature = bls_keypair.sign(msg);
        let blst_sig: BLS12381Signature = blst_keypair.sign(msg);
        let secp256k1_sig: Secp256k1Signature = secp256k1_keypair.sign(msg);

        c.bench_function("Ed25519 signature verification", move |b| {
            b.iter(|| ed_public.verify(msg, &ed_sig))
        });
        c.bench_function("BLS12377 signature verification", move |b| {
            b.iter(|| bls_public.verify(msg, &bls_sig))
        });
        c.bench_function("BLS12381 signature verification", move |b| {
            b.iter(|| blst_public.verify(msg, &blst_sig))
        });
        c.bench_function("Secp256k1 signature verification", move |b| {
            b.iter(|| secp256k1_public.verify(msg, &secp256k1_sig))
        });
    }

    fn verify_batch_signatures<M: measurement::Measurement>(c: &mut BenchmarkGroup<M>) {
        static BATCH_SIZES: [usize; 10] = [16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192];

        let mut csprng: ThreadRng = thread_rng();

        for size in BATCH_SIZES.iter() {
            let ed_keypairs: Vec<_> = (0..*size)
                .map(|_| Ed25519KeyPair::generate(&mut csprng))
                .collect();
            let bls_keypairs: Vec<_> = (0..*size)
                .map(|_| BLS12377KeyPair::generate(&mut csprng))
                .collect();
            let blst_keypairs: Vec<_> = (0..*size)
                .map(|_| BLS12381KeyPair::generate(&mut csprng))
                .collect();

            let msg: Vec<u8> = {
                crypto::blake2b_256(|hasher| {
                    hasher.update(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                })
                .to_vec()
            };

            let ed_signatures: Vec<_> = ed_keypairs.iter().map(|key| key.sign(&msg)).collect();
            let ed_public_keys: Vec<_> =
                ed_keypairs.iter().map(|key| key.public().clone()).collect();
            let bls_signatures: Vec<_> = bls_keypairs.iter().map(|key| key.sign(&msg)).collect();
            let bls_public_keys: Vec<_> = bls_keypairs
                .iter()
                .map(|key| key.public().clone())
                .collect();
            let blst_signatures: Vec<_> = blst_keypairs.iter().map(|key| key.sign(&msg)).collect();
            let blst_public_keys: Vec<_> = blst_keypairs
                .iter()
                .map(|key| key.public().clone())
                .collect();

            c.bench_with_input(
                BenchmarkId::new("Ed25519 batch verification", *size),
                &(msg.clone(), ed_public_keys, ed_signatures),
                |b, i| {
                    b.iter(|| VerifyingKey::verify_batch(&i.0, &i.1[..], &i.2[..]));
                },
            );
            c.bench_with_input(
                BenchmarkId::new("BLS12377 batch verification", *size),
                &(msg.clone(), bls_public_keys, bls_signatures),
                |b, i| {
                    b.iter(|| VerifyingKey::verify_batch(&i.0, &i.1[..], &i.2[..]));
                },
            );
            c.bench_with_input(
                BenchmarkId::new("BLS12381 batch verification", *size),
                &(msg, blst_public_keys, blst_signatures),
                |b, i| {
                    b.iter(|| VerifyingKey::verify_batch(&i.0, &i.1[..], &i.2[..]));
                },
            );
        }
    }

    fn key_generation(c: &mut Criterion) {
        let mut csprng: ThreadRng = thread_rng();

        c.bench_function("Ed25519 keypair generation", move |b| {
            b.iter(|| Ed25519KeyPair::generate(&mut csprng))
        });
        c.bench_function("BLS12377 keypair generation", move |b| {
            b.iter(|| BLS12377KeyPair::generate(&mut csprng))
        });
        c.bench_function("BLS12381 keypair generation", move |b| {
            b.iter(|| BLS12381KeyPair::generate(&mut csprng))
        });
        c.bench_function("Secp256k1 keypair generation", move |b| {
            b.iter(|| Secp256k1KeyPair::generate(&mut csprng))
        });
    }

    criterion_group! {
        name = ed25519_benches;
        config = Criterion::default();
        targets =
           sign,
           verify,
           verification_comparison,
           key_generation
    }

    fn verification_comparison(c: &mut Criterion) {
        let mut group: BenchmarkGroup<_> = c.benchmark_group("verification_comparison");
        group.sampling_mode(SamplingMode::Flat);

        verify_batch_signatures(&mut group);
        group.finish();
    }
}

criterion_main!(ed25519_benches::ed25519_benches,);
