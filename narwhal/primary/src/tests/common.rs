use std::num::NonZeroUsize;
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{AuthorityIdentifier, WorkerId};
use crypto::NetworkKeyPair;
use std::time::Duration;
use storage::{CertificateStore, CertificateStoreCache, HeaderStore, PayloadStore};
use store::{reopen, rocks, rocks::DBMap, rocks::ReadWriteOptions};
use test_utils::{
    temp_dir, PrimaryToWorkerMockServer, CERTIFICATES_CF, CERTIFICATE_DIGEST_BY_ORIGIN_CF,
    CERTIFICATE_DIGEST_BY_ROUND_CF, HEADERS_CF, PAYLOAD_CF,
};
use types::{
    BatchDigest, Certificate, CertificateDigest, Header, HeaderDigest, Round,
    WorkerSynchronizeMessage,
};

use storage::PayloadToken;
use tokio::{task::JoinHandle, time::Instant};

pub fn create_db_stores() -> (HeaderStore, CertificateStore, PayloadStore) {
    // Create a new test store.
    let rocksdb = rocks::open_cf(
        temp_dir(),
        None,
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

pub fn create_test_vote_store() -> Store<PublicKey, VoteInfo> {
    // Create a new test store.
    let rocksdb = rocks::open_cf(temp_dir(), None, &[VOTES_CF]).expect("Failed creating database");
    let votes_map = reopen!(&rocksdb, VOTES_CF;<PublicKey, VoteInfo>);
    Store::new(votes_map)
}

#[must_use]
pub fn worker_listener(
    // -1 means receive unlimited messages until timeout expires
    num_of_expected_responses: i32,
    address: mysten_network::Multiaddr,
    keypair: NetworkKeyPair,
) -> JoinHandle<Vec<WorkerSynchronizeMessage>> {
    tokio::spawn(async move {
        let (mut recv_sync, _network) = PrimaryToWorkerMockServer::spawn(keypair, address);
        let mut syncs = Vec::new();

        let timer = tokio::time::sleep(Duration::from_secs(1));
        tokio::pin!(timer);

        loop {
            tokio::select! {
                Some(message) = recv_sync.recv() => {
                    timer.as_mut().reset(Instant::now() + Duration::from_secs(1));
                    syncs.push(message);
                    if num_of_expected_responses != -1
                        && syncs.len() as i32 == num_of_expected_responses
                    {
                        return syncs;
                    }
                }
                () = &mut timer => {
                    // timeout happened - just return whatever has already
                    return syncs;
                }
            }
        }
    })
}
