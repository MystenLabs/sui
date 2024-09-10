// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{middleware::AddExtension, Extension};
use axum_server::{
    accept::Accept,
    tls_rustls::{RustlsAcceptor, RustlsConfig},
};
use fastcrypto::ed25519::Ed25519PublicKey;
use rustls::pki_types::CertificateDer;
use std::{io, sync::Arc};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::server::TlsStream;
use tower_layer::Layer;

#[derive(Debug, Clone)]
pub struct TlsConnectionInfo {
    sni_hostname: Option<Arc<str>>,
    peer_certificates: Option<Arc<[CertificateDer<'static>]>>,
    public_key: Option<Ed25519PublicKey>,
}

impl TlsConnectionInfo {
    pub fn sni_hostname(&self) -> Option<&str> {
        self.sni_hostname.as_deref()
    }

    pub fn peer_certificates(&self) -> Option<&[CertificateDer<'static>]> {
        self.peer_certificates.as_deref()
    }

    pub fn public_key(&self) -> Option<&Ed25519PublicKey> {
        self.public_key.as_ref()
    }
}

/// An `Acceptor` that will provide `TlsConnectionInfo` as an axum `Extension` for use in handlers.
#[derive(Debug, Clone)]
pub struct TlsAcceptor {
    inner: RustlsAcceptor,
}

impl TlsAcceptor {
    pub fn new(config: rustls::ServerConfig) -> Self {
        Self {
            inner: RustlsAcceptor::new(RustlsConfig::from_config(Arc::new(config))),
        }
    }
}

type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

impl<I, S> Accept<I, S> for TlsAcceptor
where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    S: Send + 'static,
{
    type Stream = TlsStream<I>;
    type Service = AddExtension<S, TlsConnectionInfo>;
    type Future = BoxFuture<'static, io::Result<(Self::Stream, Self::Service)>>;

    fn accept(&self, stream: I, service: S) -> Self::Future {
        let acceptor = self.inner.clone();

        Box::pin(async move {
            let (stream, service) = acceptor.accept(stream, service).await?;
            let server_conn = stream.get_ref().1;

            let public_key = if let Some([peer_certificate, ..]) = server_conn.peer_certificates() {
                crate::certgen::public_key_from_certificate(peer_certificate).ok()
            } else {
                None
            };

            let tls_connect_info = TlsConnectionInfo {
                peer_certificates: server_conn.peer_certificates().map(From::from),
                sni_hostname: server_conn.server_name().map(From::from),
                public_key,
            };
            let service = Extension(tls_connect_info).layer(service);

            Ok((stream, service))
        })
    }
}
