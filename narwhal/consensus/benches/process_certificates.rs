// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use consensus::tusk::{
    consensus_tests::{keys, make_consensus_store, make_optimal_certificates, mock_committee},
    *,
};
use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput,
};
use crypto::{traits::KeyPair, Hash};
use pprof::criterion::{Output, PProfProfiler};
use primary::{Certificate, Round};
use std::collections::BTreeSet;

pub fn process_certificates(c: &mut Criterion) {
    let mut consensus_group = c.benchmark_group("processing certificates");
    consensus_group.sampling_mode(SamplingMode::Flat);

    static BATCH_SIZES: [u64; 4] = [100, 500, 1000, 5000];

    for size in &BATCH_SIZES {
        let gc_depth = 12;
        let rounds: Round = *size;

        // process certificates for rounds, check we don't grow the dag too much
        let keys: Vec<_> = keys().into_iter().map(|kp| kp.public().clone()).collect();
        let genesis = Certificate::genesis(&mock_committee(&keys[..]))
            .iter()
            .map(|x| x.digest())
            .collect::<BTreeSet<_>>();
        let (certificates, _next_parents) = make_optimal_certificates(1, rounds, &genesis, &keys);
        let committee = mock_committee(&keys);

        let store_path = temp_testdir::TempDir::default();
        let store = make_consensus_store(&store_path);

        let mut state =
            consensus::tusk::State::new(Certificate::genesis(&mock_committee(&keys[..])));

        let data_size: usize = certificates
            .iter()
            .map(|cert| bincode::serialize(&cert).unwrap().len())
            .sum();
        consensus_group.throughput(Throughput::Bytes(data_size as u64));

        consensus_group.bench_with_input(
            BenchmarkId::new("batched", certificates.len()),
            &certificates,
            |b, i| {
                b.iter(|| {
                    for cert in i {
                        let _ = Consensus::process_certificate(
                            &committee,
                            &store,
                            gc_depth,
                            &mut state,
                            /* consensus_index */ 0,
                            cert.clone(),
                        );
                    }
                })
            },
        );
    }
}

criterion_group! {
    name = consensus_group;
    config = Criterion::default().with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)));
    targets = process_certificates
}
criterion_main!(consensus_group);
