// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anemo::PeerId;
use config::AuthorityIdentifier;
use network::{PrimaryToPrimaryRpc, WorkerRpc};
use test_utils::cluster::Cluster;
use types::{FetchCertificatesRequest, RequestBatchesRequest};

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_server_authorizations() {
    // Set up primaries and workers with a committee.
    let mut test_cluster = Cluster::new(None);
    test_cluster.start(Some(4), Some(1), None).await;
    tokio::time::sleep(Duration::from_secs(3)).await;

    let test_authority = test_cluster.authority(0);
    let test_client = test_authority.client().await;
    let test_committee = test_cluster.committee.clone();
    let test_worker_cache = test_cluster.worker_cache.clone();

    // Reachable to primaries in the same committee.
    {
        let target_authority = test_committee.authority(&AuthorityIdentifier(1)).unwrap();

        let primary_network = test_client.get_primary_network().await.unwrap();
        let primary_target_name = target_authority.network_key();
        let request = anemo::Request::new(FetchCertificatesRequest::default())
            .with_timeout(Duration::from_secs(5));
        primary_network
            .fetch_certificates(&primary_target_name, request)
            .await
            .unwrap();

        let worker_network = test_client.get_worker_network(0).await.unwrap();
        let worker_target_name = test_worker_cache
            .workers
            .get(target_authority.protocol_key())
            .unwrap()
            .0
            .get(&0)
            .unwrap()
            .name
            .clone();
        let request = anemo::Request::new(RequestBatchesRequest::default())
            .with_timeout(Duration::from_secs(5));
        worker_network
            .request_batches(&worker_target_name, request)
            .await
            .unwrap();
    }

    // Set up primaries and workers with a another committee.
    let mut unreachable_cluster = Cluster::new(None);
    unreachable_cluster.start(Some(4), Some(1), None).await;
    tokio::time::sleep(Duration::from_secs(3)).await;

    // test_client should not reach unreachable_authority.
    {
        let unreachable_committee = unreachable_cluster.committee.clone();
        let unreachable_worker_cache = unreachable_cluster.worker_cache.clone();

        let unreachable_authority = unreachable_committee
            .authority(&AuthorityIdentifier(0))
            .unwrap();
        let primary_target_name = unreachable_authority.network_key();
        let primary_peer_id: PeerId = PeerId(primary_target_name.0.to_bytes());
        let primary_address = unreachable_authority.primary_address();
        let primary_network = test_client.get_primary_network().await.unwrap();
        primary_network
            .connect_with_peer_id(primary_address.to_anemo_address().unwrap(), primary_peer_id)
            .await
            .unwrap();
        let request = anemo::Request::new(FetchCertificatesRequest::default())
            .with_timeout(Duration::from_secs(5));
        // Removing the AllowedPeers RequireAuthorizationLayer for primary should make this succeed.
        assert!(primary_network
            .fetch_certificates(&primary_target_name, request)
            .await
            .is_err());

        let worker_network = test_client.get_worker_network(0).await.unwrap();
        let worker_target_name = unreachable_worker_cache
            .workers
            .get(unreachable_authority.protocol_key())
            .unwrap()
            .0
            .get(&0)
            .unwrap()
            .name
            .clone();
        let request = anemo::Request::new(RequestBatchesRequest::default())
            .with_timeout(Duration::from_secs(5));
        // Removing the AllowedPeers RequireAuthorizationLayer for workers should make this succeed.
        assert!(worker_network
            .request_batches(&worker_target_name, request)
            .await
            .is_err());
    }
}
