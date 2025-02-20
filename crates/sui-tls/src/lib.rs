// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod acceptor;
mod certgen;
mod verifier;

use std::sync::Arc;

pub use acceptor::{TlsAcceptor, TlsConnectionInfo};
pub use certgen::SelfSignedCertificate;
use rustls::ClientConfig;
pub use verifier::{
    public_key_from_certificate, AllowAll, AllowPublicKeys, Allower, ClientCertVerifier,
    ServerCertVerifier,
};

pub use rustls;

use fastcrypto::ed25519::{Ed25519PrivateKey, Ed25519PublicKey};
use tokio_rustls::rustls::ServerConfig;

pub const SUI_VALIDATOR_SERVER_NAME: &str = "sui";

pub fn create_rustls_server_config(
    private_key: Ed25519PrivateKey,
    server_name: String,
) -> ServerConfig {
    // TODO: refactor to use key bytes
    let self_signed_cert = SelfSignedCertificate::new(private_key, server_name.as_str());
    let tls_cert = self_signed_cert.rustls_certificate();
    let tls_private_key = self_signed_cert.rustls_private_key();
    let mut tls_config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .unwrap_or_else(|e| panic!("Failed to create TLS server config: {:?}", e))
    .with_no_client_auth()
    .with_single_cert(vec![tls_cert], tls_private_key)
    .unwrap_or_else(|e| panic!("Failed to create TLS server config: {:?}", e));
    tls_config.alpn_protocols = vec![b"h2".to_vec()];
    tls_config
}

/// Create a TLS server config which requires mTLS, eg the client to also provide a cert and be
/// verified by the server based on the provided policy
pub fn create_rustls_server_config_with_client_verifier<A: Allower + 'static>(
    private_key: Ed25519PrivateKey,
    server_name: String,
    allower: A,
) -> ServerConfig {
    let verifier = ClientCertVerifier::new(allower, server_name.clone());
    // TODO: refactor to use key bytes
    let self_signed_cert = SelfSignedCertificate::new(private_key, server_name.as_str());
    let tls_cert = self_signed_cert.rustls_certificate();
    let tls_private_key = self_signed_cert.rustls_private_key();
    let mut tls_config = verifier
        .rustls_server_config(vec![tls_cert], tls_private_key)
        .unwrap_or_else(|e| panic!("Failed to create TLS server config: {:?}", e));
    tls_config.alpn_protocols = vec![b"h2".to_vec()];
    tls_config
}

