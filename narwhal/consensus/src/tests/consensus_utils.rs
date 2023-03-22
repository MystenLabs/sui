// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crypto::PublicKeyBytes;
use std::sync::Arc;
use storage::CertificateStore;
use store::rocks::MetricConf;
use store::{reopen, rocks, rocks::DBMap, rocks::ReadWriteOptions};
use types::{
    Certificate, CertificateDigest, CommittedSubDagShell, ConsensusStore, Round, SequenceNumber,
};

pub fn make_consensus_store(store_path: &std::path::Path) -> Arc<ConsensusStore> {
    const LAST_COMMITTED_CF: &str = "last_committed";
    const SEQUENCE_CF: &str = "sequence";

    let rocksdb = rocks::open_cf(
        store_path,
        None,
        MetricConf::default(),
        &[LAST_COMMITTED_CF, SEQUENCE_CF],
    )
    .expect("Failed to create database");

    let (last_committed_map, sequence_map) = reopen!(&rocksdb,
        LAST_COMMITTED_CF;<PublicKeyBytes, Round>,
        SEQUENCE_CF;<SequenceNumber, CommittedSubDagShell>
    );

    Arc::new(ConsensusStore::new(last_committed_map, sequence_map))
}

pub fn make_certificate_store(store_path: &std::path::Path) -> CertificateStore {
    const CERTIFICATES_CF: &str = "certificates";
    const CERTIFICATE_DIGEST_BY_ROUND_CF: &str = "certificate_digest_by_round";
    const CERTIFICATE_DIGEST_BY_ORIGIN_CF: &str = "certificate_digest_by_origin";

    let rocksdb = rocks::open_cf(
        store_path,
        None,
        MetricConf::default(),
        &[
            CERTIFICATES_CF,
            CERTIFICATE_DIGEST_BY_ROUND_CF,
            CERTIFICATE_DIGEST_BY_ORIGIN_CF,
        ],
    )
    .expect("Failed creating database");

    let (certificate_map, certificate_digest_by_round_map, certificate_digest_by_origin_map) = reopen!(&rocksdb,
        CERTIFICATES_CF;<CertificateDigest, Certificate>,
        CERTIFICATE_DIGEST_BY_ROUND_CF;<(Round, PublicKeyBytes), CertificateDigest>,
        CERTIFICATE_DIGEST_BY_ORIGIN_CF;<(PublicKeyBytes, Round), CertificateDigest>);

    CertificateStore::new(
        certificate_map,
        certificate_digest_by_round_map,
        certificate_digest_by_origin_map,
    )
}
