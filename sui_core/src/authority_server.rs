// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority::AuthorityState,
    consensus_adapter::{ConsensusAdapter, ConsensusListenerMessage},
};
use async_trait::async_trait;
use futures::{stream::BoxStream, FutureExt, TryStreamExt};
use prometheus_exporter::prometheus::{
    register_int_counter, register_int_counter_vec, IntCounter, IntCounterVec,
};
use std::{io, net::SocketAddr, sync::Arc, time::Duration};
use sui_network::{
    api::{BincodeEncodedPayload, Validator, ValidatorServer},
    network::NetworkServer,
    tonic,
};
use sui_types::{crypto::VerificationObligation, error::*, messages::*, serialize::*};
use tokio::{net::TcpListener, sync::mpsc::Sender};
use tracing::{info, Instrument};

#[cfg(test)]
#[path = "unit_tests/server_tests.rs"]
mod server_tests;

const MIN_BATCH_SIZE: u64 = 1000;
const MAX_DELAY_MILLIS: u64 = 5_000; // 5 sec
const PROM_PORT_ADDR: &str = "127.0.0.1:9184";

/// Prometheus metrics which can be displayed in Grafana, queried and alerted on
pub struct AuthorityMetrics {
    total_requests: IntCounter,
    requests_by_route: IntCounterVec,
    error_requests: IntCounter,
}

impl AuthorityMetrics {
    pub fn new() -> AuthorityMetrics {
        Self {
            total_requests: register_int_counter!(
                "total_server_requests",
                "Total number of authority server requests"
            )
            .unwrap(),
            requests_by_route: register_int_counter_vec!(
                "server_requests_by_route",
                "Number of authority requests by route",
                &["request_enum"]
            )
            .unwrap(),
            error_requests: register_int_counter!(
                "error_server_requests",
                "Number of requests which resulted in errors"
            )
            .unwrap(),
        }
    }
}

pub struct AuthorityServerHandle {
    tx_cancellation: tokio::sync::oneshot::Sender<()>,
    local_addr: SocketAddr,
    handle: tokio::task::JoinHandle<Result<(), tonic::transport::Error>>,
}

impl AuthorityServerHandle {
    pub async fn join(self) -> Result<(), std::io::Error> {
        // Note that dropping `self.complete` would terminate the server.
        self.handle
            .await?
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        Ok(())
    }

    pub async fn kill(self) -> Result<(), std::io::Error> {
        self.tx_cancellation.send(()).unwrap();
        self.handle
            .await?
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        Ok(())
    }

    pub fn get_port(&self) -> u16 {
        self.local_addr.port()
    }
}

pub struct AuthorityServer {
    server: NetworkServer,
    pub state: Arc<AuthorityState>,
    consensus_adapter: ConsensusAdapter,
    min_batch_size: u64,
    max_delay: Duration,
    metrics: AuthorityMetrics,
}

impl AuthorityServer {
    pub fn new(
        base_address: String,
        base_port: u16,
        buffer_size: usize,
        state: Arc<AuthorityState>,
        consensus_address: SocketAddr,
        tx_consensus_listener: Sender<ConsensusListenerMessage>,
    ) -> Self {
        let consensus_adapter = ConsensusAdapter::new(
            consensus_address,
            buffer_size,
            state.committee.clone(),
            tx_consensus_listener,
            /* max_delay */ Duration::from_millis(2_000),
        );

        // Start Prometheus metrics HTTP server/scrape point
        let prom_binding = PROM_PORT_ADDR.parse().unwrap();
        prometheus_exporter::start(prom_binding).expect("Failed to start Prometheus exporter");
        info!("Starting Prometheus HTTP endpoint at {}", PROM_PORT_ADDR);

        Self {
            server: NetworkServer::new(base_address, base_port, buffer_size),
            state,
            consensus_adapter,
            min_batch_size: MIN_BATCH_SIZE,
            max_delay: Duration::from_millis(MAX_DELAY_MILLIS),
            metrics: AuthorityMetrics::new(),
        }
    }

    /// Create a batch subsystem, register it with the authority state, and
    /// launch a task that manages it. Return the join handle of this task.
    pub async fn spawn_batch_subsystem(
        &self,
        min_batch_size: u64,
        max_delay: Duration,
    ) -> SuiResult<tokio::task::JoinHandle<SuiResult<()>>> {
        // Start the batching subsystem, and register the handles with the authority.
        let state = self.state.clone();
        let _batch_join_handle =
            tokio::task::spawn(
                async move { state.run_batch_service(min_batch_size, max_delay).await },
            );

        Ok(_batch_join_handle)
    }

    pub async fn spawn(self) -> Result<AuthorityServerHandle, io::Error> {
        let address = format!("{}:{}", self.server.base_address, self.server.base_port);
        self.spawn_with_bind_address(&address).await
    }

    pub async fn spawn_with_bind_address(
        self,
        address: &str,
    ) -> Result<AuthorityServerHandle, io::Error> {
        // Start the batching subsystem
        let _join_handle = self
            .spawn_batch_subsystem(self.min_batch_size, self.max_delay)
            .await;

        let std_listener = std::net::TcpListener::bind(address)?;

        let local_addr = std_listener.local_addr()?;
        let host = local_addr.ip();
        let port = local_addr.port();
        info!("Listening to TCP traffic on {host}:{port}");
        // see https://fly.io/blog/the-tokio-1-x-upgrade/#tcplistener-from_std-needs-to-be-set-to-nonblocking
        std_listener.set_nonblocking(true)?;
        let listener =
            tokio_stream::wrappers::TcpListenerStream::new(TcpListener::from_std(std_listener)?);

        let (tx_cancellation, rx_cancellation) = tokio::sync::oneshot::channel();
        let service = tonic::transport::Server::builder()
            .add_service(ValidatorServer::new(self))
            .serve_with_incoming_shutdown(listener, rx_cancellation.map(|_| ()));
        let handle = AuthorityServerHandle {
            tx_cancellation,
            local_addr,
            handle: tokio::spawn(service),
        };
        Ok(handle)
    }
}

