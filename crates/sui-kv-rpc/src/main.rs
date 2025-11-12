// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use axum::Router;
use axum::routing::get;
use clap::Parser;
use mysten_network::callback::CallbackLayer;
use prometheus::Registry;
use std::sync::Arc;
use sui_kv_rpc::KvRpcServer;
use sui_rpc_api::{RpcMetrics, RpcMetricsMakeCallbackHandler, ServerVersion};
use telemetry_subscribers::TelemetryConfig;
use tonic::transport::{Identity, Server, ServerTlsConfig};

bin_version::bin_version!();

#[derive(Parser)]
struct App {
    credentials: String,
    instance_id: String,
    #[clap(default_value = "[::1]:8000")]
    address: String,
    #[clap(default_value = "127.0.0.1")]
    metrics_host: String,
    #[clap(default_value_t = 9184)]
    metrics_port: usize,
    #[clap(long = "tls-cert", default_value = "")]
    tls_cert: String,
    #[clap(long = "tls-key", default_value = "")]
    tls_key: String,
    #[clap(long = "app-profile-id")]
    app_profile_id: Option<String>,
    #[clap(long = "checkpoint-bucket")]
    checkpoint_bucket: Option<String>,
}

async fn health_check() -> &'static str {
    "OK"
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = TelemetryConfig::new().with_env().init();
    let app = App::parse();
    unsafe {
        std::env::set_var("GOOGLE_APPLICATION_CREDENTIALS", app.credentials.clone());
    };
    let server_version = Some(ServerVersion::new("sui-kv-rpc", VERSION));
    let registry_service = mysten_metrics::start_prometheus_server(
        format!("{}:{}", app.metrics_host, app.metrics_port).parse()?,
    );
    let registry: Registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&registry);
    let server = KvRpcServer::new(
        app.instance_id,
        app.app_profile_id,
        app.checkpoint_bucket,
        server_version,
        &registry,
    )
    .await?;
    let addr = app.address.parse()?;
    let mut builder = Server::builder();
    if !app.tls_cert.is_empty() && !app.tls_key.is_empty() {
        let identity =
            Identity::from_pem(std::fs::read(app.tls_cert)?, std::fs::read(app.tls_key)?);
        let tls_config = ServerTlsConfig::new().identity(identity);
        builder = builder.tls_config(tls_config)?;
    }
    let reflection_v1 = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(
            sui_rpc_api::proto::google::protobuf::FILE_DESCRIPTOR_SET,
        )
        .register_encoded_file_descriptor_set(sui_rpc_api::proto::google::rpc::FILE_DESCRIPTOR_SET)
        .register_encoded_file_descriptor_set(sui_rpc::proto::sui::rpc::v2::FILE_DESCRIPTOR_SET)
        .build_v1()?;
    let reflection_v1alpha = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(
            sui_rpc_api::proto::google::protobuf::FILE_DESCRIPTOR_SET,
        )
        .register_encoded_file_descriptor_set(sui_rpc_api::proto::google::rpc::FILE_DESCRIPTOR_SET)
        .register_encoded_file_descriptor_set(sui_rpc::proto::sui::rpc::v2::FILE_DESCRIPTOR_SET)
        .build_v1alpha()?;
    tokio::spawn(async {
        let web_server = Router::new().route("/health", get(health_check));
        let listener = tokio::net::TcpListener::bind("0.0.0.0:8081")
            .await
            .expect("can't bind to the healthcheck port");
        axum::serve(listener, web_server.into_make_service())
            .await
            .expect("healh check service failed");
    });
    builder
        .layer(CallbackLayer::new(RpcMetricsMakeCallbackHandler::new(
            Arc::new(RpcMetrics::new(&registry)),
        )))
        .add_service(
            sui_rpc::proto::sui::rpc::v2::ledger_service_server::LedgerServiceServer::new(server),
        )
        .add_service(reflection_v1)
        .add_service(reflection_v1alpha)
        .serve(addr)
        .await?;
    Ok(())
}
