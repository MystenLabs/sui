// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Integration tests that drive a real [`Indexer`] ingesting checkpoints over HTTPS from a mock
//! object store, asserting the production `IngestionMetrics::total_dns_resolutions` metric (how many
//! DNS lookups the indexer performs). The indexer is built through the production
//! `IngestionClient::new` path, so the real metered HTTP connector and resolver are exercised, and the
//! production client negotiates HTTP/2 with fallback to HTTP/1.1 over ALPN (which requires TLS -- hence
//! the self-signed mock servers).
//!
//! Every connection closes (HTTP/1.1 per response via `Connection: close`, HTTP/2 at the connection
//! level via GOAWAY), but the DNS consequence depends on the negotiated protocol, exercised three ways:
//! an HTTP/1.1-only store (server-driven), an HTTP/2-capable store with `remote_store_http1_only`
//! forcing the client down to HTTP/1.1 (client-driven), and an HTTP/2 store. Under HTTP/1.1 every
//! (closed) connection re-resolves DNS; under HTTP/2 the concurrent checkpoint fetches multiplex over a
//! single connection, so the indexer resolves DNS essentially zero extra times.

use std::convert::Infallible;
use std::io::Write;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::Request;
use axum::http::Response;
use axum::http::StatusCode;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::server::conn::http2;
use hyper::service::service_fn;
use hyper_util::rt::TokioExecutor;
use hyper_util::rt::TokioIo;
use prometheus::Registry;
use rustls::ServerConfig;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::PrivateKeyDer;
use rustls::pki_types::PrivatePkcs8KeyDer;
use tempfile::NamedTempFile;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use url::Url;

use sui_indexer_alt_framework::Indexer;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::IngestConcurrencyConfig;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::ingestion::IngestionService;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClient;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs;
use sui_indexer_alt_framework::ingestion::test_utils::test_checkpoint_data;
use sui_indexer_alt_framework::metrics::IngestionMetrics;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::BatchStatus;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_indexer_alt_framework::pipeline::concurrent::Handler;
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_framework::store::testing::mock_store::MockStore;
use sui_indexer_alt_framework::types::full_checkpoint_content::Checkpoint;

/// Number of checkpoints each test ingests (sequence numbers `1..=CHECKPOINTS`), fetched
/// concurrently (see `ingest_concurrency` below).
const CHECKPOINTS: u64 = 8;

/// A pipeline that fetches every checkpoint (driving ingestion, and therefore DNS resolution) but
/// produces and commits nothing -- the test only cares about the ingestion-side network behaviour.
struct DnsProbe;

#[async_trait]
impl Processor for DnsProbe {
    const NAME: &'static str = "dns_probe";
    type Value = ();

    async fn process(&self, _checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        Ok(vec![])
    }
}

#[async_trait]
impl Handler for DnsProbe {
    type Store = MockStore;
    type Batch = ();

    fn batch(
        &self,
        _batch: &mut Self::Batch,
        _values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        BatchStatus::Pending
    }

    async fn commit<'a>(
        &self,
        _batch: &Self::Batch,
        _conn: &mut <MockStore as Store>::Connection<'a>,
    ) -> anyhow::Result<usize> {
        Ok(0)
    }
}

/// Generate a fresh self-signed certificate for `localhost`, returning the DER cert/key for the TLS
/// server and the PEM the client trusts as a root.
fn self_signed_localhost() -> (CertificateDer<'static>, PrivateKeyDer<'static>, String) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let cert_der = cert.cert.der().clone();
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()));
    (cert_der, key_der, cert.cert.pem())
}

/// A rustls server config pinned to the `ring` provider (reqwest's TLS uses aws-lc-rs, so we must
/// name a provider explicitly rather than rely on an ambiguous process default) advertising `alpn`.
fn server_config(
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
    alpn: Vec<Vec<u8>>,
) -> ServerConfig {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .unwrap();
    config.alpn_protocols = alpn;
    config
}