#[async_trait]
impl Validator for AuthorityServer {
    async fn transaction(
        &self,
        request: tonic::Request<BincodeEncodedPayload>,
    ) -> Result<tonic::Response<BincodeEncodedPayload>, tonic::Status> {
        let mut transaction: Transaction = request
            .into_inner()
            .deserialize()
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;

        let mut obligation = VerificationObligation::default();
        transaction
            .add_tx_sig_to_verification_obligation(&mut obligation)
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;
        obligation
            .verify_all()
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;
        //TODO This is really really bad, we should have different types for checked transactions
        transaction.is_checked = true;

        let tx_digest = transaction.digest();

        // Enable Trace Propagation across spans/processes using tx_digest
        let span = tracing::debug_span!(
            "process_tx",
            ?tx_digest,
            tx_kind = transaction.data.kind_as_str()
        );

        let info = self
            .state
            .handle_transaction(transaction)
            .instrument(span)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        let payload = BincodeEncodedPayload::try_from(&info)
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(payload))
    }

    // // Tally requests by "route" or SerializedMessage enum name
    // self.metrics
    //     .requests_by_route
    //     .with_label_values(&[message.enum_name()])
    //     .inc();

    async fn confirmation_transaction(
        &self,
        request: tonic::Request<BincodeEncodedPayload>,
    ) -> Result<tonic::Response<BincodeEncodedPayload>, tonic::Status> {
        let mut transaction: CertifiedTransaction = request
            .into_inner()
            .deserialize()
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;

        let mut obligation = VerificationObligation::default();
        transaction
            .add_to_verification_obligation(&self.state.committee, &mut obligation)
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;
        obligation
            .verify_all()
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;
        //TODO This is really really bad, we should have different types for checked transactions
        transaction.is_checked = true;

        let tx_digest = transaction.digest();
        let span = tracing::debug_span!(
            "process_cert",
            ?tx_digest,
            tx_kind = transaction.data.kind_as_str()
        );

        let confirmation_transaction = ConfirmationTransaction {
            certificate: transaction,
        };

        let info = self
            .state
            .handle_confirmation_transaction(confirmation_transaction)
            .instrument(span)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        let payload = BincodeEncodedPayload::try_from(&info)
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(payload))
    }

    async fn consensus_transaction(
        &self,
        request: tonic::Request<BincodeEncodedPayload>,
    ) -> Result<tonic::Response<BincodeEncodedPayload>, tonic::Status> {
        let transaction: ConsensusTransaction = request
            .into_inner()
            .deserialize()
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;

        let info = self
            .consensus_adapter
            .submit(&transaction)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        // For some reason the output of consensus changed, we should change it back
        let info = deserialize_message(&info[..]).unwrap();
        let info = deserialize_transaction_info(info).unwrap();

        let payload = BincodeEncodedPayload::try_from(&info)
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(payload))
    }

    async fn account_info(
        &self,
        request: tonic::Request<BincodeEncodedPayload>,
    ) -> Result<tonic::Response<BincodeEncodedPayload>, tonic::Status> {
        let request: AccountInfoRequest = request
            .into_inner()
            .deserialize()
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;

        let response = self
            .state
            .handle_account_info_request(request)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        let payload = BincodeEncodedPayload::try_from(&response)
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(payload))
    }

    async fn object_info(
        &self,
        request: tonic::Request<BincodeEncodedPayload>,
    ) -> Result<tonic::Response<BincodeEncodedPayload>, tonic::Status> {
        let request: ObjectInfoRequest = request
            .into_inner()
            .deserialize()
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;

        let response = self
            .state
            .handle_object_info_request(request)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        let payload = BincodeEncodedPayload::try_from(&response)
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(payload))
    }

    async fn transaction_info(
        &self,
        request: tonic::Request<BincodeEncodedPayload>,
    ) -> Result<tonic::Response<BincodeEncodedPayload>, tonic::Status> {
        let request: TransactionInfoRequest = request
            .into_inner()
            .deserialize()
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;

        let response = self
            .state
            .handle_transaction_info_request(request)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        let payload = BincodeEncodedPayload::try_from(&response)
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(payload))
    }

    type BatchInfoStream = BoxStream<'static, Result<BincodeEncodedPayload, tonic::Status>>;

    async fn batch_info(
        &self,
        request: tonic::Request<BincodeEncodedPayload>,
    ) -> Result<tonic::Response<Self::BatchInfoStream>, tonic::Status> {
        let request: BatchInfoRequest = request
            .into_inner()
            .deserialize()
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;

        let xstream = self
            .state
            .handle_batch_streaming(request)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        let response =
            // items
            // .chain(subscriber)
            xstream
            .map_err(|e| tonic::Status::internal(e.to_string()))
            .map_ok(|item| {
                BincodeEncodedPayload::try_from(&item).expect("serialization should not fail")
            });

        Ok(tonic::Response::new(Box::pin(response)))
    }
}
