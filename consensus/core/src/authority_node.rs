// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Instant;
use std::vec;

use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::{AuthorityIndex, Committee, NetworkKeyPair, Parameters, ProtocolKeyPair};
use parking_lot::RwLock;
use prometheus::Registry;
use sui_protocol_config::ProtocolConfig;
use tracing::info;

use crate::block::{BlockAPI, BlockRef, SignedBlock, VerifiedBlock};
use crate::block_manager::BlockManager;
use crate::block_verifier::{BlockVerifier, SignedBlockVerifier};
use crate::broadcaster::Broadcaster;
use crate::context::Context;
use crate::core::{Core, CoreSignals};
use crate::core_thread::{ChannelCoreThreadDispatcher, CoreThreadDispatcher, CoreThreadHandle};
use crate::dag_state::DagState;
use crate::error::{ConsensusError, ConsensusResult};
use crate::leader_timeout::{LeaderTimeoutTask, LeaderTimeoutTaskHandle};
use crate::metrics::initialise_metrics;
use crate::network::{NetworkManager, NetworkService};
use crate::storage::rocksdb_store::RocksDBStore;
use crate::transaction::{TransactionClient, TransactionConsumer, TransactionVerifier};

pub struct AuthorityNode<N>
where
    N: NetworkManager<AuthorityService>,
{
    context: Arc<Context>,
    start_time: Instant,
    transaction_client: Arc<TransactionClient>,
    leader_timeout_handle: LeaderTimeoutTaskHandle,
    core_thread_handle: CoreThreadHandle,
    broadcaster: Broadcaster,
    network_manager: N,
}

impl<N> AuthorityNode<N>
where
    N: NetworkManager<AuthorityService>,
{
    #[allow(unused)]
    async fn start(
        own_index: AuthorityIndex,
        committee: Committee,
        parameters: Parameters,
        protocol_config: ProtocolConfig,
        // To avoid accidentally leaking the private key, the protocol key pair should only be
        // kept in Core.
        protocol_keypair: ProtocolKeyPair,
        network_keypair: NetworkKeyPair,
        transaction_verifier: Arc<dyn TransactionVerifier>,
        registry: Registry,
    ) -> Self {
        info!("Starting authority with index {}", own_index);
        let context = Arc::new(Context::new(
            own_index,
            committee,
            parameters,
            protocol_config,
            initialise_metrics(registry),
        ));
        let start_time = Instant::now();

        // Create the transactions client and the transactions consumer
        let (tx_client, tx_receiver) = TransactionClient::new(context.clone());
        let tx_consumer = TransactionConsumer::new(tx_receiver, context.clone(), None);

        // Construct Core components.
        let (core_signals, signals_receivers) = CoreSignals::new();
        let store = Arc::new(RocksDBStore::new(&context.parameters.db_path_str_unsafe()));
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let block_manager = BlockManager::new(context.clone(), dag_state);
        let core = Core::new(
            context.clone(),
            tx_consumer,
            block_manager,
            core_signals,
            protocol_keypair,
            store,
        );

        let (core_dispatcher, core_thread_handle) =
            ChannelCoreThreadDispatcher::start(core, context.clone());
        let leader_timeout_handle =
            LeaderTimeoutTask::start(core_dispatcher.clone(), &signals_receivers, context.clone());

        // Create network manager and client.
        let network_manager = N::new(context.clone());
        let network_client = network_manager.client();

        // Create Broadcaster.
        let broadcaster = Broadcaster::new(context.clone(), network_client, &signals_receivers);

        // Start network service.
        let block_verifier = Arc::new(SignedBlockVerifier::new(
            context.clone(),
            transaction_verifier,
        ));
        let network_service = Arc::new(AuthorityService {
            context: context.clone(),
            block_verifier,
            core_dispatcher,
        });
        network_manager.install_service(network_keypair, network_service);

        Self {
            context,
            start_time,
            transaction_client: Arc::new(tx_client),
            leader_timeout_handle,
            core_thread_handle,
            broadcaster,
            network_manager,
        }
    }

    #[allow(unused)]
    async fn stop(mut self) {
        info!(
            "Stopping authority. Total run time: {:?}",
            self.start_time.elapsed()
        );

        self.network_manager.stop().await;
        self.broadcaster.stop();
        self.core_thread_handle.stop();
        self.leader_timeout_handle.stop().await;

        self.context
            .metrics
            .node_metrics
            .uptime
            .observe(self.start_time.elapsed().as_secs_f64());
    }

    #[allow(unused)]
    pub fn transaction_client(&self) -> Arc<TransactionClient> {
        self.transaction_client.clone()
    }
}

