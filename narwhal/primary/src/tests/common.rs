// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::PayloadToken;
use config::WorkerId;
use crypto::NetworkKeyPair;
use std::time::Duration;
use storage::CertificateStore;
use store::{reopen, rocks, rocks::DBMap, Store};
use test_utils::{
    temp_dir, PrimaryToWorkerMockServer, CERTIFICATES_CF, CERTIFICATE_ID_BY_ROUND_CF, HEADERS_CF,
    PAYLOAD_CF,
};
use tokio::{task::JoinHandle, time::timeout};
use types::{
    BatchDigest, Certificate, CertificateDigest, Header, HeaderDigest, PrimaryWorkerMessage, Round,
};

pub fn create_db_stores() -> (
    Store<HeaderDigest, Header>,
    CertificateStore,
    Store<(BatchDigest, WorkerId), PayloadToken>,
) {
    // Create a new test store.
    let rocksdb = rocks::open_cf(
        temp_dir(),
        None,
        &[
            HEADERS_CF,
            CERTIFICATES_CF,
            CERTIFICATE_ID_BY_ROUND_CF,
            PAYLOAD_CF,
        ],
    )
    .expect("Failed creating database");

    let (header_map, certificate_map, certificate_id_by_round_map, payload_map) = reopen!(&rocksdb,
        HEADERS_CF;<HeaderDigest, Header>,
        CERTIFICATES_CF;<CertificateDigest, Certificate>,
        CERTIFICATE_ID_BY_ROUND_CF;<(Round, CertificateDigest), u8>,
        PAYLOAD_CF;<(BatchDigest, WorkerId), PayloadToken>);

    (
        Store::new(header_map),
        CertificateStore::new(certificate_map, certificate_id_by_round_map),
        Store::new(payload_map),
    )
}

#[must_use]
pub fn worker_listener(
    num_of_expected_responses: i32,
    address: multiaddr::Multiaddr,
    keypair: NetworkKeyPair,
) -> JoinHandle<Vec<PrimaryWorkerMessage>> {
    tokio::spawn(async move {
        let (mut recv, _network) = PrimaryToWorkerMockServer::spawn(keypair, address);
        let mut responses = Vec::new();

        loop {
            match timeout(Duration::from_secs(1), recv.recv()).await {
                Err(_) => {
                    // timeout happened - just return whatever has already
                    return responses;
                }
                Ok(Some(message)) => {
                    responses.push(message);

                    // if -1 is given, then we don't count the number of messages
                    // but we just rely to receive as many as possible until timeout
                    // happens when waiting for requests.
                    if num_of_expected_responses != -1
                        && responses.len() as i32 == num_of_expected_responses
                    {
                        return responses;
                    }
                }
                //  sender closed
                _ => panic!("Failed to receive network message"),
            }
        }
    })
}
