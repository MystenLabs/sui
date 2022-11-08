// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crypto::PublicKey;
use std::sync::Arc;
use storage::CertificateStore;
use store::{reopen, rocks, rocks::DBMap};
use types::{
    Certificate, CertificateDigest, CommittedSubDagShell, ConsensusStore, Round, SequenceNumber,
};

pub fn make_consensus_store(store_path: &std::path::Path) -> Arc<ConsensusStore> {
    const LAST_COMMITTED_CF: &str = "last_committed";
    const SEQUENCE_CF: &str = "sequence";
    const SUB_DAG_CF: &str = "sub_dag";

    let rocksdb = rocks::open_cf(
        store_path,
        None,
        &[LAST_COMMITTED_CF, SEQUENCE_CF, SUB_DAG_CF],
    )
    .expect("Failed to create database");

    let (last_committed_map, sequence_map, sub_dag_map) = reopen!(&rocksdb,
        LAST_COMMITTED_CF;<PublicKey, Round>,
        SEQUENCE_CF;<SequenceNumber, CertificateDigest>,
        SUB_DAG_CF;<SequenceNumber, CommittedSubDagShell>
    );

    Arc::new(ConsensusStore::new(
        last_committed_map,
        sequence_map,
        sub_dag_map,
    ))
}

pub fn make_certificate_store(store_path: &std::path::Path) -> CertificateStore {
    const CERTIFICATES_CF: &str = "certificates";
    const CERTIFICATE_DIGEST_BY_ROUND_CF: &str = "certificate_digest_by_round";
    const CERTIFICATE_DIGEST_BY_ORIGIN_CF: &str = "certificate_digest_by_origin";

    let rocksdb = rocks::open_cf(
        store_path,
        None,
        &[
            CERTIFICATES_CF,
            CERTIFICATE_DIGEST_BY_ROUND_CF,
            CERTIFICATE_DIGEST_BY_ORIGIN_CF,
        ],
    )
    .expect("Failed creating database");

    let (certificate_map, certificate_digest_by_round_map, certificate_digest_by_origin_map) = reopen!(&rocksdb,
        CERTIFICATES_CF;<CertificateDigest, Certificate>,
        CERTIFICATE_DIGEST_BY_ROUND_CF;<(Round, PublicKey), CertificateDigest>,
        CERTIFICATE_DIGEST_BY_ORIGIN_CF;<(PublicKey, Round), CertificateDigest>);

    CertificateStore::new(
        certificate_map,
        certificate_digest_by_round_map,
        certificate_digest_by_origin_map,
    )
}
