// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use consensus::{
    bullshark::Bullshark,
    consensus::{ConsensusProtocol, ConsensusState},
    metrics::ConsensusMetrics,
};
use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput,
};
use fastcrypto::hash::Hash;
use narwhal_consensus as consensus;
use pprof::criterion::{Output, PProfProfiler};
use prometheus::Registry;
use std::{collections::BTreeSet, sync::Arc};
use storage::NodeStorage;
use test_utils::{make_consensus_store, make_optimal_certificates, temp_dir, CommitteeFixture};
use tokio::time::Instant;
use types::{Certificate, Round};

pub fn process_certificates(c: &mut Criterion) {
    let mut consensus_group = c.benchmark_group("processing certificates");
    consensus_group.sampling_mode(SamplingMode::Flat);

    static BATCH_SIZES: [u64; 4] = [100, 500, 1000, 5000];

    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let keys: Vec<_> = fixture.authorities().map(|a| a.id()).collect();

    for size in &BATCH_SIZES {
        let gc_depth = 12;
        let rounds: Round = *size;

        // process certificates for rounds, check we don't grow the dag too much
        let genesis = Certificate::genesis(&committee)
            .iter()
            .map(|x| x.digest())
            .collect::<BTreeSet<_>>();
        let (certificates, _next_parents) =
            make_optimal_certificates(&committee, 1..=rounds, &genesis, &keys);

        let store_path = temp_dir();
        let store = NodeStorage::reopen(&store_path, None);
        let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

        let mut state = ConsensusState::new(metrics.clone(), gc_depth);

        let data_size: usize = certificates
            .iter()
            .map(|cert| bcs::to_bytes(&cert).unwrap().len())
            .sum();
        consensus_group.throughput(Throughput::Bytes(data_size as u64));

        let mut ordering_engine = Bullshark {
            committee: committee.clone(),
            store: store.consensus_store,
            metrics,
            last_successful_leader_election_timestamp: Instant::now(),
            last_leader_election: Default::default(),
            max_inserted_certificate_round: 0,
            num_sub_dags_per_schedule: 100,
        };
        consensus_group.bench_with_input(
            BenchmarkId::new("batched", certificates.len()),
            &certificates,
            |b, i| {
                b.iter(|| {
                    for cert in i {
                        let _ = ordering_engine.process_certificate(&mut state, cert.clone());
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
