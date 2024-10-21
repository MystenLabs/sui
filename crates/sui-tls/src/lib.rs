// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod acceptor;
mod certgen;
mod verifier;

pub const SUI_VALIDATOR_SERVER_NAME: &str = "sui";

pub use acceptor::{TlsAcceptor, TlsConnectionInfo};
pub use certgen::SelfSignedCertificate;
pub use verifier::{
    public_key_from_certificate, AllowAll, AllowPublicKeys, Allower, ClientCertVerifier,
    ServerCertVerifier,
};

pub use rustls;

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
