// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod acceptor;
mod certgen;
mod verifier;

pub const SUI_VALIDATOR_SERVER_NAME: &str = "sui";

pub use acceptor::{TlsAcceptor, TlsConnectionInfo};
pub use certgen::SelfSignedCertificate;
pub use verifier::{AllowAll, Allower, CertVerifier, HashSetAllow, ValidatorAllowlist};

pub use rustls;

#[cfg(test)]
mod tests {
    use super::*;
    use fastcrypto::ed25519::Ed25519KeyPair;
    use fastcrypto::traits::KeyPair;
    use rustls::server::ClientCertVerifier;

    #[test]
    fn verify_allowall() {
        let mut rng = rand::thread_rng();
        let allowed = Ed25519KeyPair::generate(&mut rng);
        let disallowed = Ed25519KeyPair::generate(&mut rng);
        let random_cert_bob =
            SelfSignedCertificate::new(allowed.private(), SUI_VALIDATOR_SERVER_NAME);
        let random_cert_alice =
            SelfSignedCertificate::new(disallowed.private(), SUI_VALIDATOR_SERVER_NAME);

        let verifier = CertVerifier::new(AllowAll);

        // The bob passes validation
        verifier
            .verify_client_cert(
                &random_cert_bob.rustls_certificate(),
                &[],
                std::time::SystemTime::now(),
            )
            .unwrap();

        // The alice passes validation
        verifier
            .verify_client_cert(
                &random_cert_alice.rustls_certificate(),
                &[],
                std::time::SystemTime::now(),
            )
            .unwrap();
    }

    #[test]
    fn verify_hashset() {
        let mut rng = rand::thread_rng();
        let allowed = Ed25519KeyPair::generate(&mut rng);
        let disallowed = Ed25519KeyPair::generate(&mut rng);

        let allowed_public_key = allowed.public().to_owned();
        let allowed_cert = SelfSignedCertificate::new(allowed.private(), SUI_VALIDATOR_SERVER_NAME);

        let disallowed_cert =
            SelfSignedCertificate::new(disallowed.private(), SUI_VALIDATOR_SERVER_NAME);

        let mut allowlist = HashSetAllow::new();
        let verifier = CertVerifier::new(allowlist.clone());

        // Add our public key to the allower
        allowlist
            .inner_mut()
            .write()
            .unwrap()
            .insert(allowed_public_key);

        // The allowed cert passes validation
        verifier
            .verify_client_cert(
                &allowed_cert.rustls_certificate(),
                &[],
                std::time::SystemTime::now(),
            )
            .unwrap();

        // The disallowed cert fails validation
        verifier
            .verify_client_cert(
                &disallowed_cert.rustls_certificate(),
                &[],
                std::time::SystemTime::now(),
            )
            .unwrap_err();

        // After removing the allowed public key from the set it now fails validation
        allowlist.inner_mut().write().unwrap().clear();
        verifier
            .verify_client_cert(
                &allowed_cert.rustls_certificate(),
                &[],
                std::time::SystemTime::now(),
            )
            .unwrap_err();
    }

    #[test]
    fn invalid_server_name() {
        let mut rng = rand::thread_rng();
        let keypair = Ed25519KeyPair::generate(&mut rng);
        let public_key = keypair.public().to_owned();
        let cert = SelfSignedCertificate::new(keypair.private(), "not-sui");

        let mut allowlist = HashSetAllow::new();
        let verifier = CertVerifier::new(allowlist.clone());

        // Add our public key to the allower
        allowlist.inner_mut().write().unwrap().insert(public_key);

        // Allowed public key but the server-name in the cert is not the required "sui"
        verifier
            .verify_client_cert(
                &cert.rustls_certificate(),
                &[],
                std::time::SystemTime::now(),
            )
            .unwrap_err();
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

        let mut allowlist = HashSetAllow::new();
        let tls_config = CertVerifier::new(allowlist.clone())
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
        allowlist
            .inner_mut()
            .write()
            .unwrap()
            .insert(client_public_key.clone());

        let res = client.get(&server_url).send().await.unwrap();
        let body = res.text().await.unwrap();
        assert_eq!(client_public_key.to_string(), body);
    }
}
