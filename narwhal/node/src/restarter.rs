// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{Node, NodeStorage};
use arc_swap::ArcSwap;
use config::{Committee, Parameters, SharedWorkerCache, WorkerCache, WorkerId};
use crypto::{KeyPair, NetworkKeyPair};
use executor::ExecutionState;
use fastcrypto::traits::KeyPair as _;
use futures::future::join_all;
use network::{P2pNetwork, ReliableNetwork};
use prometheus::Registry;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::mpsc::Receiver;
use types::{PrimaryWorkerMessage, ReconfigureNotification, WorkerPrimaryMessage};

// Module to start a node (primary, workers and default consensus), keep it running, and restarting it
/// every time the committee changes.
pub struct NodeRestarter;

impl NodeRestarter {
    pub async fn watch<State>(
        primary_keypair: KeyPair,
        primary_network_keypair: NetworkKeyPair,
        worker_ids_and_keypairs: Vec<(WorkerId, NetworkKeyPair)>,
        committee: &Committee,
        worker_cache: SharedWorkerCache,
        storage_base_path: PathBuf,
        execution_state: Arc<State>,
        parameters: Parameters,
        mut rx_reconfigure: Receiver<(
            KeyPair,
            NetworkKeyPair,
            Committee,
            Vec<(WorkerId, NetworkKeyPair)>,
            WorkerCache,
        )>,
        registry: &Registry,
    ) where
        State: ExecutionState + Send + Sync + 'static,
    {
        let mut primary_keypair = primary_keypair;
        let mut primary_network_keypair = primary_network_keypair;
        let mut name = primary_keypair.public().clone();
        let mut worker_ids_and_keypairs = worker_ids_and_keypairs;
        let mut committee = committee.clone();

        // construct a p2p network that we can use to send reconfigure messages to our primary and
        // workers. We generate a random key simply to construct the network. Also, ideally this
        // would be done via a different interface.
        let mut handles = Vec::new();
        let network = anemo::Network::bind("127.0.0.1:0")
            .server_name("narwhal")
            .private_key(
                NetworkKeyPair::generate(&mut rand::rngs::OsRng)
                    .private()
                    .0
                    .to_bytes(),
            )
            .start(anemo::Router::new())
            .unwrap();

        // Listen for new committees.
        loop {
            tracing::info!("Starting epoch E{}", committee.epoch());

            // Get a fresh store for the new epoch.
            let mut store_path = storage_base_path.clone();
            store_path.push(format!("epoch{}", committee.epoch()));
            let store = NodeStorage::reopen(store_path);

            // Restart the relevant components.
            let primary_handles = Node::spawn_primary(
                primary_keypair,
                primary_network_keypair,
                Arc::new(ArcSwap::new(Arc::new(committee.clone()))),
                worker_cache.clone(),
                &store,
                parameters.clone(),
                /* consensus */ true,
                execution_state.clone(),
                registry,
            )
            .await
            .unwrap();

            let worker_handles = Node::spawn_workers(
                name.clone(),
                worker_ids_and_keypairs,
                Arc::new(ArcSwap::new(Arc::new(committee.clone()))),
                worker_cache.clone(),
                &store,
                parameters.clone(),
                registry,
            );

            handles.extend(primary_handles);
            handles.extend(worker_handles);

            // Wait for a committee change.
            let (
                new_keypair,
                new_network_keypair,
                new_committee,
                new_worker_ids_and_keypairs,
                new_worker_cache,
            ) = match rx_reconfigure.recv().await {
                Some(x) => x,
                None => break,
            };
            tracing::info!("Starting reconfiguration with committee {committee}");

            // Shutdown all relevant components.
            // TODO: shutdown message should probably be sent in a better way than by injecting
            // it through the networking stack.
            let address = network::multiaddr_to_address(
                &committee
                    .primary(&name)
                    .expect("Our key is not in the committee"),
            )
            .unwrap();
            let network_key = committee
                .network_key(&name)
                .expect("Our key is not in the committee");
            let mut primary_network =
                P2pNetwork::new_for_single_address(network_key.to_owned(), address).await;
            let message = WorkerPrimaryMessage::Reconfigure(ReconfigureNotification::Shutdown);
            let primary_cancel_handle =
                primary_network.send(network_key.to_owned(), &message).await;

            let message = PrimaryWorkerMessage::Reconfigure(ReconfigureNotification::Shutdown);
            let mut worker_names = Vec::new();
            for worker in worker_cache
                .load()
                .our_workers(&name)
                .expect("Our key is not in the worker cache")
            {
                let address = network::multiaddr_to_address(&worker.worker_address).unwrap();
                let peer_id = anemo::PeerId(worker.name.0.to_bytes());
                network
                    .connect_with_peer_id(address, peer_id)
                    .await
                    .unwrap();
                worker_names.push(worker.name);
            }
            let worker_cancel_handles = P2pNetwork::new(network.clone())
                .broadcast(worker_names, &message)
                .await;

            // Ensure the message has been received.
            primary_cancel_handle
                .await
                .expect("Failed to notify primary");
            join_all(worker_cancel_handles).await;
            tracing::debug!("Committee reconfiguration message successfully sent");

            // Wait for the components to shut down.
            join_all(handles.drain(..)).await;
            tracing::debug!("All tasks successfully exited");

            // Give it an extra second in case the last task to exit is a network server. The OS
            // may need a moment to make the TCP ports available again.
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            tracing::debug!("Epoch E{} terminated", committee.epoch());

            // Update the settings for the next epoch.
            primary_keypair = new_keypair;
            primary_network_keypair = new_network_keypair;
            name = primary_keypair.public().clone();
            worker_ids_and_keypairs = new_worker_ids_and_keypairs;
            committee = new_committee;
            worker_cache.swap(Arc::new(new_worker_cache));
        }
    }
}