/// Authority's network interface.
pub struct AuthorityService {
    context: Arc<Context>,
    block_verifier: Arc<dyn BlockVerifier>,
    core_dispatcher: ChannelCoreThreadDispatcher,
}

#[async_trait]
impl NetworkService for AuthorityService {
    async fn handle_send_block(
        &self,
        peer: AuthorityIndex,
        serialized_block: Bytes,
    ) -> ConsensusResult<()> {
        // TODO: dedup block verifications, here and with fetched blocks.
        let signed_block: SignedBlock =
            bcs::from_bytes(&serialized_block).map_err(ConsensusError::MalformedBlock)?;
        if peer != signed_block.author() {
            self.context
                .metrics
                .node_metrics
                .invalid_blocks
                .with_label_values(&[&peer.to_string()])
                .inc();
            let e = ConsensusError::UnexpectedAuthority(signed_block.author(), peer);
            info!("Block with wrong authority from {}: {}", peer, e);
            return Err(e);
        }
        if let Err(e) = self.block_verifier.verify(&signed_block) {
            self.context
                .metrics
                .node_metrics
                .invalid_blocks
                .with_label_values(&[&peer.to_string()])
                .inc();
            info!("Invalid block from {}: {}", peer, e);
            return Err(e);
        }
        let verified_block = VerifiedBlock::new_verified(signed_block, serialized_block);
        self.core_dispatcher
            .add_blocks(vec![verified_block])
            .await
            .map_err(|_| ConsensusError::Shutdown)?;
        Ok(())
    }

    async fn handle_fetch_blocks(
        &self,
        _peer: AuthorityIndex,
        _block_refs: Vec<BlockRef>,
    ) -> ConsensusResult<Vec<Bytes>> {
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use consensus_config::{local_committee_and_keys, NetworkKeyPair, Parameters, ProtocolKeyPair};
    use fastcrypto::traits::ToFromBytes;
    use prometheus::Registry;
    use sui_protocol_config::ProtocolConfig;
    use tempfile::TempDir;

    use crate::authority_node::AuthorityNode;
    use crate::network::anemo_network::AnemoManager;
    use crate::transaction::NoopTransactionVerifier;

    #[tokio::test]
    async fn start_and_stop() {
        let (committee, keypairs) = local_committee_and_keys(0, vec![1]);
        let registry = Registry::new();

        let temp_dir = TempDir::new().unwrap();
        let parameters = Parameters {
            db_path: Some(temp_dir.into_path()),
            ..Default::default()
        };
        let txn_verifier = NoopTransactionVerifier {};

        let (own_index, _) = committee.authorities().last().unwrap();
        let protocol_keypair = ProtocolKeyPair::from_bytes(keypairs[0].1.as_bytes()).unwrap();
        let network_keypair = NetworkKeyPair::from_bytes(keypairs[0].0.as_bytes()).unwrap();

        let authority = AuthorityNode::<AnemoManager>::start(
            own_index,
            committee,
            parameters,
            ProtocolConfig::get_for_max_version_UNSAFE(),
            protocol_keypair,
            network_keypair,
            Arc::new(txn_verifier),
            registry,
        )
        .await;

        assert_eq!(authority.context.own_index, own_index);
        assert_eq!(authority.context.committee.epoch(), 0);
        assert_eq!(authority.context.committee.size(), 1);

        authority.stop().await;
    }
}
