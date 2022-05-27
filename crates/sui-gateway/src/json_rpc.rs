// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::SuiRpcModule;
use anyhow::Result;
use jsonrpsee::{
    http_server::{AccessControlBuilder, HttpServerBuilder, HttpServerHandle},
    RpcModule,
};
use jsonrpsee_core::middleware::Middleware;
use once_cell::sync::Lazy;
use prometheus_exporter::prometheus::{
    register_histogram_vec, register_int_counter_vec, HistogramVec, IntCounterVec,
};
use std::{env, net::SocketAddr, time::Instant};
use sui_open_rpc::Project;
use tracing::info;

pub struct JsonRpcServerBuilder {
    module: RpcModule<()>,
    server_builder: HttpServerBuilder<JsonRpcMetrics>,
    rpc_doc: Project,
}

pub fn sui_rpc_doc() -> Project {
    Project::new(
        "Sui JSON-RPC",
        "Sui JSON-RPC API for interaction with the Sui network gateway.",
        "Mysten Labs",
        "https://mystenlabs.com",
        "build@mystenlabs.com",
        "Apache-2.0",
        "https://raw.githubusercontent.com/MystenLabs/sui/main/LICENSE",
    )
}

impl JsonRpcServerBuilder {
    pub fn new() -> Result<Self> {
        let mut ac_builder = AccessControlBuilder::default();

        if let Ok(value) = env::var("ACCESS_CONTROL_ALLOW_ORIGIN") {
            let list = value.split(',').collect::<Vec<_>>();
            info!("Setting ACCESS_CONTROL_ALLOW_ORIGIN to : {:?}", list);
            ac_builder = ac_builder.set_allowed_origins(list)?;
        }

        let acl = ac_builder.build();
        info!(?acl);

        let server_builder = HttpServerBuilder::default()
            .set_access_control(acl)
            .set_middleware(JsonRpcMetrics::new());

        let module = RpcModule::new(());

        Ok(Self {
            module,
            server_builder,
            rpc_doc: sui_rpc_doc(),
        })
    }

    pub fn register_module<T: SuiRpcModule>(&mut self, module: T) -> Result<()> {
        self.rpc_doc.add_module(T::rpc_doc_module());
        self.module.merge(module.rpc()).map_err(Into::into)
    }

    pub async fn start(mut self, listen_address: SocketAddr) -> Result<HttpServerHandle> {
        self.module
            .register_method("rpc.discover", move |_, _| Ok(self.rpc_doc.clone()))?;

        let server = self.server_builder.build(listen_address).await?;

        let addr = server.local_addr()?;
        info!(local_addr =? addr, "Sui JSON-RPC server listening on {addr}");
        info!(
            "Available JSON-RPC methods : {:?}",
            self.module.method_names().collect::<Vec<_>>()
        );

        server.start(self.module).map_err(Into::into)
    }
}

#[derive(Clone)]
struct JsonRpcMetrics {
    /// Counter of requests, route is a label (ie separate timeseries per route)
    requests_by_route: IntCounterVec,
    /// Request latency, route is a label
    req_latency_by_route: HistogramVec,
    /// Failed requests by route
    errors_by_route: IntCounterVec,
}

impl JsonRpcMetrics {
    pub fn new() -> Self {
        static METRICS: Lazy<JsonRpcMetrics> = Lazy::new(|| JsonRpcMetrics {
            requests_by_route: register_int_counter_vec!(
                "rpc_requests_by_route",
                "Number of requests by route",
                &["route"]
            )
            .unwrap(),
            req_latency_by_route: register_histogram_vec!(
                "req_latency_by_route",
                "Latency of a request by route",
                &["route"]
            )
            .unwrap(),
            errors_by_route: register_int_counter_vec!(
                "errors_by_route",
                "Number of errors by route",
                &["route"]
            )
            .unwrap(),
        });

        Lazy::force(&METRICS).clone()
    }
}

impl Middleware for JsonRpcMetrics {
    type Instant = Instant;

    fn on_request(&self) -> Instant {
        Instant::now()
    }

    fn on_result(&self, name: &str, success: bool, started_at: Instant) {
        self.requests_by_route.with_label_values(&[name]).inc();
        let req_latency_secs = (Instant::now() - started_at).as_secs_f64();
        self.req_latency_by_route
            .with_label_values(&[name])
            .observe(req_latency_secs);
        if !success {
            self.errors_by_route.with_label_values(&[name]).inc();
        }
    }
}
