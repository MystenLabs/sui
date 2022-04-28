// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::WorkerId;
use crypto::ed25519::Ed25519PublicKey;
use store::{reopen, rocks, rocks::DBMap, Store};
use test_utils::{temp_dir, CERTIFICATES_CF, HEADERS_CF, PAYLOAD_CF};
use types::{BatchDigest, Certificate, CertificateDigest, Header, HeaderDigest};

use crate::PayloadToken;

pub fn create_db_stores() -> (
    Store<HeaderDigest, Header<Ed25519PublicKey>>,
    Store<CertificateDigest, Certificate<Ed25519PublicKey>>,
    Store<(BatchDigest, WorkerId), PayloadToken>,
) {
    // Create a new test store.
    let rocksdb = rocks::open_cf(temp_dir(), None, &[HEADERS_CF, CERTIFICATES_CF, PAYLOAD_CF])
        .expect("Failed creating database");

    let (header_map, certificate_map, payload_map) = reopen!(&rocksdb,
        HEADERS_CF;<HeaderDigest, Header<Ed25519PublicKey>>,
        CERTIFICATES_CF;<CertificateDigest, Certificate<Ed25519PublicKey>>,
        PAYLOAD_CF;<(BatchDigest, WorkerId), PayloadToken>);

    (
        Store::new(header_map),
        Store::new(certificate_map),
        Store::new(payload_map),
    )
}
