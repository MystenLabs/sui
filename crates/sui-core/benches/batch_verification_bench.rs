// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use criterion::*;

use rand::prelude::*;
use rand::seq::SliceRandom;

use sui_core::test_utils::{make_cert_with_large_committee, make_dummy_tx};
use sui_types::committee::Committee;
use sui_types::crypto::{get_key_pair, AccountKeyPair};

use sui_core::batch_bls_verifier::*;

use criterion::Criterion;

fn batch_verification_bench(c: &mut Criterion) {
    let (committee, key_pairs) = Committee::new_simple_test_committee_of_size(100);

    let mut group = c.benchmark_group("batch_verify");
    // throughput improvements mostly level off at a batch size of 32, and latency starts getting
    // pretty significant at that point.
    for batch_size in [1, 4, 16, 32, 64] {
        for num_errors in [0, 1] {
            let (receiver, _): (_, AccountKeyPair) = get_key_pair();

            let senders: Vec<_> = (0..batch_size)
                .into_iter()
                .map(|_| get_key_pair::<AccountKeyPair>())
                .collect();

            let txns: Vec<_> = senders
                .iter()
                .map(|(sender, sender_sec)| make_dummy_tx(receiver, *sender, sender_sec))
                .collect();

            let mut certs: Vec<_> = txns
                .iter()
                .map(|t| make_cert_with_large_committee(&committee, &key_pairs, t))
                .collect();

            let (other_sender, other_sender_sec): (_, AccountKeyPair) = get_key_pair();
            let other_tx = make_dummy_tx(receiver, other_sender, &other_sender_sec);
            let other_cert = make_cert_with_large_committee(&committee, &key_pairs, &other_tx);

            for cert in certs.iter_mut().take(num_errors as usize) {
                *cert.auth_sig_mut_for_testing() = other_cert.auth_sig().clone();
            }

            group.throughput(Throughput::Elements(batch_size));
            group.bench_with_input(
                BenchmarkId::from_parameter(format!("size={} errors={}", batch_size, num_errors)),
                &batch_size,
                |b, batch_size| {
                    assert_eq!(certs.len() as u64, *batch_size);
                    b.iter(|| {
                        certs.shuffle(&mut thread_rng());
                        batch_verify_certificates(&committee, &certs);
                    })
                },
            );
        }
    }
    group.finish();
}

criterion_group!(benches, batch_verification_bench);
criterion_main!(benches);
