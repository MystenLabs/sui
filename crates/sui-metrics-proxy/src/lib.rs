// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
pub mod admin;
pub mod channels;
pub mod handlers;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{self, Request, StatusCode},
    };
    use serde_json::json;
    use tower::ServiceExt; // for `oneshot`

    #[tokio::test]
    async fn post_json() {
        let app = admin::app(10, "unittest-network".to_string());

        let response = app
            .oneshot(
                Request::builder()
                    .method(http::Method::POST)
                    .uri("/publish/metrics")
                    .header(
                        http::header::CONTENT_TYPE,
                        "application/mysten.proxy.promexposition",
                    )
                    .body(Body::from(
                        serde_json::to_vec(&json!({"foo":"fooman"})).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        assert_eq!(body, String::from("accepted").into_bytes());
    }

    // TODO enable when tls is re-enabled
    // #[tokio::test]
    // async fn axum_acceptor() {
    //     use fastcrypto::ed25519::Ed25519KeyPair;
    //     use fastcrypto::traits::KeyPair;

    //     use sui_tls::{
    //         SelfSignedCertificate, TlsAcceptor, TlsConnectionInfo, ValidatorCertVerifier,
    //     };

    //     let mut rng = rand::thread_rng();
    //     let client_keypair = Ed25519KeyPair::generate(&mut rng);
    //     let client_public_key = client_keypair.public().to_owned();
    //     // server_name param must be "sui"
    //     let client_certificate = SelfSignedCertificate::new(client_keypair.private(), "sui");
    //     let server_keypair = Ed25519KeyPair::generate(&mut rng);
    //     let server_certificate = SelfSignedCertificate::new(server_keypair.private(), "localhost");

    //     let client = reqwest::Client::builder()
    //         .add_root_certificate(server_certificate.reqwest_certificate())
    //         .identity(client_certificate.reqwest_identity())
    //         .https_only(true)
    //         .build()
    //         .unwrap();

    //     let (tls_config, allowlist) = ValidatorCertVerifier::rustls_server_config(
    //         vec![server_certificate.rustls_certificate()],
    //         server_certificate.rustls_private_key(),
    //     )
    //     .unwrap();

    //     async fn handler(tls_info: axum::Extension<TlsConnectionInfo>) -> String {
    //         tls_info.public_key().unwrap().to_string()
    //     }

    //     let app = admin::app(10, "unittest-network".to_string());
    //     let listener = std::net::TcpListener::bind("localhost:0").unwrap();
    //     let server_address = listener.local_addr().unwrap();
    //     let acceptor = TlsAcceptor::new(tls_config);
    //     let _server = tokio::spawn(async move {
    //         admin::server(listener, acceptor, app).await.unwrap();
    //     });
    //     let server_url = format!(
    //         "https://localhost:{}/publish/metrics",
    //         server_address.port()
    //     );
    //     // Client request is rejected because it isn't in the allowlist
    //     client.get(&server_url).send().await.unwrap_err();

    //     // Insert the client's public key into the allowlist and verify the request is successful
    //     allowlist.write().unwrap().insert(client_public_key.clone());

    //     let res = client
    //         .post(&server_url)
    //         .body("{\"some\":\"data\"}")
    //         .send()
    //         .await
    //         .unwrap();
    //     let body = res.text().await.unwrap();
    //     assert_eq!("accepted", body);
    // }
}
