// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use jsonrpsee::{
    http_server::{AccessControlBuilder, HttpServerBuilder, HttpServerHandle},
    RpcModule,
};
use jsonrpsee_core::{middleware::Middleware, server::rpc_module::Methods};
use prometheus_exporter::prometheus::{
    register_histogram_vec, register_int_counter_vec, HistogramVec, IntCounterVec,
};
use serde::Serialize;
use std::{env, net::SocketAddr, time::Instant};
use tracing::info;

pub struct JsonRpcServerBuilder {
    module: RpcModule<()>,
    server_builder: HttpServerBuilder<JsonRpcMetrics>,
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
        })
    }

    pub fn register_methods(&mut self, methods: impl Into<Methods>) -> Result<()> {
        self.module.merge(methods).map_err(Into::into)
    }

    pub fn register_open_rpc<T>(&mut self, spec: T) -> Result<()>
    where
        T: Clone + Serialize + Send + Sync + 'static,
    {
        self.module
            .register_method("rpc.discover", move |_, _| Ok(spec.clone()))?;
        Ok(())
    }

    pub async fn start(self, listen_address: SocketAddr) -> Result<HttpServerHandle> {
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
        Self {
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
        }
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