/// Build the response for a checkpoint store request: `200` with `test_checkpoint_data(seq)` for
/// `/<seq>.binpb.zst` (including `0.binpb.zst`, fetched for the chain id), `404` otherwise. When
/// `close`, the response carries `Connection: close` so the HTTP/1.1 server tears the connection down.
fn store_response(path: &str, close: bool) -> Response<Body> {
    let body = path
        .trim_start_matches('/')
        .strip_suffix(".binpb.zst")
        .and_then(|seq| seq.parse::<u64>().ok())
        .map(test_checkpoint_data);

    let mut builder = Response::builder().status(if body.is_some() {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    });
    if close {
        builder = builder.header("connection", "close");
    }
    builder
        .body(body.map(Body::from).unwrap_or_else(Body::empty))
        .unwrap()
}

/// Spawn a TLS-terminating mock checkpoint store on `127.0.0.1`, returning its address and the PEM
/// the client must trust. When `supports_h2`, the server advertises both `h2` and `http/1.1` (a
/// production-like server that prefers HTTP/2 but can fall back -- and so still completes the ALPN
/// handshake with a client that forces HTTP/1.1); otherwise it advertises only `http/1.1`. Each
/// connection is served with the protocol that ALPN actually negotiated: HTTP/2 connections close via
/// GOAWAY after a delay -- long enough for the concurrent batch to multiplex over them -- while
/// HTTP/1.1 connections close immediately after each response (`Connection: close`).
async fn spawn_tls_store(supports_h2: bool) -> (SocketAddr, String) {
    let (cert, key, cert_pem) = self_signed_localhost();
    let alpn = if supports_h2 {
        vec![b"h2".to_vec(), b"http/1.1".to_vec()]
    } else {
        vec![b"http/1.1".to_vec()]
    };
    let acceptor = TlsAcceptor::from(Arc::new(server_config(cert, key, alpn)));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(tls) = acceptor.accept(stream).await else {
                    return;
                };
                // Serve whatever ALPN negotiated, not a fixed mode: a client may force HTTP/1.1 even
                // against this HTTP/2-capable server.
                let is_h2 = tls.get_ref().1.alpn_protocol() == Some(&b"h2"[..]);
                let io = TokioIo::new(tls);
                let service = service_fn(move |req: Request<Incoming>| async move {
                    Ok::<_, Infallible>(store_response(req.uri().path(), !is_h2))
                });

                if is_h2 {
                    let conn =
                        http2::Builder::new(TokioExecutor::new()).serve_connection(io, service);
                    tokio::pin!(conn);
                    // Serve the multiplexed batch, then GOAWAY: the connection closes, but only after
                    // the (already dispatched) concurrent streams have shared it.
                    tokio::select! {
                        _ = conn.as_mut() => {}
                        _ = tokio::time::sleep(Duration::from_secs(1)) => {
                            conn.as_mut().graceful_shutdown();
                            let _ = conn.await;
                        }
                    }
                } else {
                    let _ = http1::Builder::new().serve_connection(io, service).await;
                }
            });
        }
    });

    (addr, cert_pem)
}

