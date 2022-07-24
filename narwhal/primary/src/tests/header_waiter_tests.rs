// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    common::{create_db_stores, worker_listener},
    header_waiter::{HeaderWaiter, WaiterMessage},
    metrics::PrimaryMetrics,
    PrimaryWorkerMessage,
};
use core::sync::atomic::AtomicU64;
use crypto::{ed25519::Ed25519PublicKey, Hash};
use prometheus::Registry;
use std::{sync::Arc, time::Duration};
use test_utils::{fixture_header_with_payload, resolve_name_and_committee};
use tokio::{
    sync::{mpsc::channel, watch},
    time::timeout,
};
use types::{BatchDigest, ReconfigureNotification, Round};

#[tokio::test]
async fn successfully_synchronize_batches() {
    // GIVEN
    let (name, committee) = resolve_name_and_committee();
    let (_, certificate_store, payload_store) = create_db_stores();
    let consensus_round = Arc::new(AtomicU64::new(0));
    let gc_depth: Round = 1;
    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (tx_synchronizer, rx_synchronizer) = channel(10);
    let (tx_core, mut rx_core) = channel(10);
    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));

    HeaderWaiter::spawn(
        name.clone(),
        committee.clone(),
        certificate_store,
        payload_store.clone(),
        consensus_round,
        gc_depth,
        /* sync_retry_delay */ Duration::from_secs(5),
        /* sync_retry_nodes */ 3,
        rx_reconfigure,
        rx_synchronizer,
        tx_core,
        metrics,
    );

    // AND a header
    let worker_id = 0;
    let header = fixture_header_with_payload(2);
    let missing_digests = vec![BatchDigest::default()];
    let missing_digests_map = missing_digests
        .clone()
        .into_iter()
        .map(|d| (d, worker_id))
        .collect();

    // AND send a message to synchronizer batches
    tx_synchronizer
        .send(WaiterMessage::SyncBatches(
            missing_digests_map,
            header.clone(),
        ))
        .await
        .unwrap();

    // AND spin up a worker node that primary owns
    let worker_address = committee
        .worker(&name, &worker_id)
        .unwrap()
        .primary_to_worker;

    let handle = worker_listener::<PrimaryWorkerMessage<Ed25519PublicKey>>(1, worker_address);

    // THEN
    if let Ok(Ok(mut result)) = timeout(Duration::from_millis(4_000), handle).await {
        match result.remove(0) {
            PrimaryWorkerMessage::Synchronize(missing, _) => {
                assert_eq!(
                    missing_digests, missing,
                    "Expected missing digests don't match"
                );

                // now simulate the write of the batch to the payload store
                payload_store
                    .write_all(missing_digests.into_iter().map(|e| ((e, worker_id), 1)))
                    .await
                    .unwrap();
            }
            _ => panic!("Unexpected message received!"),
        }

        // now get the output as expected
        let header_result = rx_core.recv().await.unwrap();
        assert_eq!(header.digest(), header_result.digest());
    } else {
        panic!("Messages not received by worker");
    }
}
