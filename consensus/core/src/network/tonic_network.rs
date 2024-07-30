// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    net::{SocketAddr, SocketAddrV4, SocketAddrV6},
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use bytes::Bytes;
use cfg_if::cfg_if;
use consensus_config::{AuthorityIndex, NetworkKeyPair, NetworkPublicKey};
use futures::{stream, Stream, StreamExt as _};
use hyper::server::conn::Http;
use mysten_common::sync::notify_once::NotifyOnce;
use mysten_metrics::monitored_future;
use mysten_network::{
    callback::{CallbackLayer, MakeCallbackHandler, ResponseHandler},
    multiaddr::Protocol,
    Multiaddr,
};
use parking_lot::RwLock;
use tokio::{
    pin,
    task::JoinSet,
    time::{timeout, Instant},
};
use tokio_rustls::TlsAcceptor;
use tokio_stream::{iter, Iter};
use tonic::{transport::Server, Request, Response, Streaming};
use tower_http::{
    trace::{DefaultMakeSpan, DefaultOnFailure, TraceLayer},
    ServiceBuilderExt,
};
use tracing::{debug, error, info, trace, warn};

use super::{
    metrics_layer::{MetricsCallbackMaker, MetricsResponseCallback, SizedRequest, SizedResponse},
    tonic_gen::{
        consensus_service_client::ConsensusServiceClient,
        consensus_service_server::ConsensusService,
    },
    tonic_tls::create_rustls_client_config,
    BlockStream, NetworkClient, NetworkManager, NetworkService,
};
use crate::{
    block::{BlockRef, VerifiedBlock},
    commit::CommitRange,
    context::Context,
    error::{ConsensusError, ConsensusResult},
    network::{
        tonic_gen::consensus_service_server::ConsensusServiceServer,
        tonic_tls::create_rustls_server_config,
    },
    CommitIndex, Round,
};

// Maximum bytes size in a single fetch_blocks()response.
// TODO: put max RPC response size in protocol config.
const MAX_FETCH_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

// Maximum total bytes fetched in a single fetch_blocks() call, after combining the responses.
const MAX_TOTAL_FETCHED_BYTES: usize = 128 * 1024 * 1024;

// Maximum number of connections in backlog.
#[cfg(not(msim))]
const MAX_CONNECTIONS_BACKLOG: u32 = 1024;

// The time we are willing to wait for a connection to get gracefully shutdown before we attempt to
// forcefully shutdown its task.
const CONNECTION_SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(1);

// Implements Tonic RPC client for Consensus.
pub(crate) struct TonicClient {
    context: Arc<Context>,
    network_keypair: NetworkKeyPair,
    channel_pool: Arc<ChannelPool>,
}

impl TonicClient {
    pub(crate) fn new(context: Arc<Context>, network_keypair: NetworkKeyPair) -> Self {
        Self {
            context: context.clone(),
            network_keypair,
            channel_pool: Arc::new(ChannelPool::new(context)),
        }
    }

    async fn get_client(
        &self,
        peer: AuthorityIndex,
        timeout: Duration,
    ) -> ConsensusResult<ConsensusServiceClient<Channel>> {
        let config = &self.context.parameters.tonic;
        let channel = self
            .channel_pool
            .get_channel(self.network_keypair.clone(), peer, timeout)
            .await?;
        Ok(ConsensusServiceClient::new(channel)
            .max_encoding_message_size(config.message_size_limit)
            .max_decoding_message_size(config.message_size_limit))
    }
}

// TODO: make sure callsites do not send request to own index, and return error otherwise.
#[async_trait]
impl NetworkClient for TonicClient {
    const SUPPORT_STREAMING: bool = true;

    async fn send_block(
        &self,
        peer: AuthorityIndex,
        block: &VerifiedBlock,
        timeout: Duration,
    ) -> ConsensusResult<()> {
        let mut client = self.get_client(peer, timeout).await?;
        let mut request = Request::new(SendBlockRequest {
            block: block.serialized().clone(),
        });
        request.set_timeout(timeout);
        client
            .send_block(request)
            .await
            .map_err(|e| ConsensusError::NetworkRequest(format!("send_block failed: {e:?}")))?;
        Ok(())
    }

    async fn subscribe_blocks(
        &self,
        peer: AuthorityIndex,
        last_received: Round,
        timeout: Duration,
    ) -> ConsensusResult<BlockStream> {
        let mut client = self.get_client(peer, timeout).await?;
        // TODO: add sampled block acknowledgments for latency measurements.
        let request = Request::new(stream::once(async move {
            SubscribeBlocksRequest {
                last_received_round: last_received,
            }
        }));
        let response = client.subscribe_blocks(request).await.map_err(|e| {
            ConsensusError::NetworkRequest(format!("subscribe_blocks failed: {e:?}"))
        })?;
        let stream = response.into_inner().filter_map(move |b| async move {
            match b {
                Ok(response) => Some(response.block),
                Err(e) => {
                    debug!("Network error received from {}: {e:?}", peer);
                    None
                }
            }
        });
        let rate_limited_stream =
            tokio_stream::StreamExt::throttle(stream, self.context.parameters.min_round_delay / 2)
                .boxed();
        Ok(rate_limited_stream)
    }

