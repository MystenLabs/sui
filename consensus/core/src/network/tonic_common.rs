// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared internals for the validator (`tonic_network.rs`) and observer
//! (`observer.rs`) Tonic implementations: prost message types referenced from
//! both generated services, channel establishment, fetch-stream draining,
//! peer-cert public-key extraction, and HTTP/2 server setup helpers.

use std::time::Duration;

use bytes::Bytes;
use consensus_config::{NetworkKeyPair, NetworkPublicKey};
use consensus_types::block::{BlockRef, Round};
use mysten_network::{Multiaddr, callback::CallbackLayer};
use sui_http::ServerHandle;
use tonic::Streaming;
use tower_http::trace::{DefaultMakeSpan, DefaultOnFailure, TraceLayer};
use tracing::{debug, info, trace, warn};

use crate::{
    CommitIndex,
    context::Context,
    error::{ConsensusError, ConsensusResult},
    network::{
        metrics_layer::MetricsCallbackMaker, to_host_port_str, tonic_tls::certificate_server_name,
    },
};

/// Tonic channel wrapped with metrics + tracing layers.
pub(crate) type Channel = mysten_network::callback::Callback<
    tower_http::trace::Trace<
        tonic_rustls::Channel,
        tower_http::classify::SharedClassifier<tower_http::classify::GrpcErrorsAsFailures>,
    >,
    MetricsCallbackMaker,
>;

// Maximum bytes size in a single fetch_blocks() response.
// TODO: put max RPC response size in protocol config.
pub(crate) const MAX_FETCH_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

// HTTP/2 connection and stream window sizes for both validator and observer servers.
const HTTP2_INITIAL_CONNECTION_WINDOW_SIZE: u32 = 64 << 20; // 64 MB
const HTTP2_INITIAL_STREAM_WINDOW_SIZE: u32 = 32 << 20; // 32 MB

pub(crate) fn max_fetch_blocks_response_bytes(
    context: &Context,
    block_refs: &[BlockRef],
    fetch_after_rounds: &[Round],
) -> usize {
    let max_response_num_blocks = if fetch_after_rounds.is_empty() {
        block_refs
            .len()
            .min(context.parameters.max_blocks_per_fetch)
    } else if block_refs.is_empty() {
        context.parameters.max_blocks_per_fetch
    } else {
        context.parameters.max_blocks_per_sync
    };

    max_response_num_blocks
        .saturating_mul(context.protocol_config.max_transactions_in_block_bytes() as usize)
        .saturating_mul(2)
}

pub(crate) fn chunk_blocks(blocks: Vec<Bytes>, chunk_limit: usize) -> Vec<Vec<Bytes>> {
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

// =====================================================================
// Shared Tonic prost types — referenced from both ConsensusService and
// ObserverService via consensus/core/build.rs. Field tags must remain
// stable: changing them is a wire-incompatible change.
// =====================================================================

#[derive(Clone, prost::Message)]
pub(crate) struct FetchBlocksRequest {
    #[prost(bytes = "vec", repeated, tag = "1")]
    pub(crate) block_refs: Vec<Vec<u8>>,
    // The round per authority after which blocks should be fetched. The vector represents the round
    // for each authority and its length should be the same as the committee size.
    // When this field is non-empty, additional ancestors of the requested blocks can be fetched.
    #[prost(uint32, repeated, tag = "2")]
    pub(crate) fetch_after_rounds: Vec<Round>,
    // When true, missing ancestors of the requested blocks will be fetched as well.
    // When false, additional blocks are fetched depth-first from the requested block authorities.
    // This field is only meaningful when fetch_after_rounds is non-empty.
    #[prost(bool, tag = "3")]
    pub(crate) fetch_missing_ancestors: bool,
}

#[derive(Clone, prost::Message)]
pub(crate) struct FetchBlocksResponse {
    // The response of the requested blocks as Serialized SignedBlock.
    #[prost(bytes = "bytes", repeated, tag = "1")]
    pub(crate) blocks: Vec<Bytes>,
}

#[derive(Clone, prost::Message)]
pub(crate) struct FetchCommitsRequest {
    #[prost(uint32, tag = "1")]
    pub(crate) start: CommitIndex,
    #[prost(uint32, tag = "2")]
    pub(crate) end: CommitIndex,
}

#[derive(Clone, prost::Message)]
pub(crate) struct FetchCommitsResponse {
    // Serialized consecutive Commit.
    #[prost(bytes = "bytes", repeated, tag = "1")]
    pub(crate) commits: Vec<Bytes>,
    // Serialized SignedBlock that certify the last commit from above.
    #[prost(bytes = "bytes", repeated, tag = "2")]
    pub(crate) certifier_blocks: Vec<Bytes>,
}

// =====================================================================
// Channel construction
// =====================================================================

/// Establishes a Tonic channel to `peer_address`, authenticated via TLS using
/// `network_keypair` (local) and `peer_network_key` (remote), wrapped with the
/// outbound metrics + tracing layer stack. Retries `connect()` up to `timeout`.
pub(crate) async fn connect_channel(
    context: &Context,
    network_keypair: NetworkKeyPair,
    peer_network_key: NetworkPublicKey,
    peer_address: &Multiaddr,
    timeout: Duration,
) -> ConsensusResult<Channel> {
    let address = to_host_port_str(peer_address).map_err(|e| {
        ConsensusError::NetworkConfig(format!("Cannot convert address to host:port: {e:?}"))
    })?;
    let address = format!("https://{address}");
    let tonic_config = &context.parameters.tonic;
    let buffer_size = tonic_config.connection_buffer_size;
    let client_tls_config = sui_tls::create_rustls_client_config(
        peer_network_key.into_inner(),
        certificate_server_name(context),
        Some(network_keypair.private_key().into_inner()),
    );
    let endpoint = tonic_rustls::Channel::from_shared(address.clone())
        .map_err(|e| ConsensusError::NetworkConfig(format!("invalid URI '{address}': {e}")))?
        .connect_timeout(timeout)
        .initial_connection_window_size(Some(buffer_size as u32))
        .initial_stream_window_size(Some(buffer_size as u32 / 2))
        .keep_alive_while_idle(true)
        .keep_alive_timeout(tonic_config.keepalive_interval)
        .http2_keep_alive_interval(tonic_config.keepalive_interval)
        // tcp keepalive is probably unnecessary and is unsupported by msim.
        .user_agent("mysticeti")
        .unwrap()
        .tls_config(client_tls_config)
        .unwrap();

    let deadline = tokio::time::Instant::now() + timeout;
    let channel = loop {
        trace!("Connecting to endpoint at {address}");
        match endpoint.connect().await {
            Ok(channel) => break channel,
            Err(e) => {
                debug!("Failed to connect to endpoint at {address}: {e:?}");
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
            context.metrics.network_metrics.outbound.clone(),
            context.parameters.tonic.excessive_message_size,
        )))
        .layer(
            TraceLayer::new_for_grpc()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::TRACE))
                .on_failure(DefaultOnFailure::new().level(tracing::Level::DEBUG)),
        )
        .service(channel);

    Ok(channel)
}

