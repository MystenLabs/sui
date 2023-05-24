// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput,
};
use fastcrypto::hash::Hash;
use narwhal_types::Certificate;
use std::collections::BTreeSet;
use test_utils::{latest_protocol_version, make_optimal_certificates, CommitteeFixture};

pub fn verify_certificates(c: &mut Criterion) {
    let mut bench_group = c.benchmark_group("verify_certificate");
    bench_group.sampling_mode(SamplingMode::Flat);

    static COMMITTEE_SIZES: [usize; 4] = [4, 10, 40, 100];
    for committee_size in COMMITTEE_SIZES {
        let fixture = CommitteeFixture::builder()
            .committee_size(committee_size.try_into().unwrap())
            .build();
        let committee = fixture.committee();
        let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();

        // process certificates for rounds, check we don't grow the dag too much
        let genesis = Certificate::genesis(&committee)
            .iter()
            .map(|x| x.digest())
            .collect::<BTreeSet<_>>();
        let (certificates, _next_parents) = make_optimal_certificates(
            &committee,
            &latest_protocol_version(),
            1..=1,
            &genesis,
            &ids,
        );
        let certificate = certificates.front().unwrap().clone();

        let data_size: usize = bcs::to_bytes(&certificate).unwrap().len();
        bench_group.throughput(Throughput::Bytes(data_size as u64));

        bench_group.bench_with_input(
            BenchmarkId::new("with_committee_size", committee_size),
            &certificate,
            |b, cert| {
                let worker_cache = fixture.worker_cache();
                b.iter(|| {
                    let _ = cert.verify(&committee, &worker_cache);
                })
            },
        );
    }
}

criterion_group! {
    name = verify_certificate;
    config = Criterion::default().sample_size(1000).noise_threshold(0.1);
    targets = verify_certificates
}
criterion_main!(verify_certificate);
