// Copyright(C) 2022, Mysten Labs
// SPDX-License-Identifier: Apache-2.0
#[macro_use]
extern crate criterion;
extern crate ed25519_dalek;
extern crate rand;

mod ed25519_benches {
    use super::*;
    use criterion::*;
    use crypto::{
        bls12377::{BLS12377KeyPair, BLS12377Signature},
        ed25519::*,
        traits::{KeyPair, VerifyingKey},
        Verifier,
    };
    use ed25519_dalek::Digest as _;
    use rand::{prelude::ThreadRng, thread_rng};
    use signature::Signer;

    fn sign(c: &mut Criterion) {
        let mut csprng: ThreadRng = thread_rng();
        let ed_keypair = Ed25519KeyPair::generate(&mut csprng);
        let bls_keypair = BLS12377KeyPair::generate(&mut csprng);
        let msg: &[u8] = b"";

        c.bench_function("Ed25519 signing", move |b| b.iter(|| ed_keypair.sign(msg)));
        c.bench_function("BLS12377 signing", move |b| {
            b.iter(|| bls_keypair.sign(msg))
        });
    }

    fn verify(c: &mut Criterion) {
        let mut csprng: ThreadRng = thread_rng();
        let ed_keypair = Ed25519KeyPair::generate(&mut csprng);
        let bls_keypair = BLS12377KeyPair::generate(&mut csprng);
        let ed_public = ed_keypair.public();
        let bls_public = bls_keypair.public();
        let msg: &[u8] = b"";
        let ed_sig: Ed25519Signature = ed_keypair.sign(msg);
        let bls_sig: BLS12377Signature = bls_keypair.sign(msg);

        c.bench_function("Ed25519 signature verification", move |b| {
            b.iter(|| ed_public.verify(msg, &ed_sig))
        });
        c.bench_function("BLS12377 signature verification", move |b| {
            b.iter(|| bls_public.verify(msg, &bls_sig))
        });
    }

    fn verify_batch_signatures<M: measurement::Measurement>(c: &mut BenchmarkGroup<M>) {
        static BATCH_SIZES: [usize; 14] = [
            16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072,
        ];

        let mut csprng: ThreadRng = thread_rng();

        for size in BATCH_SIZES.iter() {
            let ed_keypairs: Vec<_> = (0..*size)
                .map(|_| Ed25519KeyPair::generate(&mut csprng))
                .collect();
            let bls_keypairs: Vec<_> = (0..*size)
                .map(|_| BLS12377KeyPair::generate(&mut csprng))
                .collect();
            let msg: Vec<u8> = {
                let mut h = ed25519_dalek::Sha512::new();
                h.update(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
                h.finalize().to_vec()
            };
            let ed_signatures: Vec<_> = ed_keypairs.iter().map(|key| key.sign(&msg)).collect();
            let ed_public_keys: Vec<_> =
                ed_keypairs.iter().map(|key| key.public().clone()).collect();
            let bls_signatures: Vec<_> = bls_keypairs.iter().map(|key| key.sign(&msg)).collect();
            let bls_public_keys: Vec<_> = bls_keypairs
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
                &(msg, bls_public_keys, bls_signatures),
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