    async fn fetch_blocks(
        &self,
        peer: AuthorityIndex,
        block_refs: Vec<BlockRef>,
        highest_accepted_rounds: Vec<Round>,
        timeout: Duration,
    ) -> ConsensusResult<Vec<Bytes>> {
        let mut client = self.get_client(peer, timeout).await?;
        let mut request = Request::new(FetchBlocksRequest {
            block_refs: block_refs
                .iter()
                .filter_map(|r| match bcs::to_bytes(r) {
                    Ok(serialized) => Some(serialized),
                    Err(e) => {
                        debug!("Failed to serialize block ref {:?}: {e:?}", r);
                        None
                    }
                })
                .collect(),
            highest_accepted_rounds,
        });
        request.set_timeout(timeout);
        let mut stream = client
            .fetch_blocks(request)
            .await
            .map_err(|e| {
                if e.code() == tonic::Code::DeadlineExceeded {
                    ConsensusError::NetworkRequestTimeout(format!("fetch_blocks failed: {e:?}"))
                } else {
                    ConsensusError::NetworkRequest(format!("fetch_blocks failed: {e:?}"))
                }
            })?
            .into_inner();
        let mut blocks = vec![];
        let mut total_fetched_bytes = 0;
        loop {
            match stream.message().await {
                Ok(Some(response)) => {
                    for b in &response.blocks {
                        total_fetched_bytes += b.len();
                    }
                    blocks.extend(response.blocks);
                    if total_fetched_bytes > MAX_TOTAL_FETCHED_BYTES {
                        info!(
                            "fetch_blocks() fetched bytes exceeded limit: {} > {}, terminating stream.",
                            total_fetched_bytes, MAX_TOTAL_FETCHED_BYTES,
                        );
                        break;
                    }
                }
                Ok(None) => {
                    break;
                }
                Err(e) => {
                    if blocks.is_empty() {
                        if e.code() == tonic::Code::DeadlineExceeded {
                            return Err(ConsensusError::NetworkRequestTimeout(format!(
                                "fetch_blocks failed mid-stream: {e:?}"
                            )));
                        }
                        return Err(ConsensusError::NetworkRequest(format!(
                            "fetch_blocks failed mid-stream: {e:?}"
                        )));
                    } else {
                        warn!("fetch_blocks failed mid-stream: {e:?}");
                        break;
                    }
                }
            }
        }
        Ok(blocks)
    }

    async fn fetch_commits(
        &self,
        peer: AuthorityIndex,
        commit_range: CommitRange,
        timeout: Duration,
    ) -> ConsensusResult<(Vec<Bytes>, Vec<Bytes>)> {
        let mut client = self.get_client(peer, timeout).await?;
        let mut request = Request::new(FetchCommitsRequest {
            start: commit_range.start(),
            end: commit_range.end(),
        });
        request.set_timeout(timeout);
        let response = client
            .fetch_commits(request)
            .await
            .map_err(|e| ConsensusError::NetworkRequest(format!("fetch_commits failed: {e:?}")))?;
        let response = response.into_inner();
        Ok((response.commits, response.certifier_blocks))
    }

    async fn fetch_latest_blocks(
        &self,
        peer: AuthorityIndex,
        authorities: Vec<AuthorityIndex>,
        timeout: Duration,
    ) -> ConsensusResult<Vec<Bytes>> {
        let mut client = self.get_client(peer, timeout).await?;
        let mut request = Request::new(FetchLatestBlocksRequest {
            authorities: authorities
                .iter()
                .map(|authority| authority.value() as u32)
                .collect(),
        });
        request.set_timeout(timeout);
        let mut stream = client
            .fetch_latest_blocks(request)
            .await
            .map_err(|e| {
                if e.code() == tonic::Code::DeadlineExceeded {
                    ConsensusError::NetworkRequestTimeout(format!("fetch_blocks failed: {e:?}"))
                } else {
                    ConsensusError::NetworkRequest(format!("fetch_blocks failed: {e:?}"))
                }
            })?
            .into_inner();
        let mut blocks = vec![];
        let mut total_fetched_bytes = 0;
        loop {
            match stream.message().await {
                Ok(Some(response)) => {
                    for b in &response.blocks {
                        total_fetched_bytes += b.len();
                    }
                    blocks.extend(response.blocks);
                    if total_fetched_bytes > MAX_TOTAL_FETCHED_BYTES {
                        info!(
                            "fetch_blocks() fetched bytes exceeded limit: {} > {}, terminating stream.",
                            total_fetched_bytes, MAX_TOTAL_FETCHED_BYTES,
                        );
                        break;
                    }
                }
                Ok(None) => {
                    break;
                }
                Err(e) => {
                    if blocks.is_empty() {
                        if e.code() == tonic::Code::DeadlineExceeded {
                            return Err(ConsensusError::NetworkRequestTimeout(format!(
                                "fetch_blocks failed mid-stream: {e:?}"
                            )));
                        }
                        return Err(ConsensusError::NetworkRequest(format!(
                            "fetch_blocks failed mid-stream: {e:?}"
                        )));
                    } else {
                        warn!("fetch_blocks failed mid-stream: {e:?}");
                        break;
                    }
                }
            }
        }
        Ok(blocks)
    }
}

