use consensus::{
    consensus_tests::{keys, make_optimal_certificates, mock_committee},
    *,
};
use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput,
};
use crypto::Hash;
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
        let keys: Vec<_> = keys().into_iter().map(|(x, _)| x).collect();
        let genesis = Certificate::genesis(&mock_committee(&keys[..]))
            .iter()
            .map(|x| x.digest())
            .collect::<BTreeSet<_>>();
        let (certificates, _next_parents) = make_optimal_certificates(1, rounds, &genesis, &keys);
        let committee = mock_committee(&keys);

        let temp_dir = tempfile::tempdir().expect("Failed to open temporary directory");
        let mut state =
            consensus::State::new(Certificate::genesis(&mock_committee(&keys[..])), temp_dir)
                .unwrap();

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
                            gc_depth,
                            &mut state,
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