// =====================================================================
// Fetch-blocks streaming drain helper
// =====================================================================

/// Drains a server-streaming response of block batches, accumulating blocks
/// until either the stream ends or `max_allowed_bytes` is exceeded.
///
/// On a mid-stream error: if any blocks have been received already, logs a
/// warning and returns the partial result; otherwise maps `DeadlineExceeded`
/// to `NetworkRequestTimeout` and other errors to `NetworkRequest`.
pub(crate) async fn drain_blocks_stream<R, F>(
    mut stream: Streaming<R>,
    max_allowed_bytes: usize,
    op_name: &'static str,
    extract: F,
) -> ConsensusResult<Vec<Bytes>>
where
    F: Fn(R) -> Vec<Bytes>,
{
    let mut blocks = vec![];
    let mut total_fetched_bytes = 0;
    loop {
        match stream.message().await {
            Ok(Some(response)) => {
                let new_blocks = extract(response);
                for b in &new_blocks {
                    total_fetched_bytes += b.len();
                }
                blocks.extend(new_blocks);
                if total_fetched_bytes > max_allowed_bytes {
                    info!(
                        "{op_name}() fetched bytes exceeded limit: {} > {}, terminating stream.",
                        total_fetched_bytes, max_allowed_bytes,
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
                            "{op_name} failed mid-stream: {e:?}"
                        )));
                    }
                    return Err(ConsensusError::NetworkRequest(format!(
                        "{op_name} failed mid-stream: {e:?}"
                    )));
                } else {
                    warn!("{op_name} failed mid-stream: {e:?}");
                    break;
                }
            }
        }
    }
    Ok(blocks)
}

// =====================================================================
// TLS peer cert -> public key extraction
// =====================================================================

/// Extracts the peer's `NetworkPublicKey` from a single-cert TLS chain.
/// Returns `None` if there isn't exactly one cert or the cert can't be parsed.
pub(crate) fn extract_peer_public_key(
    peer_certificates: &sui_http::PeerCertificates,
) -> Option<NetworkPublicKey> {
    let certs = peer_certificates.peer_certs();
    if certs.len() != 1 {
        trace!(
            "Unexpected number of certificates from TLS stream: {}",
            certs.len()
        );
        return None;
    }
    let public_key = sui_tls::public_key_from_certificate(&certs[0])
        .map_err(|e| {
            trace!("Failed to extract public key from certificate: {e:?}");
            e
        })
        .ok()?;
    Some(NetworkPublicKey::new(public_key))
}

// =====================================================================
// HTTP/2 server config + serve retry helper
// =====================================================================

pub(crate) fn http2_server_config(keepalive: Duration) -> sui_http::Config {
    sui_http::Config::default()
        .initial_connection_window_size(HTTP2_INITIAL_CONNECTION_WINDOW_SIZE)
        .initial_stream_window_size(HTTP2_INITIAL_STREAM_WINDOW_SIZE)
        .http2_keepalive_interval(Some(keepalive))
        .http2_keepalive_timeout(Some(keepalive))
        .accept_http1(false)
}

/// Runs `serve_fn` repeatedly with a 20-second deadline. During simtest
/// crash/restart tests an older instance may briefly hold the TCP port; this
/// retry gives the kernel time to release it before failing hard.
pub(crate) async fn serve_with_retry<F, E>(name: &'static str, mut serve_fn: F) -> ServerHandle
where
    F: FnMut() -> Result<ServerHandle, E>,
    E: std::fmt::Debug,
{
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        match serve_fn() {
            Ok(server) => return server,
            Err(err) => {
                warn!("Error starting {name} server: {err:?}");
                if std::time::Instant::now() > deadline {
                    panic!("Failed to start {name} server within required deadline");
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}
