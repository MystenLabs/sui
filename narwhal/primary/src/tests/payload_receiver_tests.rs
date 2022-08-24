// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::common::create_db_stores;
use crate::payload_receiver::PayloadReceiver;
use types::BatchDigest;

#[tokio::test]
async fn receive_batch() {
    // GIVEN
    let worker_id = 5;
    let digest = BatchDigest::new([5u8; 32]);
    let (tx_workers, rx_workers) = test_utils::test_channel!(1);
    let (_, _, payload_store) = create_db_stores();

    let _handle = PayloadReceiver::spawn(payload_store.clone(), rx_workers);

    for _ in 0..4 {
        // WHEN - irrespective of how many times will send the same (digest, worker_id)
        tx_workers.send((digest, worker_id)).await.unwrap();

        // THEN we expected to be stored successfully (no errors thrown) and have an
        // idempotent behaviour.
        let result = payload_store
            .notify_read((digest, worker_id))
            .await
            .unwrap();

        assert_eq!(result.unwrap(), 0u8);
    }
}
