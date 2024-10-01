// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
pub mod admin;
pub mod config;
pub mod consumer;
pub mod handlers;
pub mod histogram_relay;
pub mod metrics;
pub mod middleware;
pub mod peers;
pub mod prom_to_mimir;
pub mod remote_write;

/// var extracts environment variables at runtime with a default fallback value
/// if a default is not provided, the value is simply an empty string if not found
/// This function will return the provided default if env::var cannot find the key
/// or if the key is somehow malformed.
#[macro_export]
macro_rules! var {
    ($key:expr) => {
        match std::env::var($key) {
            Ok(val) => val,
            Err(_) => "".into(),
        }
    };
    ($key:expr, $default:expr) => {
        match std::env::var($key) {
            Ok(val) => val.parse::<_>().unwrap(),
            Err(_) => $default,
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::Labels;
    use crate::histogram_relay::HistogramRelay;
    use crate::prom_to_mimir::tests::*;

    use crate::{admin::CertKeyPair, config::RemoteWriteConfig, peers::SuiNodeProvider};
    use axum::http::StatusCode;
    use axum::routing::post;
    use axum::Router;
    use prometheus::Encoder;
    use prometheus::PROTOBUF_FORMAT;
    use protobuf::RepeatedField;
    use std::net::TcpListener;
    use std::time::Duration;
    use sui_tls::{ClientCertVerifier, TlsAcceptor};

    async fn run_dummy_remote_write(listener: TcpListener) {
        /// i accept everything, send me the trash
        async fn handler() -> StatusCode {
            StatusCode::OK
        }

        // build our application with a route
        let app = Router::new().route("/v1/push", post(handler));

        // run it
        listener.set_nonblocking(true).unwrap();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        axum::serve(listener, app).await.unwrap();
    }

    async fn run_dummy_remote_write_very_slow(listener: TcpListener) {
        /// i accept everything, send me the trash, but i will sleep and never return before a timeout
        /// this is for testing slow clients and this is the easiest way to do so without adding a special
        /// route in the server to do so
        async fn handler() -> StatusCode {
            // Simulate a route that hangs while waiting for a client to send data
            // but the server itself doesn't delay its processing
            tokio::time::sleep(Duration::from_secs(60)).await; // A very long sleep
            StatusCode::OK
        }

        // build our application with a route
        let app = Router::new().route("/v1/push", post(handler));

        // run it
        listener.set_nonblocking(true).unwrap();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        axum::serve(listener, app).await.unwrap();
    }

    /// test_axum_acceptor is a basic e2e test that creates a mock remote_write post endpoint and has a simple
    /// sui-node client that posts data to the proxy using the protobuf format.  The server processes this
    /// data and sends it to the mock remote_write which accepts everything.  Future work is to make this more
    /// robust and expand the scope of coverage, probabaly moving this test elsewhere and renaming it.
    #[tokio::test]
    async fn test_axum_acceptor() {
        // generate self-signed certificates
        let CertKeyPair(client_priv_cert, client_pub_key) = admin::generate_self_cert("sui".into());
        let CertKeyPair(server_priv_cert, _) = admin::generate_self_cert("localhost".into());

        // create a fake rpc server
        let dummy_remote_write_listener = std::net::TcpListener::bind("localhost:0").unwrap();
        let dummy_remote_write_address = dummy_remote_write_listener.local_addr().unwrap();
        let dummy_remote_write_url = format!(
            "http://localhost:{}/v1/push",
            dummy_remote_write_address.port()
        );

        let _dummy_remote_write =
            tokio::spawn(async move { run_dummy_remote_write(dummy_remote_write_listener).await });

        // init the tls config and allower
        let mut allower = SuiNodeProvider::new("".into(), Duration::from_secs(30), vec![]);
        let tls_config = ClientCertVerifier::new(
            allower.clone(),
            sui_tls::SUI_VALIDATOR_SERVER_NAME.to_string(),
        )
        .rustls_server_config(
            vec![server_priv_cert.rustls_certificate()],
            server_priv_cert.rustls_private_key(),
        )
        .unwrap();

        let client = admin::make_reqwest_client(
            RemoteWriteConfig {
                url: dummy_remote_write_url.to_owned(),
                username: "bar".into(),
                password: "foo".into(),
                ..Default::default()
            },
            "dummy user agent",
        );

        let app = admin::app(
            Labels {
                network: "unittest-network".into(),
                inventory_hostname: "ansible_inventory_name".into(),
            },
            client,
            HistogramRelay::new(),
            Some(allower.clone()),
        );

        let listener = std::net::TcpListener::bind("localhost:0").unwrap();
        let server_address = listener.local_addr().unwrap();
        let server_url = format!(
            "https://localhost:{}/publish/metrics",
            server_address.port()
        );

        let acceptor = TlsAcceptor::new(tls_config);
        let _server = tokio::spawn(async move {
            admin::server(listener, app, Some(acceptor)).await.unwrap();
        });

        // build a client
        let client = reqwest::Client::builder()
            .add_root_certificate(server_priv_cert.reqwest_certificate())
            .identity(client_priv_cert.reqwest_identity())
            .https_only(true)
            .build()
            .unwrap();

        // Client request is rejected because it isn't in the allowlist
        client.get(&server_url).send().await.unwrap_err();

        // Insert the client's public key into the allowlist and verify the request is successful
        allower.get_sui_mut().write().unwrap().insert(
            client_pub_key.to_owned(),
            peers::AllowedPeer {
                name: "some-node".into(),
                public_key: client_pub_key.to_owned(),
            },
        );

        let mf = create_metric_family(
            "foo_metric",
            "some help this is",
            None,
            RepeatedField::from_vec(vec![create_metric_counter(
                RepeatedField::from_vec(create_labels(vec![("some", "label")])),
                create_counter(2046.0),
            )]),
        );

        let mut buf = vec![];
        let encoder = prometheus::ProtobufEncoder::new();
        encoder.encode(&[mf], &mut buf).unwrap();

        let res = client
            .post(&server_url)
            .header(reqwest::header::CONTENT_TYPE, PROTOBUF_FORMAT)
            .body(buf)
            .send()
            .await
            .expect("expected a successful post with a self-signed certificate");
        let status = res.status();
        let body = res.text().await.unwrap();
        assert_eq!("created", body);
        assert_eq!(status, reqwest::StatusCode::CREATED);
    }

    /// this is a long test to ensure we are timing out clients that are slow  
    #[tokio::test]
    async fn test_client_timeout() {
        // generate self-signed certificates
        let CertKeyPair(client_priv_cert, client_pub_key) = admin::generate_self_cert("sui".into());
        let CertKeyPair(server_priv_cert, _) = admin::generate_self_cert("localhost".into());

        // create a fake rpc server
        let dummy_remote_write_listener = std::net::TcpListener::bind("localhost:0").unwrap();
        let dummy_remote_write_address = dummy_remote_write_listener.local_addr().unwrap();
        let dummy_remote_write_url = format!(
            "http://localhost:{}/v1/push",
            dummy_remote_write_address.port()
        );

        let _dummy_remote_write = tokio::spawn(async move {
            run_dummy_remote_write_very_slow(dummy_remote_write_listener).await
        });

        // init the tls config and allower
        let mut allower = SuiNodeProvider::new("".into(), Duration::from_secs(30), vec![]);
        let tls_config = ClientCertVerifier::new(
            allower.clone(),
            sui_tls::SUI_VALIDATOR_SERVER_NAME.to_string(),
        )
        .rustls_server_config(
            vec![server_priv_cert.rustls_certificate()],
            server_priv_cert.rustls_private_key(),
        )
        .unwrap();

        let client = admin::make_reqwest_client(
            RemoteWriteConfig {
                url: dummy_remote_write_url.to_owned(),
                username: "bar".into(),
                password: "foo".into(),
                ..Default::default()
            },
            "dummy user agent",
        );

        // this will affect other tests if they are run in parallel, but we only have two tests, so it shouldn't be an issue (yet)
        // even still, the other tests complete very fast so those tests would also need to slow down by orders and orders to be
        // bothered by this env var
        std::env::set_var("NODE_CLIENT_TIMEOUT", "5");

        let app = admin::app(
            Labels {
                network: "unittest-network".into(),
                inventory_hostname: "ansible_inventory_name".into(),
            },
            client,
            HistogramRelay::new(),
            Some(allower.clone()),
        );

        let listener = std::net::TcpListener::bind("localhost:0").unwrap();
        let server_address = listener.local_addr().unwrap();
        let server_url = format!(
            "https://localhost:{}/publish/metrics",
            server_address.port()
        );

        let acceptor = TlsAcceptor::new(tls_config);
        let _server = tokio::spawn(async move {
            admin::server(listener, app, Some(acceptor)).await.unwrap();
        });

        // build a client
        let client = reqwest::Client::builder()
            .add_root_certificate(server_priv_cert.reqwest_certificate())
            .identity(client_priv_cert.reqwest_identity())
            .https_only(true)
            .build()
            .unwrap();

        // Client request is rejected because it isn't in the allowlist
        client.get(&server_url).send().await.unwrap_err();

        // Insert the client's public key into the allowlist and verify the request is successful
        allower.get_sui_mut().write().unwrap().insert(
            client_pub_key.to_owned(),
            peers::AllowedPeer {
                name: "some-node".into(),
                public_key: client_pub_key.to_owned(),
            },
        );

        let mf = create_metric_family(
            "foo_metric",
            "some help this is",
            None,
            RepeatedField::from_vec(vec![create_metric_counter(
                RepeatedField::from_vec(create_labels(vec![("some", "label")])),
                create_counter(2046.0),
            )]),
        );

        let mut buf = vec![];
        let encoder = prometheus::ProtobufEncoder::new();
        encoder.encode(&[mf], &mut buf).unwrap();

        let res = client
            .post(&server_url)
            .header(reqwest::header::CONTENT_TYPE, PROTOBUF_FORMAT)
            .body(buf)
            .send()
            .await
            .expect("expected a successful post with a self-signed certificate");
        let status = res.status();
        assert_eq!(status, StatusCode::REQUEST_TIMEOUT);
        // Clean up the environment variable
        std::env::remove_var("NODE_CLIENT_TIMEOUT");
    }
}
