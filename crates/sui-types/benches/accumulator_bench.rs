// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[macro_use]
extern crate criterion;

use fastcrypto::hash::MultisetHash;
use sui_types::accumulator::Accumulator;
use sui_types::base_types::ObjectDigest;

use criterion::Criterion;

fn accumulator_benchmark(c: &mut Criterion) {
    let digests: Vec<_> = (0..1_000).map(|_| ObjectDigest::random()).collect();
    let mut accumulator = Accumulator::default();

    c.bench_function("accumulate_digests", |b| {
        b.iter(|| accumulator.insert_all(&digests))
    });

    let mut accumulator = Accumulator::default();
    let point = {
        let digest = ObjectDigest::random();
        let mut accumulator = Accumulator::default();
        accumulator.insert(digest);
        accumulator
    };
    c.bench_function("sum_accumulators", |b| b.iter(|| accumulator.union(&point)));
}

criterion_group!(benches, accumulator_benchmark);
criterion_main!(benches);
