// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use criterion::*;

use sui_core::batch_bls_verifier::{BatchCertificateVerifierMetrics, VerifiedCertificateCache};
use sui_types::digests::CertificateDigest;

use criterion::Criterion;

fn verified_cert_cache_bench(c: &mut Criterion) {
    let mut digests: Vec<_> = (0..(1 << 18))
        .map(|_| CertificateDigest::random())
        .collect();
    digests.extend_from_slice(&digests.clone());
    rand::seq::SliceRandom::shuffle(digests.as_mut_slice(), &mut rand::rngs::OsRng);

    let cpus = num_cpus::get();
    let chunk_size = digests.len() / cpus;
    let chunks: Vec<Vec<CertificateDigest>> = digests
        .chunks(chunk_size)
        .take(cpus)
        .map(|c| c.to_vec())
        .collect();
    assert_eq!(chunks.len(), cpus);

    let registry = prometheus::Registry::new();
    let metrics = BatchCertificateVerifierMetrics::new(&registry);
    let cache = VerifiedCertificateCache::new(metrics);

    let mut group = c.benchmark_group("digest-caching");
    group.throughput(Throughput::Elements(chunk_size as u64));

    group.bench_function("digest cache", |b| {
        b.iter(|| {
            std::thread::scope(|s| {
                let threads = chunks.iter().map(|chunk| {
                    s.spawn(|| {
                        for digest in &**chunk {
                            if cache.is_cert_verified(digest) {
                                continue;
                            } else {
                                cache.cache_cert_verified(*digest);
                            }
                        }
                    })
                });

                for thread in threads {
                    thread.join().unwrap();
                }
            });
        });
    });
    group.finish();
}

criterion_group!(benches, verified_cert_cache_bench);
criterion_main!(benches);
