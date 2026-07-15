// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio_rustls::rustls::pki_types::CertificateDer;

pub(crate) type ActiveConnections<A = std::net::SocketAddr> =
    Arc<RwLock<HashMap<ConnectionId, ConnectionInfo<A>>>>;

pub type ConnectionId = usize;

#[derive(Debug)]
pub struct ConnectionInfo<A>(Arc<Inner<A>>);

#[derive(Clone, Debug)]
pub struct PeerCertificates(Arc<Vec<tokio_rustls::rustls::pki_types::CertificateDer<'static>>>);

impl PeerCertificates {
    pub fn peer_certs(&self) -> &[tokio_rustls::rustls::pki_types::CertificateDer<'static>] {
        self.0.as_ref()
    }
}

impl<A> ConnectionInfo<A> {
    pub(crate) fn new(
        address: A,
        peer_certificates: Option<Arc<Vec<CertificateDer<'static>>>>,
        graceful_shutdown_token: tokio_util::sync::CancellationToken,
    ) -> Self {
        Self(Arc::new(Inner {
            address,
            time_established: std::time::Instant::now(),
            peer_certificates: peer_certificates.map(PeerCertificates),
            graceful_shutdown_token,
        }))
    }

    /// The peer's remote address
    pub fn remote_address(&self) -> &A {
        &self.0.address
    }

    /// Time the Connection was established
    pub fn time_established(&self) -> std::time::Instant {
        self.0.time_established
    }

    pub fn peer_certificates(&self) -> Option<&PeerCertificates> {
        self.0.peer_certificates.as_ref()
    }

    /// A stable identifier for this connection
    pub fn id(&self) -> ConnectionId {
        &*self.0 as *const _ as usize
    }

    /// Trigger a graceful shutdown of this connection
    pub fn close(&self) {
        self.0.graceful_shutdown_token.cancel()
    }
}

#[derive(Debug)]
struct Inner<A = std::net::SocketAddr> {
    address: A,

    // Time that the connection was established
    time_established: std::time::Instant,

    peer_certificates: Option<PeerCertificates>,
    graceful_shutdown_token: tokio_util::sync::CancellationToken,
}

#[derive(Debug, Clone)]
pub struct ConnectInfo<A = std::net::SocketAddr> {
    /// Returns the local address of this connection.
    pub local_addr: A,
    /// Returns the remote (peer) address of this connection.
    pub remote_addr: A,
}

impl<A> ConnectInfo<A> {
    /// Return the local address the IO resource is connected.
    pub fn local_addr(&self) -> &A {
        &self.local_addr
    }

    /// Return the remote address the IO resource is connected too.
    pub fn remote_addr(&self) -> &A {
        &self.remote_addr
    }
}
