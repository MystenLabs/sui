// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput,
};
use fastcrypto::hash::Hash;
use narwhal_types as types;
use rand::Rng;
use test_utils::latest_protocol_version;
use types::Batch;

pub fn batch_digest(c: &mut Criterion) {
    let mut digest_group = c.benchmark_group("Batch digests");
    digest_group.sampling_mode(SamplingMode::Flat);

    static BATCH_SIZES: [usize; 4] = [100, 500, 1000, 5000];

    for size in BATCH_SIZES {
        let tx_gen = || {
            (0..512)
                .map(|_| rand::thread_rng().gen())
                .collect::<Vec<u8>>()
        };
        let batch = Batch::new(
            (0..size).map(|_| tx_gen()).collect::<Vec<_>>(),
            &latest_protocol_version(),
        );
        digest_group.throughput(Throughput::Bytes(512 * size as u64));
        digest_group.bench_with_input(BenchmarkId::new("batch digest", size), &batch, |b, i| {
            b.iter(|| i.digest())
        });
    }
}

criterion_group! {
    name = consensus_group;
    config = Criterion::default();
    targets = batch_digest
}
criterion_main!(consensus_group);
