// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::hash::MultisetHash;
use sui_types::base_types::ObjectDigest;
use sui_types::object_state_hash::ObjectStateHash;

use criterion::*;

fn object_state_hash_benchmark(c: &mut Criterion) {
    {
        let digests: Vec<_> = (0..1_000).map(|_| ObjectDigest::random()).collect();
        let mut state_hash = ObjectStateHash::default();

        let mut group = c.benchmark_group("object_state_hash");
        group.throughput(Throughput::Elements(digests.len() as u64));

        group.bench_function("accumulate_digests", |b| {
            b.iter(|| state_hash.insert_all(&digests))
        });
    }

    {
        let mut group = c.benchmark_group("object_state_hash");
        group.throughput(Throughput::Elements(1));

        let mut state_hash = ObjectStateHash::default();
        let point = {
            let digest = ObjectDigest::random();
            let mut state_hash = ObjectStateHash::default();
            state_hash.insert(digest);
            state_hash
        };

        let serialized = bcs::to_bytes(&point).unwrap();

        group.bench_function("sum_object_state_hashes", |b| {
            b.iter(|| state_hash.union(&point))
        });
        group.bench_function("serialize_object_state_hashes", |b| {
            b.iter(|| bcs::to_bytes(&state_hash).unwrap())
        });
        group.bench_function("deserialize_object_state_hashes", |b| {
            b.iter(|| bcs::from_bytes::<ObjectStateHash>(&serialized).unwrap())
        });
    }
}

criterion_group!(benches, object_state_hash_benchmark);
criterion_main!(benches);
