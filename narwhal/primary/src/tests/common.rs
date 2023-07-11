use std::num::NonZeroUsize;
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{AuthorityIdentifier, WorkerId};
use storage::PayloadToken;
use storage::{CertificateStore, CertificateStoreCache, HeaderStore, PayloadStore};
use store::rocks::MetricConf;
use store::{reopen, rocks, rocks::DBMap, rocks::ReadWriteOptions};
use test_utils::{
    temp_dir, CERTIFICATES_CF, CERTIFICATE_DIGEST_BY_ORIGIN_CF, CERTIFICATE_DIGEST_BY_ROUND_CF,
    HEADERS_CF, PAYLOAD_CF,
};
use types::{BatchDigest, Certificate, CertificateDigest, Header, HeaderDigest, Round};

pub fn create_db_stores() -> (HeaderStore, CertificateStore, PayloadStore) {
    // Create a new test store.
    let rocksdb = rocks::open_cf(
        temp_dir(),
        None,
        MetricConf::default(),
        &[
            HEADERS_CF,
            CERTIFICATES_CF,
            CERTIFICATE_DIGEST_BY_ROUND_CF,
            CERTIFICATE_DIGEST_BY_ORIGIN_CF,
            PAYLOAD_CF,
        ],
    )
    .expect("Failed creating database");

    let (
        header_map,
        certificate_map,
        certificate_digest_by_round_map,
        certificate_digest_by_origin_map,
        payload_map,
    ) = reopen!(&rocksdb,
        HEADERS_CF;<HeaderDigest, Header>,
        CERTIFICATES_CF;<CertificateDigest, Certificate>,
        CERTIFICATE_DIGEST_BY_ROUND_CF;<(Round, AuthorityIdentifier), CertificateDigest>,
        CERTIFICATE_DIGEST_BY_ORIGIN_CF;<(AuthorityIdentifier, Round), CertificateDigest>,
        PAYLOAD_CF;<(BatchDigest, WorkerId), PayloadToken>);

    (
        HeaderStore::new(header_map),
        CertificateStore::new(
            certificate_map,
            certificate_digest_by_round_map,
            certificate_digest_by_origin_map,
            CertificateStoreCache::new(NonZeroUsize::new(100).unwrap(), None),
        ),
        PayloadStore::new(payload_map),
    )
}
