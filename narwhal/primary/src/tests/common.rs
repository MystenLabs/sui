// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use bincode::deserialize;
use config::WorkerId;
use crypto::ed25519::Ed25519PublicKey;
use serde::de::DeserializeOwned;
use std::time::Duration;
use store::{reopen, rocks, rocks::DBMap, Store};
use test_utils::{temp_dir, PrimaryToWorkerMockServer, CERTIFICATES_CF, HEADERS_CF, PAYLOAD_CF};
use types::{BatchDigest, Certificate, CertificateDigest, Header, HeaderDigest};

use crate::PayloadToken;
use tokio::{task::JoinHandle, time::timeout};

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

pub fn worker_listener<T>(
    num_of_expected_responses: i32,
    address: multiaddr::Multiaddr,
) -> JoinHandle<Vec<T>>
where
    T: Send + DeserializeOwned + 'static,
{
    tokio::spawn(async move {
        let mut recv = PrimaryToWorkerMockServer::spawn(address);
        let mut responses = Vec::new();

        loop {
            match timeout(Duration::from_secs(1), recv.recv()).await {
                Err(_) => {
                    // timeout happened - just return whatever has already
                    return responses;
                }
                Ok(Some(message)) => {
                    match deserialize::<'_, T>(&message.payload) {
                        Ok(msg) => {
                            responses.push(msg);

                            // if -1 is given, then we don't count the number of messages
                            // but we just rely to receive as many as possible until timeout
                            // happens when waiting for requests.
                            if num_of_expected_responses != -1
                                && responses.len() as i32 == num_of_expected_responses
                            {
                                return responses;
                            }
                        }
                        Err(err) => {
                            panic!("Error occurred {err}");
                        }
                    }
                }
                //  sender closed
                _ => panic!("Failed to receive network message"),
            }
        }
    })
}
