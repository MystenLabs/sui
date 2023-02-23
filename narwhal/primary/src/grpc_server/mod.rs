// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use self::{configuration::NarwhalConfiguration, validator::NarwhalValidator};
use crate::{
    block_synchronizer::handler::Handler,
    grpc_server::{metrics::EndpointMetrics, proposer::NarwhalProposer},
    BlockRemover, BlockWaiter,
};
use config::{AuthorityIdentifier, Committee};
use consensus::dag::Dag;

use crypto::PublicKey;
use multiaddr::Multiaddr;
use std::{sync::Arc, time::Duration};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{error, info, warn};
use types::{ConditionalBroadcastReceiver, ConfigurationServer, ProposerServer, ValidatorServer};

mod configuration;
pub mod metrics;
mod proposer;
mod validator;

pub struct ConsensusAPIGrpc<SynchronizerHandler: Handler + Send + Sync + 'static> {
    authority_id: AuthorityIdentifier,
    // Multiaddr of gRPC server
    socket_address: Multiaddr,
    block_waiter: BlockWaiter<SynchronizerHandler>,
    block_remover: BlockRemover,
    get_collections_timeout: Duration,
    remove_collections_timeout: Duration,
    block_synchronizer_handler: Arc<SynchronizerHandler>,
    dag: Option<Arc<Dag>>,
    committee: Committee,
    endpoints_metrics: EndpointMetrics,
    rx_shutdown: ConditionalBroadcastReceiver,
}

impl<SynchronizerHandler: Handler + Send + Sync + 'static> ConsensusAPIGrpc<SynchronizerHandler> {
    #[must_use]
    pub fn spawn(
        authority_id: AuthorityIdentifier,
        socket_address: Multiaddr,
        block_waiter: BlockWaiter<SynchronizerHandler>,
        block_remover: BlockRemover,
        get_collections_timeout: Duration,
        remove_collections_timeout: Duration,
        block_synchronizer_handler: Arc<SynchronizerHandler>,
        dag: Option<Arc<Dag>>,
        committee: Committee,
        endpoints_metrics: EndpointMetrics,
        rx_shutdown: ConditionalBroadcastReceiver,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let _ = Self {
                name,
                socket_address,
                block_waiter,
                block_remover,
                get_collections_timeout,
                remove_collections_timeout,
                block_synchronizer_handler,
                dag,
                committee,
                endpoints_metrics,
                rx_shutdown,
            }
            .run()
            .await
            .map_err(|e| error!("{:?}", e));
        })
    }

    async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        const GRACEFUL_SHUTDOWN_DURATION: Duration = Duration::from_millis(2_000);

        let narwhal_validator = NarwhalValidator::new(
            self.block_waiter,
            self.block_remover,
            self.get_collections_timeout,
            self.remove_collections_timeout,
            self.block_synchronizer_handler,
            self.dag.clone(),
        );

        let narwhal_proposer = NarwhalProposer::new(self.dag, self.committee.clone());
        let narwhal_configuration = NarwhalConfiguration::new(
            self.committee
                .primary_by_id(&self.authority_id)
                .expect("Our public key is not in the committee"),
            self.committee.clone(),
        );

        let config = mysten_network::config::Config::default();
        let mut server = config
            .server_builder_with_metrics(self.endpoints_metrics.clone())
            .add_service(ValidatorServer::new(narwhal_validator))
            .add_service(ConfigurationServer::new(narwhal_configuration))
            .add_service(ProposerServer::new(narwhal_proposer))
            .bind(&self.socket_address)
            .await?;
        let local_addr = server.local_addr();
        info!("Consensus API gRPC Server listening on {local_addr}");

        let shutdown_handle = server.take_cancel_handle().unwrap();

        let server_handle = tokio::spawn(server.serve());

        // wait to receive a shutdown signal
        let _ = self.rx_shutdown.receiver.recv().await;

        // once do just gracefully shutdown the node
        shutdown_handle.send(()).unwrap();

        // now wait until the handle completes or timeout if it takes long time
        match timeout(GRACEFUL_SHUTDOWN_DURATION, server_handle).await {
            Ok(_) => {
                info!("Successfully shutting down gracefully grpc server");
            }
            Err(err) => {
                warn!(
                    "Time out while waiting to gracefully shutdown grpc server: {}",
                    err
                )
            }
        }

        Ok(())
    }
}
