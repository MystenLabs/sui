// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use diesel::{ConnectionError, ConnectionResult};
use diesel_async::AsyncPgConnection;
use rustls::{
    ClientConfig, DigitallySignedStruct, RootCertStore,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use tokio_postgres_rustls::MakeRustlsConnect;
use tracing::error;
use webpki_roots::TLS_SERVER_ROOTS;

/// A custom verifier that skips all server certificate verification. This mirrors libpq default
/// behavior of not doing any server verification.
#[derive(Debug)]
pub(crate) struct SkipServerCertCheck;

/// Implement the `ServerCertVerifier` trait for `SkipServerCertCheck` to always return valid. This
/// skips all server certificate verification.
impl ServerCertVerifier for SkipServerCertCheck {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::client::WebPkiServerVerifier::builder(Arc::new(root_certs()))
            .build()
            .unwrap()
            .supported_verify_schemes()
    }
}

/// Establish a PostgreSQL connection with custom TLS configuration using tokio-postgres. The
/// returned connection is compatible with diesel-async. This is needed because diesel-async does
/// not expose TLS configuration. Even with a TLS connector, the actual usage is negotiated with the
/// postgres server:
/// - Client sends SSLRequest, server responds 'S' (TLS supported) or 'N' (TLS not supported)
/// - If server supports TLS: encrypted connection using the provided config
/// - If server doesn't support TLS: falls back to plaintext (when using sslmode=prefer default)
/// - If sslmode=disable in URL: no TLS attempted regardless of connector
pub(crate) async fn establish_tls_connection(
    database_url: &str,
    tls_config: ClientConfig,
) -> ConnectionResult<AsyncPgConnection> {
    let tls = MakeRustlsConnect::new(tls_config);
    let (client, conn) = tokio_postgres::connect(database_url, tls)
        .await
        .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;

    // The `conn` object performs actual IO with the database, and tokio-postgres suggests spawning
    // it off to run in the background. This will resolve only when the connection is closed, either
    // because of a fatal error or because its associated Client has dropped and all outstanding
    // work has completed.
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            error!("Database connection terminated: {e}");
        }
    });

    // Users interact with the database through the client object. We convert it into an
    // AsyncPgConnection so it can be compatible with diesel.
    AsyncPgConnection::try_from(client).await
}

/// Builds a TLS configuration from the provided DbArgs. If tls_verify_cert is false, disable server
/// certificate verification. If tls_ca_cert_path is provided, add the custom CA certificate to the
/// root certificates.
pub(crate) fn build_tls_config(
    tls_verify_cert: bool,
    tls_ca_cert_path: Option<PathBuf>,
) -> anyhow::Result<ClientConfig> {
    if !tls_verify_cert {
        return Ok(ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerCertCheck))
            .with_no_client_auth());
    }

    let mut root_certs = root_certs();

    // Add custom CA certificate if provided
    if let Some(ca_cert_path) = &tls_ca_cert_path {
        let ca_cert_bytes = std::fs::read(ca_cert_path).with_context(|| {
            format!(
                "Failed to read CA certificate from {}",
                ca_cert_path.display()
            )
        })?;

        let certs = if ca_cert_bytes.starts_with(b"-----BEGIN CERTIFICATE-----") {
            rustls_pemfile::certs(&mut ca_cert_bytes.as_slice())
                .collect::<Result<Vec<_>, _>>()
                .with_context(|| {
                    format!(
                        "Failed to parse PEM certificates from {}",
                        ca_cert_path.display()
                    )
                })?
        } else {
            // Assume DER format for binary files
            vec![CertificateDer::from(ca_cert_bytes)]
        };

        // Add all certificates to the root store
        for cert in certs {
            root_certs
                .add(cert)
                .with_context(|| "Failed to add CA certificate to root store".to_string())?;
        }
    }

    Ok(ClientConfig::builder()
        .with_root_certificates(root_certs)
        .with_no_client_auth())
}

fn root_certs() -> RootCertStore {
    RootCertStore {
        roots: TLS_SERVER_ROOTS.to_vec(),
    }
}
