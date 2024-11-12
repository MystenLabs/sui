// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use criterion::*;

use itertools::Itertools as _;
use rand::prelude::*;
use rand::seq::SliceRandom;

use futures::future::join_all;
use prometheus::Registry;
use std::sync::Arc;
use sui_core::test_utils::{make_cert_with_large_committee, make_dummy_tx};
use sui_types::committee::Committee;
use sui_types::crypto::{get_key_pair, AccountKeyPair, AuthorityKeyPair};
use sui_types::transaction::CertifiedTransaction;

use fastcrypto_zkp::bn254::zk_login_api::ZkLoginEnv;
use sui_core::signature_verifier::*;
use sui_types::signature_verification::VerifiedDigestCache;
fn gen_certs(
    committee: &Committee,
    key_pairs: &[AuthorityKeyPair],
    count: u64,
) -> Vec<CertifiedTransaction> {
    let (receiver, _): (_, AccountKeyPair) = get_key_pair();

    let senders: Vec<_> = (0..count)
        .map(|_| get_key_pair::<AccountKeyPair>())
        .collect();

    let txns: Vec<_> = senders
        .iter()
        .map(|(sender, sender_sec)| make_dummy_tx(receiver, *sender, sender_sec))
        .collect();

    txns.iter()
        .map(|t| make_cert_with_large_committee(committee, key_pairs, t))
        .collect()
}

fn async_verifier_bench(c: &mut Criterion) {
    let (committee, key_pairs) = Committee::new_simple_test_committee_of_size(100);
    let committee = Arc::new(committee);
    let count = 200;
    let certs = gen_certs(&committee, &key_pairs, count);

    let mut group = c.benchmark_group("async_verify");

    // 8 times as many tasks as CPUs.
    let over_subscription = 32;

    // Get throughput per core
    group.throughput(Throughput::Elements(count * over_subscription));

    group.sample_size(10);

    let registry = Registry::new();
    let metrics = SignatureVerifierMetrics::new(&registry);

    let num_cpus = num_cpus::get() as u64;
    for num_threads in [1, num_cpus / 2, num_cpus] {
        for batch_size in [8, 16, 32] {
            group.bench_with_input(
                BenchmarkId::new(
                    format!("num_threads={num_threads} batch_size={batch_size}"),
                    count,
                ),
                &count,
                |b, _| {
                    let runtime = tokio::runtime::Builder::new_multi_thread()
                        .worker_threads(num_threads as usize)
                        .enable_time()
                        .build()
                        .unwrap();
                    let batch_verifier = Arc::new(SignatureVerifier::new_with_batch_size(
                        committee.clone(),
                        batch_size,
                        metrics.clone(),
                        vec![],
                        ZkLoginEnv::Test,
                        true,
                        true,
                        Some(30),
                    ));

                    b.iter(|| {
                        let handles: Vec<_> = (0..(num_threads * over_subscription))
                            .map(|_| {
                                let batch_verifier = batch_verifier.clone();
                                let certs = certs.clone();
                                runtime.spawn(async move {
                                    for c in certs.into_iter() {
                                        batch_verifier.verify_cert_skip_cache(c).await.unwrap();
                                    }
                                })
                            })
                            .collect();

                        runtime.block_on(async move {
                            join_all(handles).await;
                        });
                    })
                },
            );
        }
    }
}

fn batch_verification_bench(c: &mut Criterion) {
    let (committee, key_pairs) = Committee::new_simple_test_committee_of_size(100);

    let mut group = c.benchmark_group("batch_verify");
    // throughput improvements mostly level off at a batch size of 32, and latency starts getting
    // pretty significant at that point.
    for batch_size in [1, 4, 16, 32, 64] {
        for num_errors in [0, 1] {
            let mut certs = gen_certs(&committee, &key_pairs, batch_size);

            let (receiver, _): (_, AccountKeyPair) = get_key_pair();
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
                        batch_verify_certificates(
                            &committee,
                            &certs.iter().collect_vec(),
                            Arc::new(VerifiedDigestCache::new_empty()),
                        );
                    })
                },
            );
        }
    }
    group.finish();
}

criterion_group!(benches, batch_verification_bench, async_verifier_bench);
criterion_main!(benches);