pub fn create_rustls_client_config(
    target_public_key: Ed25519PublicKey,
    server_name: String,
    client_key: Option<Ed25519PrivateKey>, // optional self-signed cert for client verification
) -> ClientConfig {
    let tls_config = ServerCertVerifier::new(target_public_key, server_name.clone());
    let tls_config = if let Some(private_key) = client_key {
        let self_signed_cert = SelfSignedCertificate::new(private_key, server_name.as_str());
        let tls_cert = self_signed_cert.rustls_certificate();
        let tls_private_key = self_signed_cert.rustls_private_key();
        tls_config.rustls_client_config_with_client_auth(vec![tls_cert], tls_private_key)
    } else {
        tls_config.rustls_client_config_with_no_client_auth()
    }
    .unwrap_or_else(|e| panic!("Failed to create TLS client config: {e:?}"));
    tls_config
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use fastcrypto::ed25519::Ed25519KeyPair;
    use fastcrypto::traits::KeyPair;
    use rustls::client::danger::ServerCertVerifier as _;
    use rustls::pki_types::ServerName;
    use rustls::pki_types::UnixTime;
    use rustls::server::danger::ClientCertVerifier as _;

    #[test]
    fn verify_allowall() {
        let mut rng = rand::thread_rng();
        let allowed = Ed25519KeyPair::generate(&mut rng);
        let disallowed = Ed25519KeyPair::generate(&mut rng);
        let random_cert_bob =
            SelfSignedCertificate::new(allowed.private(), SUI_VALIDATOR_SERVER_NAME);
        let random_cert_alice =
            SelfSignedCertificate::new(disallowed.private(), SUI_VALIDATOR_SERVER_NAME);

        let verifier = ClientCertVerifier::new(AllowAll, SUI_VALIDATOR_SERVER_NAME.to_string());

        // The bob passes validation
        verifier
            .verify_client_cert(&random_cert_bob.rustls_certificate(), &[], UnixTime::now())
            .unwrap();

        // The alice passes validation
        verifier
            .verify_client_cert(
                &random_cert_alice.rustls_certificate(),
                &[],
                UnixTime::now(),
            )
            .unwrap();
    }

    #[test]
    fn verify_server_cert() {
        let mut rng = rand::thread_rng();
        let allowed = Ed25519KeyPair::generate(&mut rng);
        let disallowed = Ed25519KeyPair::generate(&mut rng);
        let allowed_public_key = allowed.public().to_owned();
        let random_cert_bob =
            SelfSignedCertificate::new(allowed.private(), SUI_VALIDATOR_SERVER_NAME);
        let random_cert_alice =
            SelfSignedCertificate::new(disallowed.private(), SUI_VALIDATOR_SERVER_NAME);

        let verifier =
            ServerCertVerifier::new(allowed_public_key, SUI_VALIDATOR_SERVER_NAME.to_string());

        // The bob passes validation
        verifier
            .verify_server_cert(
                &random_cert_bob.rustls_certificate(),
                &[],
                &ServerName::try_from("example.com").unwrap(),
                &[],
                UnixTime::now(),
            )
            .unwrap();

        // The alice does not pass validation
        let err = verifier
            .verify_server_cert(
                &random_cert_alice.rustls_certificate(),
                &[],
                &ServerName::try_from("example.com").unwrap(),
                &[],
                UnixTime::now(),
            )
            .unwrap_err();
        assert!(
            matches!(err, rustls::Error::General(_)),
            "Actual error: {err:?}"
        );
    }

    #[test]
    fn verify_hashset() {
        let mut rng = rand::thread_rng();
        let allowed = Ed25519KeyPair::generate(&mut rng);
        let disallowed = Ed25519KeyPair::generate(&mut rng);

        let allowed_public_keys = BTreeSet::from([allowed.public().to_owned()]);
        let allowed_cert = SelfSignedCertificate::new(allowed.private(), SUI_VALIDATOR_SERVER_NAME);

        let disallowed_cert =
            SelfSignedCertificate::new(disallowed.private(), SUI_VALIDATOR_SERVER_NAME);

        let allowlist = AllowPublicKeys::new(allowed_public_keys);
        let verifier =
            ClientCertVerifier::new(allowlist.clone(), SUI_VALIDATOR_SERVER_NAME.to_string());

        // The allowed cert passes validation
        verifier
            .verify_client_cert(&allowed_cert.rustls_certificate(), &[], UnixTime::now())
            .unwrap();

        // The disallowed cert fails validation
        let err = verifier
            .verify_client_cert(&disallowed_cert.rustls_certificate(), &[], UnixTime::now())
            .unwrap_err();
        assert!(
            matches!(err, rustls::Error::General(_)),
            "Actual error: {err:?}"
        );

        // After removing the allowed public key from the set it now fails validation
        allowlist.update(BTreeSet::new());
        let err = verifier
            .verify_client_cert(&allowed_cert.rustls_certificate(), &[], UnixTime::now())
            .unwrap_err();
        assert!(
            matches!(err, rustls::Error::General(_)),
            "Actual error: {err:?}"
        );
    }

    #[test]
    fn invalid_server_name() {
        let mut rng = rand::thread_rng();
        let keypair = Ed25519KeyPair::generate(&mut rng);
        let public_key = keypair.public().to_owned();
        let cert = SelfSignedCertificate::new(keypair.private(), "not-sui");

        let allowlist = AllowPublicKeys::new(BTreeSet::from([public_key.clone()]));
        let client_verifier =
            ClientCertVerifier::new(allowlist.clone(), SUI_VALIDATOR_SERVER_NAME.to_string());

        // Allowed public key but the server-name in the cert is not the required "sui"
        let err = client_verifier
            .verify_client_cert(&cert.rustls_certificate(), &[], UnixTime::now())
            .unwrap_err();
        assert_eq!(
            err,
            rustls::Error::InvalidCertificate(rustls::CertificateError::NotValidForName),
            "Actual error: {err:?}"
        );

        let server_verifier =
            ServerCertVerifier::new(public_key, SUI_VALIDATOR_SERVER_NAME.to_string());

        // Allowed public key but the server-name in the cert is not the required "sui"
        let err = server_verifier
            .verify_server_cert(
                &cert.rustls_certificate(),
                &[],
                &ServerName::try_from("example.com").unwrap(),
                &[],
                UnixTime::now(),
            )
            .unwrap_err();
        assert_eq!(
            err,
            rustls::Error::InvalidCertificate(rustls::CertificateError::NotValidForName),
            "Actual error: {err:?}"
        );
    }

    #[tokio::test]
    async fn axum_acceptor() {
        use fastcrypto::ed25519::Ed25519KeyPair;
        use fastcrypto::traits::KeyPair;

        let mut rng = rand::thread_rng();
        let client_keypair = Ed25519KeyPair::generate(&mut rng);
        let client_public_key = client_keypair.public().to_owned();
        let client_certificate =
            SelfSignedCertificate::new(client_keypair.private(), SUI_VALIDATOR_SERVER_NAME);
        let server_keypair = Ed25519KeyPair::generate(&mut rng);
        let server_certificate = SelfSignedCertificate::new(server_keypair.private(), "localhost");

        let client = reqwest::Client::builder()
            .add_root_certificate(server_certificate.reqwest_certificate())
            .identity(client_certificate.reqwest_identity())
            .https_only(true)
            .build()
            .unwrap();

        let allowlist = AllowPublicKeys::new(BTreeSet::new());
        let tls_config =
            ClientCertVerifier::new(allowlist.clone(), SUI_VALIDATOR_SERVER_NAME.to_string())
                .rustls_server_config(
                    vec![server_certificate.rustls_certificate()],
                    server_certificate.rustls_private_key(),
                )
                .unwrap();

        async fn handler(tls_info: axum::Extension<TlsConnectionInfo>) -> String {
            tls_info.public_key().unwrap().to_string()
        }

        let app = axum::Router::new().route("/", axum::routing::get(handler));
        let listener = std::net::TcpListener::bind("localhost:0").unwrap();
        let server_address = listener.local_addr().unwrap();
        let acceptor = TlsAcceptor::new(tls_config);
        let _server = tokio::spawn(async move {
            axum_server::Server::from_tcp(listener)
                .acceptor(acceptor)
                .serve(app.into_make_service())
                .await
                .unwrap()
        });

        let server_url = format!("https://localhost:{}", server_address.port());
        // Client request is rejected because it isn't in the allowlist
        client.get(&server_url).send().await.unwrap_err();

        // Insert the client's public key into the allowlist and verify the request is successful
        allowlist.update(BTreeSet::from([client_public_key.clone()]));

        let res = client.get(&server_url).send().await.unwrap();
        let body = res.text().await.unwrap();
        assert_eq!(client_public_key.to_string(), body);
    }
}