/// Build a real [`Indexer`] -- through the production [`IngestionClient::new`] path -- ingesting
/// checkpoints `1..=CHECKPOINTS` concurrently over HTTPS from a mock object store (HTTP/2-capable when
/// `supports_h2`), optionally forcing the client to HTTP/1.1 via `force_http1_only`, and return how
/// many DNS lookups the indexer itself performed (the metric delta after a warm-up fetch, so the count
/// reflects the concurrent batch rather than connection set-up).
async fn indexer_dns_over_tls(supports_h2: bool, force_http1_only: bool) -> usize {
    let (addr, cert_pem) = spawn_tls_store(supports_h2).await;

    let mut ca_file = NamedTempFile::new().unwrap();
    ca_file.write_all(cert_pem.as_bytes()).unwrap();
    ca_file.flush().unwrap();

    let registry = Registry::new();
    let metrics = IngestionMetrics::new(None, &registry);

    // A hostname (not an IP literal) so the client actually resolves DNS; `localhost` resolves to the
    // loopback address the mock binds to.
    let url = Url::parse(&format!("https://localhost:{}/", addr.port())).unwrap();
    let args = IngestionClientArgs {
        remote_store_url: Some(url),
        remote_store_ca_certificate: Some(ca_file.path().to_path_buf()),
        remote_store_http1_only: force_http1_only,
        ..Default::default()
    };
    let ingestion_client = IngestionClient::new(args, metrics.clone()).unwrap();

    // Warm the connection pool (and cache the chain id) before the concurrent batch, so the HTTP/2
    // case has an established, pooled connection to multiplex onto. Without this, a cold burst of
    // concurrent requests races to open a connection per request over ALPN (`Ver::Auto` is not
    // de-duplicated), resolving DNS several times regardless of protocol and masking the multiplexing
    // we want to demonstrate. Measuring the metric delta from here excludes this set-up cost.
    tokio::time::timeout(Duration::from_secs(20), ingestion_client.checkpoint(0))
        .await
        .expect("warm-up fetch timed out (TLS misconfigured?)")
        .expect("warm-up fetch failed");
    let baseline = metrics.total_dns_resolutions.get();

    // Fetch all `CHECKPOINTS` concurrently: HTTP/1.1 needs a connection each (no multiplexing),
    // whereas HTTP/2 multiplexes them over the single warm connection.
    let ingestion_config = IngestionConfig {
        ingest_concurrency: IngestConcurrencyConfig::Fixed {
            value: CHECKPOINTS as usize,
        },
        ..Default::default()
    };
    let ingestion_service =
        IngestionService::with_clients(ingestion_client, None, ingestion_config, metrics.clone());

    let indexer_args = IndexerArgs {
        first_checkpoint: Some(1),
        last_checkpoint: Some(CHECKPOINTS),
        ..Default::default()
    };
    let mut indexer = Indexer::with_ingestion_service(
        MockStore::default(),
        indexer_args,
        ingestion_service,
        None,
        &registry,
    )
    .await
    .unwrap();

    indexer
        .concurrent_pipeline(DnsProbe, ConcurrentConfig::default())
        .await
        .unwrap();

    let mut handle = indexer.run().await.unwrap();
    handle.join().await.unwrap();

    (metrics.total_dns_resolutions.get() - baseline) as usize
}

/// object_store's HTTP store resolves DNS (an uncached `getaddrinfo`) on every new TCP connection, and
/// HTTP/1.1 cannot multiplex, so fetching `CHECKPOINTS` checkpoints concurrently against an HTTP/1.1
/// store that closes connections opens (and resolves DNS for) a connection per checkpoint.
#[tokio::test]
async fn dns_resolved_per_checkpoint_over_http1() {
    let count = indexer_dns_over_tls(false, false).await;

    assert!(
        count >= CHECKPOINTS as usize,
        "expected at least one DNS resolution per checkpoint ({CHECKPOINTS}), got {count}"
    );
}

/// Same per-checkpoint DNS behaviour as [`dns_resolved_per_checkpoint_over_http1`], but driven by the
/// client rather than the server: the store is HTTP/2-capable, yet `remote_store_http1_only` forces the
/// client to HTTP/1.1, so it cannot multiplex and resolves DNS once per checkpoint.
#[tokio::test]
async fn dns_resolved_per_checkpoint_when_http1_only_forced() {
    let count = indexer_dns_over_tls(true, true).await;

    assert!(
        count >= CHECKPOINTS as usize,
        "expected forcing HTTP/1.1 to resolve DNS per checkpoint ({CHECKPOINTS}), got {count}"
    );
}

/// Counterpart to the HTTP/1.1 tests: with no flag the production client negotiates HTTP/2 over ALPN,
/// so the same concurrent batch multiplexes over the single warm connection and the indexer resolves
/// DNS essentially zero extra times -- even though the HTTP/2 store also closes connections.
#[tokio::test]
async fn dns_resolved_once_when_http2_multiplexes() {
    let count = indexer_dns_over_tls(true, false).await;

    assert!(
        count < CHECKPOINTS as usize,
        "expected HTTP/2 multiplexing to amortize DNS (well under {CHECKPOINTS}), got {count}"
    );
}