// Tonic channel wrapped with layers.
type Channel = mysten_network::callback::Callback<
    tower_http::trace::Trace<
        tonic::transport::Channel,
        tower_http::classify::SharedClassifier<tower_http::classify::GrpcErrorsAsFailures>,
    >,
    MetricsCallbackMaker,
>;

/// Manages a pool of connections to peers to avoid constantly reconnecting,
/// which can be expensive.
struct ChannelPool {
    context: Arc<Context>,
    // Size is limited by known authorities in the committee.
    channels: RwLock<BTreeMap<AuthorityIndex, Channel>>,
}

impl ChannelPool {
    fn new(context: Arc<Context>) -> Self {
        Self {
            context,
            channels: RwLock::new(BTreeMap::new()),
        }
    }

    async fn get_channel(
        &self,
        network_keypair: NetworkKeyPair,
        peer: AuthorityIndex,
        timeout: Duration,
    ) -> ConsensusResult<Channel> {
        {
            let channels = self.channels.read();
            if let Some(channel) = channels.get(&peer) {
                return Ok(channel.clone());
            }
        }

        let authority = self.context.committee.authority(peer);
        let address = to_host_port_str(&authority.address).map_err(|e| {
            ConsensusError::NetworkConfig(format!("Cannot convert address to host:port: {e:?}"))
        })?;
        let address = format!("https://{address}");
        let config = &self.context.parameters.tonic;
        let buffer_size = config.connection_buffer_size;
        let endpoint = tonic::transport::Channel::from_shared(address.clone())
            .unwrap()
            .connect_timeout(timeout)
            .initial_connection_window_size(Some(buffer_size as u32))
            .initial_stream_window_size(Some(buffer_size as u32 / 2))
            .keep_alive_while_idle(true)
            .keep_alive_timeout(config.keepalive_interval)
            .http2_keep_alive_interval(config.keepalive_interval)
            // tcp keepalive is probably unnecessary and is unsupported by msim.
            .user_agent("mysticeti")
            .unwrap();

        let client_tls_config = create_rustls_client_config(&self.context, network_keypair, peer);
        let https_connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(client_tls_config)
            .https_only()
            .enable_http2()
            .build();

        let deadline = tokio::time::Instant::now() + timeout;
        let channel = loop {
            trace!("Connecting to endpoint at {address}");
            match endpoint
                .connect_with_connector(https_connector.clone())
                .await
            {
                Ok(channel) => break channel,
                Err(e) => {
                    warn!("Failed to connect to endpoint at {address}: {e:?}");
                    if tokio::time::Instant::now() >= deadline {
                        return Err(ConsensusError::NetworkClientConnection(format!(
                            "Timed out connecting to endpoint at {address}: {e:?}"
                        )));
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        };
        trace!("Connected to {address}");

        let channel = tower::ServiceBuilder::new()
            .layer(CallbackLayer::new(MetricsCallbackMaker::new(
                self.context.metrics.network_metrics.outbound.clone(),
                self.context.parameters.tonic.excessive_message_size,
            )))
            .layer(
                TraceLayer::new_for_grpc()
                    .make_span_with(DefaultMakeSpan::new().level(tracing::Level::TRACE))
                    .on_failure(DefaultOnFailure::new().level(tracing::Level::DEBUG)),
            )
            .service(channel);

        let mut channels = self.channels.write();
        // There should not be many concurrent attempts at connecting to the same peer.
        let channel = channels.entry(peer).or_insert(channel);
        Ok(channel.clone())
    }
}

/// Proxies Tonic requests to NetworkService with actual handler implementation.
struct TonicServiceProxy<S: NetworkService> {
    context: Arc<Context>,
    service: Arc<S>,
}

impl<S: NetworkService> TonicServiceProxy<S> {
    fn new(context: Arc<Context>, service: Arc<S>) -> Self {
        Self { context, service }
    }
}

#[async_trait]
impl<S: NetworkService> ConsensusService for TonicServiceProxy<S> {
    async fn send_block(
        &self,
        request: Request<SendBlockRequest>,
    ) -> Result<Response<SendBlockResponse>, tonic::Status> {
        let Some(peer_index) = request
            .extensions()
            .get::<PeerInfo>()
            .map(|p| p.authority_index)
        else {
            return Err(tonic::Status::internal("PeerInfo not found"));
        };
        let block = request.into_inner().block;
        self.service
            .handle_send_block(peer_index, block)
            .await
            .map_err(|e| tonic::Status::invalid_argument(format!("{e:?}")))?;
        Ok(Response::new(SendBlockResponse {}))
    }

    type SubscribeBlocksStream =
        Pin<Box<dyn Stream<Item = Result<SubscribeBlocksResponse, tonic::Status>> + Send>>;

    async fn subscribe_blocks(
        &self,
        request: Request<Streaming<SubscribeBlocksRequest>>,
    ) -> Result<Response<Self::SubscribeBlocksStream>, tonic::Status> {
        let Some(peer_index) = request
            .extensions()
            .get::<PeerInfo>()
            .map(|p| p.authority_index)
        else {
            return Err(tonic::Status::internal("PeerInfo not found"));
        };
        let mut request_stream = request.into_inner();
        let first_request = match request_stream.next().await {
            Some(Ok(r)) => r,
            Some(Err(e)) => {
                debug!(
                    "subscribe_blocks() request from {} failed: {e:?}",
                    peer_index
                );
                return Err(tonic::Status::invalid_argument("Request error"));
            }
            None => {
                return Err(tonic::Status::invalid_argument("Missing request"));
            }
        };
        let stream = self
            .service
            .handle_subscribe_blocks(peer_index, first_request.last_received_round)
            .await
            .map_err(|e| tonic::Status::internal(format!("{e:?}")))?
            .map(|block| Ok(SubscribeBlocksResponse { block }));
        let rate_limited_stream =
            tokio_stream::StreamExt::throttle(stream, self.context.parameters.min_round_delay / 2)
                .boxed();
        Ok(Response::new(rate_limited_stream))
    }

    type FetchBlocksStream = Iter<std::vec::IntoIter<Result<FetchBlocksResponse, tonic::Status>>>;

    async fn fetch_blocks(
        &self,
        request: Request<FetchBlocksRequest>,
    ) -> Result<Response<Self::FetchBlocksStream>, tonic::Status> {
        let Some(peer_index) = request
            .extensions()
            .get::<PeerInfo>()
            .map(|p| p.authority_index)
        else {
            return Err(tonic::Status::internal("PeerInfo not found"));
        };
        let inner = request.into_inner();
        let block_refs = inner
            .block_refs
            .into_iter()
            .filter_map(|serialized| match bcs::from_bytes(&serialized) {
                Ok(r) => Some(r),
                Err(e) => {
                    debug!("Failed to deserialize block ref {:?}: {e:?}", serialized);
                    None
                }
            })
            .collect();
        let highest_accepted_rounds = inner.highest_accepted_rounds;
        let blocks = self
            .service
            .handle_fetch_blocks(peer_index, block_refs, highest_accepted_rounds)
            .await
            .map_err(|e| tonic::Status::internal(format!("{e:?}")))?;
        let responses: std::vec::IntoIter<Result<FetchBlocksResponse, tonic::Status>> =
            chunk_blocks(blocks, MAX_FETCH_RESPONSE_BYTES)
                .into_iter()
                .map(|blocks| Ok(FetchBlocksResponse { blocks }))
                .collect::<Vec<_>>()
                .into_iter();
        let stream = iter(responses);
        Ok(Response::new(stream))
    }

    async fn fetch_commits(
        &self,
        request: Request<FetchCommitsRequest>,
    ) -> Result<Response<FetchCommitsResponse>, tonic::Status> {
        let Some(peer_index) = request
            .extensions()
            .get::<PeerInfo>()
            .map(|p| p.authority_index)
        else {
            return Err(tonic::Status::internal("PeerInfo not found"));
        };
        let request = request.into_inner();
        let (commits, certifier_blocks) = self
            .service
            .handle_fetch_commits(peer_index, (request.start..=request.end).into())
            .await
            .map_err(|e| tonic::Status::internal(format!("{e:?}")))?;
        let commits = commits
            .into_iter()
            .map(|c| c.serialized().clone())
            .collect();
        let certifier_blocks = certifier_blocks
            .into_iter()
            .map(|b| b.serialized().clone())
            .collect();
        Ok(Response::new(FetchCommitsResponse {
            commits,
            certifier_blocks,
        }))
    }

    type FetchLatestBlocksStream =
        Iter<std::vec::IntoIter<Result<FetchLatestBlocksResponse, tonic::Status>>>;

    async fn fetch_latest_blocks(
        &self,
        request: Request<FetchLatestBlocksRequest>,
    ) -> Result<Response<Self::FetchLatestBlocksStream>, tonic::Status> {
        let Some(peer_index) = request
            .extensions()
            .get::<PeerInfo>()
            .map(|p| p.authority_index)
        else {
            return Err(tonic::Status::internal("PeerInfo not found"));
        };
        let inner = request.into_inner();

        // Convert the authority indexes and validate them
        let mut authorities = vec![];
        for authority in inner.authorities.into_iter() {
            let Some(authority) = self
                .context
                .committee
                .to_authority_index(authority as usize)
            else {
                return Err(tonic::Status::internal(format!(
                    "Invalid authority index provided {authority}"
                )));
            };
            authorities.push(authority);
        }

        let blocks = self
            .service
            .handle_fetch_latest_blocks(peer_index, authorities)
            .await
            .map_err(|e| tonic::Status::internal(format!("{e:?}")))?;
        let responses: std::vec::IntoIter<Result<FetchLatestBlocksResponse, tonic::Status>> =
            chunk_blocks(blocks, MAX_FETCH_RESPONSE_BYTES)
                .into_iter()
                .map(|blocks| Ok(FetchLatestBlocksResponse { blocks }))
                .collect::<Vec<_>>()
                .into_iter();
        let stream = iter(responses);
        Ok(Response::new(stream))
    }
}

/// Manages the lifecycle of Tonic network client and service. Typical usage during initialization:
/// 1. Create a new `TonicManager`.
/// 2. Take `TonicClient` from `TonicManager::client()`.
/// 3. Create consensus components.
/// 4. Create `TonicService` for consensus service handler.
/// 5. Install `TonicService` to `TonicManager` with `TonicManager::install_service()`.
pub(crate) struct TonicManager {
    context: Arc<Context>,
    network_keypair: NetworkKeyPair,
    client: Arc<TonicClient>,
    server: JoinSet<()>,
    shutdown_notif: Arc<NotifyOnce>,
}

impl TonicManager {
    pub(crate) fn new(context: Arc<Context>, network_keypair: NetworkKeyPair) -> Self {
        Self {
            context: context.clone(),
            network_keypair: network_keypair.clone(),
            client: Arc::new(TonicClient::new(context, network_keypair)),
            server: JoinSet::new(),
            shutdown_notif: Arc::new(NotifyOnce::new()),
        }
    }
}

impl<S: NetworkService> NetworkManager<S> for TonicManager {
    type Client = TonicClient;

    fn new(context: Arc<Context>, network_keypair: NetworkKeyPair) -> Self {
        TonicManager::new(context, network_keypair)
    }

    fn client(&self) -> Arc<Self::Client> {
        self.client.clone()
    }

    async fn install_service(&mut self, service: Arc<S>) {
        self.context
            .metrics
            .network_metrics
            .network_type
            .with_label_values(&["tonic"])
            .set(1);

        debug!("Starting tonic service");

        let authority = self.context.committee.authority(self.context.own_index);
        // Bind to localhost in unit tests since only local networking is needed.
        // Bind to the unspecified address to allow the actual address to be assigned,
        // in simtest and production.
        cfg_if!(
            if #[cfg(test)] {
                let own_address = authority.address.with_localhost_ip();
            } else {
                let own_address = authority.address.with_zero_ip();
            }
        );
        let own_address = to_socket_addr(&own_address).unwrap();
        let service = TonicServiceProxy::new(self.context.clone(), service);
        let config = &self.context.parameters.tonic;

        let consensus_service = Server::builder()
            .layer(
                TraceLayer::new_for_grpc()
                    .make_span_with(DefaultMakeSpan::new().level(tracing::Level::TRACE))
                    .on_failure(DefaultOnFailure::new().level(tracing::Level::DEBUG)),
            )
            .initial_connection_window_size(64 << 20)
            .initial_stream_window_size(32 << 20)
            .http2_keepalive_interval(Some(config.keepalive_interval))
            .http2_keepalive_timeout(Some(config.keepalive_interval))
            // tcp keepalive is unsupported by msim
            .add_service(
                ConsensusServiceServer::new(service)
                    .max_encoding_message_size(config.message_size_limit)
                    .max_decoding_message_size(config.message_size_limit),
            )
            .into_service();

        let inbound_metrics = self.context.metrics.network_metrics.inbound.clone();
        let excessive_message_size = self.context.parameters.tonic.excessive_message_size;

        let mut http = Http::new();
        http.http2_only(true);
        let http = Arc::new(http);

        let tls_server_config =
            create_rustls_server_config(&self.context, self.network_keypair.clone());
        let tls_acceptor = TlsAcceptor::from(Arc::new(tls_server_config));

        // Create listener to incoming connections.
        let deadline = Instant::now() + Duration::from_secs(20);
        let listener = loop {
            if Instant::now() > deadline {
                panic!("Failed to start server: timeout");
            }
            cfg_if!(
                if #[cfg(msim)] {
                    // msim does not have a working stub for TcpSocket. So create TcpListener directly.
                    match tokio::net::TcpListener::bind(own_address).await {
                        Ok(listener) => break listener,
                        Err(e) => {
                            warn!("Error binding to {own_address}: {e:?}");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                } else {
                    let tcp_connection_metrics = &self.context.metrics.network_metrics.tcp_connection_metrics;

                    // Try creating an ephemeral port to test the highest allowed send and recv buffer sizes.
                    // Buffer sizes are not set explicitly on the socket used for real traffic,
                    // to allow the OS to set appropriate values.
                    {
                        let ephemeral_addr = SocketAddr::new(own_address.ip(), 0);
                        let ephemeral_socket = create_socket(&ephemeral_addr);
                        if let Err(e) = ephemeral_socket.set_send_buffer_size(32 << 20) {
                            info!("Failed to set send buffer size: {e:?}");
                        }
                        if let Err(e) = ephemeral_socket.set_recv_buffer_size(32 << 20) {
                            info!("Failed to set recv buffer size: {e:?}");
                        }
                        if ephemeral_socket.bind(ephemeral_addr).is_ok() {
                            tcp_connection_metrics.socket_send_buffer_max_size.set(ephemeral_socket.send_buffer_size().unwrap_or(0) as i64);
                            tcp_connection_metrics.socket_recv_buffer_max_size.set(ephemeral_socket.recv_buffer_size().unwrap_or(0) as i64);
                        };
                    }

                    // Create TcpListener via TCP socket.
                    let socket = create_socket(&own_address);
                    match socket.bind(own_address) {
                        Ok(_) => {}
                        Err(e) => {
                            warn!("Error binding to {own_address}: {e:?}");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                    };

                    tcp_connection_metrics.socket_send_buffer_size.set(socket.send_buffer_size().unwrap_or(0) as i64);
                    tcp_connection_metrics.socket_recv_buffer_size.set(socket.recv_buffer_size().unwrap_or(0) as i64);

                    match socket.listen(MAX_CONNECTIONS_BACKLOG) {
                        Ok(listener) => break listener,
                        Err(e) => {
                            warn!("Error listening at {own_address}: {e:?}");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            );
        };

        let connections_info = Arc::new(ConnectionsInfo::new(self.context.clone()));

        let shutdown_notif = self.shutdown_notif.clone();

        self.server.spawn(monitored_future!(async move {
            let mut connection_handlers = JoinSet::new();

            loop {
                let (tcp_stream, peer_addr) = tokio::select! {
                    result = listener.accept() => {
                        match result {
                            // This is the only branch that has addition processing.
                            // Other branches continue or break from the loop.
                            Ok(incoming) => incoming,
                            Err(e) => {
                                warn!("Error accepting connection: {}", e);
                                continue;
                            }
                        }
                    },
                    Some(result) = connection_handlers.join_next() => {
                        match result {
                            Ok(Ok(())) => {},
                            Ok(Err(e)) => {
                                warn!("Error serving connection: {e:?}");
                            },
                            Err(e) => {
                                debug!("Connection task error, likely shutting down: {e:?}");
                            }
                        }
                        continue;
                    },
                    _ = shutdown_notif.wait() => {
                        info!("Received shutdown. Stopping consensus service.");
                        if timeout(CONNECTION_SHUTDOWN_GRACE_PERIOD, async {
                            while connection_handlers.join_next().await.is_some() {}
                        }).await.is_err() {
                            warn!("Failed to stop all connection handlers in {CONNECTION_SHUTDOWN_GRACE_PERIOD:?}. Forcing shutdown.");
                            connection_handlers.shutdown().await;
                        }
                        return;
                    },
                };
                trace!("Received TCP connection attempt from {peer_addr}");

                let tls_acceptor = tls_acceptor.clone();
                let consensus_service = consensus_service.clone();
                let inbound_metrics = inbound_metrics.clone();
                let http = http.clone();
                let connections_info = connections_info.clone();
                let shutdown_notif = shutdown_notif.clone();

                connection_handlers.spawn(async move {
                    let tls_stream = tls_acceptor.accept(tcp_stream).await.map_err(|e| {
                        let msg = format!("Error accepting TLS connection: {e:?}");
                        trace!(msg);
                        ConsensusError::NetworkServerConnection(msg)
                    })?;
                    trace!("Accepted TLS connection");

                    let certificate_public_key =
                        if let Some(certs) = tls_stream.get_ref().1.peer_certificates() {
                            if certs.len() != 1 {
                                let msg = format!(
                                    "Unexpected number of certificates from TLS stream: {}",
                                    certs.len()
                                );
                                trace!(msg);
                                return Err(ConsensusError::NetworkServerConnection(msg));
                            }
                            trace!("Received {} certificates", certs.len());
                            sui_tls::public_key_from_certificate(&certs[0]).map_err(|e| {
                                trace!("Failed to extract public key from certificate: {e:?}");
                                ConsensusError::NetworkServerConnection(format!(
                                    "Failed to extract public key from certificate: {e:?}"
                                ))
                            })?
                        } else {
                            return Err(ConsensusError::NetworkServerConnection(
                                "No certificate found in TLS stream".to_string(),
                            ));
                        };
                    let client_public_key = NetworkPublicKey::new(certificate_public_key);
                    // TODO: improvement connection management. limit connection per peer to 1.
                    let Some(authority_index) =
                        connections_info.authority_index(&client_public_key)
                    else {
                        let msg = format!(
                            "Failed to find the authority with public key {client_public_key:?}"
                        );
                        error!("{}", msg);
                        return Err(ConsensusError::NetworkServerConnection(msg));
                    };
                    let svc = tower::ServiceBuilder::new()
                        // NOTE: the PeerInfo extension is copied to every request served.
                        // If PeerInfo starts to contain complex values, it should be wrapped in an Arc<>.
                        .add_extension(PeerInfo { authority_index })
                        .layer(CallbackLayer::new(MetricsCallbackMaker::new(
                            inbound_metrics,
                            excessive_message_size,
                        )))
                        .service(consensus_service.clone());

                    pin! {
                        let connection = http.serve_connection(tls_stream, svc);
                    }
                    trace!("Connection ready. Starting to serve requests for {peer_addr:?}");

                    let mut has_shutdown = false;
                    loop {
                        tokio::select! {
                            result = connection.as_mut() => {
                                match result {
                                    Ok(()) => {
                                        trace!("Connection closed for {peer_addr:?}");
                                        break;
                                    },
                                    Err(e) => {
                                        let msg = format!("Connection error serving {peer_addr:?}: {e:?}");
                                        trace!(msg);
                                        return Err(ConsensusError::NetworkServerConnection(msg));
                                    },
                                }
                            },
                            _ = shutdown_notif.wait(), if !has_shutdown => {
                                trace!("Received shutdown. Stopping connection for {peer_addr:?}");
                                connection.as_mut().graceful_shutdown();
                                has_shutdown = true;
                            },
                        }
                    }

                    Ok(())
                });
            }
        }));

        info!("Server started at: {own_address}");
    }

    async fn stop(&mut self) {
        let _ = self.shutdown_notif.notify();
        self.server.join_next().await;

        self.context
            .metrics
            .network_metrics
            .network_type
            .with_label_values(&["tonic"])
            .set(0);
    }
}

/// Attempts to convert a multiaddr of the form `/[ip4,ip6,dns]/{}/udp/{port}` into
/// a host:port string.
fn to_host_port_str(addr: &Multiaddr) -> Result<String, &'static str> {
    let mut iter = addr.iter();

    match (iter.next(), iter.next()) {
        (Some(Protocol::Ip4(ipaddr)), Some(Protocol::Udp(port))) => {
            Ok(format!("{}:{}", ipaddr, port))
        }
        (Some(Protocol::Ip6(ipaddr)), Some(Protocol::Udp(port))) => {
            Ok(format!("{}:{}", ipaddr, port))
        }
        (Some(Protocol::Dns(hostname)), Some(Protocol::Udp(port))) => {
            Ok(format!("{}:{}", hostname, port))
        }

        _ => {
            tracing::warn!("unsupported multiaddr: '{addr}'");
            Err("invalid address")
        }
    }
}

/// Attempts to convert a multiaddr of the form `/[ip4,ip6]/{}/[udp,tcp]/{port}` into
/// a SocketAddr value.
fn to_socket_addr(addr: &Multiaddr) -> Result<SocketAddr, &'static str> {
    let mut iter = addr.iter();

    match (iter.next(), iter.next()) {
        (Some(Protocol::Ip4(ipaddr)), Some(Protocol::Udp(port)))
        | (Some(Protocol::Ip4(ipaddr)), Some(Protocol::Tcp(port))) => {
            Ok(SocketAddr::V4(SocketAddrV4::new(ipaddr, port)))
        }

        (Some(Protocol::Ip6(ipaddr)), Some(Protocol::Udp(port)))
        | (Some(Protocol::Ip6(ipaddr)), Some(Protocol::Tcp(port))) => {
            Ok(SocketAddr::V6(SocketAddrV6::new(ipaddr, port, 0, 0)))
        }

        _ => {
            tracing::warn!("unsupported multiaddr: '{addr}'");
            Err("invalid address")
        }
    }
}

#[cfg(not(msim))]
fn create_socket(address: &SocketAddr) -> tokio::net::TcpSocket {
    let socket = if address.is_ipv4() {
        tokio::net::TcpSocket::new_v4()
    } else if address.is_ipv6() {
        tokio::net::TcpSocket::new_v6()
    } else {
        panic!("Invalid own address: {address:?}");
    }
    .unwrap_or_else(|e| panic!("Cannot create TCP socket: {e:?}"));
    if let Err(e) = socket.set_nodelay(true) {
        info!("Failed to set TCP_NODELAY: {e:?}");
    }
    if let Err(e) = socket.set_reuseaddr(true) {
        info!("Failed to set SO_REUSEADDR: {e:?}");
    }
    socket
}

/// Looks up authority index by authority public key.
///
/// TODO: Add connection monitoring, and keep track of connected peers.
/// TODO: Maybe merge with connection_monitor.rs
struct ConnectionsInfo {
    authority_key_to_index: BTreeMap<NetworkPublicKey, AuthorityIndex>,
}

impl ConnectionsInfo {
    fn new(context: Arc<Context>) -> Self {
        let authority_key_to_index = context
            .committee
            .authorities()
            .map(|(index, authority)| (authority.network_key.clone(), index))
            .collect();
        Self {
            authority_key_to_index,
        }
    }

    fn authority_index(&self, key: &NetworkPublicKey) -> Option<AuthorityIndex> {
        self.authority_key_to_index.get(key).copied()
    }
}

/// Information about the client peer, set per connection.
#[derive(Clone, Debug)]
struct PeerInfo {
    authority_index: AuthorityIndex,
}

// Adapt MetricsCallbackMaker and MetricsResponseCallback to http.

impl SizedRequest for http::request::Parts {
    fn size(&self) -> usize {
        // TODO: implement this.
        0
    }

    fn route(&self) -> String {
        let path = self.uri.path();
        path.rsplit_once('/')
            .map(|(_, route)| route)
            .unwrap_or("unknown")
            .to_string()
    }
}

impl SizedResponse for http::response::Parts {
    fn size(&self) -> usize {
        // TODO: implement this.
        0
    }

    fn error_type(&self) -> Option<String> {
        if self.status.is_success() {
            None
        } else {
            Some(self.status.to_string())
        }
    }
}

impl MakeCallbackHandler for MetricsCallbackMaker {
    type Handler = MetricsResponseCallback;

    fn make_handler(&self, request: &http::request::Parts) -> Self::Handler {
        self.handle_request(request)
    }
}

impl ResponseHandler for MetricsResponseCallback {
    fn on_response(self, response: &http::response::Parts) {
        self.on_response(response)
    }

    fn on_error<E>(self, err: &E) {
        self.on_error(err)
    }
}

/// Network message types.
#[derive(Clone, prost::Message)]
pub(crate) struct SendBlockRequest {
    // Serialized SignedBlock.
    #[prost(bytes = "bytes", tag = "1")]
    block: Bytes,
}

#[derive(Clone, prost::Message)]
pub(crate) struct SendBlockResponse {}

#[derive(Clone, prost::Message)]
pub(crate) struct SubscribeBlocksRequest {
    #[prost(uint32, tag = "1")]
    last_received_round: Round,
}

#[derive(Clone, prost::Message)]
pub(crate) struct SubscribeBlocksResponse {
    #[prost(bytes = "bytes", tag = "1")]
    block: Bytes,
}

#[derive(Clone, prost::Message)]
pub(crate) struct FetchBlocksRequest {
    #[prost(bytes = "vec", repeated, tag = "1")]
    block_refs: Vec<Vec<u8>>,
    // The highest accepted round per authority. The vector represents the round for each authority
    // and its length should be the same as the committee size.
    #[prost(uint32, repeated, tag = "2")]
    highest_accepted_rounds: Vec<Round>,
}

#[derive(Clone, prost::Message)]
pub(crate) struct FetchBlocksResponse {
    // The response of the requested blocks as Serialized SignedBlock.
    #[prost(bytes = "bytes", repeated, tag = "1")]
    blocks: Vec<Bytes>,
}

#[derive(Clone, prost::Message)]
pub(crate) struct FetchCommitsRequest {
    #[prost(uint32, tag = "1")]
    start: CommitIndex,
    #[prost(uint32, tag = "2")]
    end: CommitIndex,
}

#[derive(Clone, prost::Message)]
pub(crate) struct FetchCommitsResponse {
    // Serialized consecutive Commit.
    #[prost(bytes = "bytes", repeated, tag = "1")]
    commits: Vec<Bytes>,
    // Serialized SignedBlock that certify the last commit from above.
    #[prost(bytes = "bytes", repeated, tag = "2")]
    certifier_blocks: Vec<Bytes>,
}

#[derive(Clone, prost::Message)]
pub(crate) struct FetchLatestBlocksRequest {
    #[prost(uint32, repeated, tag = "1")]
    authorities: Vec<u32>,
}

#[derive(Clone, prost::Message)]
pub(crate) struct FetchLatestBlocksResponse {
    // The response of the requested blocks as Serialized SignedBlock.
    #[prost(bytes = "bytes", repeated, tag = "1")]
    blocks: Vec<Bytes>,
}

fn chunk_blocks(blocks: Vec<Bytes>, chunk_limit: usize) -> Vec<Vec<Bytes>> {
    let mut chunks = vec![];
    let mut chunk = vec![];
    let mut chunk_size = 0;
    for block in blocks {
        let block_size = block.len();
        if !chunk.is_empty() && chunk_size + block_size > chunk_limit {
            chunks.push(chunk);
            chunk = vec![];
            chunk_size = 0;
        }
        chunk.push(block);
        chunk_size += block_size;
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    chunks
}
