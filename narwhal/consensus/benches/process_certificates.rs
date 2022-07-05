// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use arc_swap::ArcSwap;
use consensus::{
    bullshark::Bullshark,
    consensus::{ConsensusProtocol, ConsensusState},
    metrics::ConsensusMetrics,
};
use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput,
};
use crypto::{traits::KeyPair, Hash};
use pprof::criterion::{Output, PProfProfiler};
use prometheus::Registry;
use std::{collections::BTreeSet, sync::Arc};
use test_utils::{keys, make_consensus_store, make_optimal_certificates, mock_committee, temp_dir};
use types::{Certificate, Round};

pub fn process_certificates(c: &mut Criterion) {
    let mut consensus_group = c.benchmark_group("processing certificates");
    consensus_group.sampling_mode(SamplingMode::Flat);

    static BATCH_SIZES: [u64; 4] = [100, 500, 1000, 5000];

    for size in &BATCH_SIZES {
        let gc_depth = 12;
        let rounds: Round = *size;

        // process certificates for rounds, check we don't grow the dag too much
        let keys: Vec<_> = keys(None)
            .into_iter()
            .map(|kp| kp.public().clone())
            .collect();
        let genesis = Certificate::genesis(&mock_committee(&keys[..]))
            .iter()
            .map(|x| x.digest())
            .collect::<BTreeSet<_>>();
        let (certificates, _next_parents) = make_optimal_certificates(1..=rounds, &genesis, &keys);
        let committee = Arc::new(ArcSwap::from_pointee(mock_committee(&keys)));

        let store_path = temp_dir();
        let store = make_consensus_store(&store_path);
        let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

        let mut state =
            ConsensusState::new(Certificate::genesis(&mock_committee(&keys[..])), metrics);

        let data_size: usize = certificates
            .iter()
            .map(|cert| bincode::serialize(&cert).unwrap().len())
            .sum();
        consensus_group.throughput(Throughput::Bytes(data_size as u64));

        let mut ordering_engine = Bullshark {
            committee,
            store,
            gc_depth,
        };
        consensus_group.bench_with_input(
            BenchmarkId::new("batched", certificates.len()),
            &certificates,
            |b, i| {
                b.iter(|| {
                    for cert in i {
                        let _ = ordering_engine.process_certificate(
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
