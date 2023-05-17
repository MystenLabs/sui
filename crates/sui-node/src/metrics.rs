// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use axum::{
    extract::Extension,
    http::{header, StatusCode},
    routing::get,
    Router,
};

use mysten_network::metrics::MetricsCallbackProvider;
use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry, Encoder, HistogramVec, IntCounterVec, IntGaugeVec,
    Registry, TextEncoder, PROTOBUF_FORMAT,
};

use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sui_network::tonic::Code;

use mysten_metrics::RegistryService;
use tracing::{error, warn};

const METRICS_ROUTE: &str = "/metrics";

// Creates a new http server that has as a sole purpose to expose
// and endpoint that prometheus agent can use to poll for the metrics.
// A RegistryService is returned that can be used to get access in prometheus Registries.
pub fn start_prometheus_server(addr: SocketAddr) -> RegistryService {
    let registry = Registry::new();

    let registry_service = RegistryService::new(registry);

    if cfg!(msim) {
        // prometheus uses difficult-to-support features such as TcpSocket::from_raw_fd(), so we
        // can't yet run it in the simulator.
        warn!("not starting prometheus server in simulator");
        return registry_service;
    }

    let app = Router::new()
        .route(METRICS_ROUTE, get(metrics))
        .layer(Extension(registry_service.clone()));

    tokio::spawn(async move {
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
            .unwrap();
    });

    registry_service
}

async fn metrics(Extension(registry_service): Extension<RegistryService>) -> (StatusCode, String) {
    let metrics_families = registry_service.gather_all();
    match TextEncoder.encode_to_string(&metrics_families) {
        Ok(metrics) => (StatusCode::OK, metrics),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("unable to encode metrics: {error}"),
        ),
    }
}

pub struct MetricsPushClient {
    certificate: std::sync::Arc<sui_tls::SelfSignedCertificate>,
    client: reqwest::Client,
}

impl MetricsPushClient {
    pub fn new(network_key: sui_types::crypto::NetworkKeyPair) -> Self {
        use fastcrypto::traits::KeyPair;
        let certificate = std::sync::Arc::new(sui_tls::SelfSignedCertificate::new(
            network_key.private(),
            sui_tls::SUI_VALIDATOR_SERVER_NAME,
        ));
        let identity = certificate.reqwest_identity();
        let client = reqwest::Client::builder()
            .identity(identity)
            .build()
            .unwrap();

        Self {
            certificate,
            client,
        }
    }

    pub fn certificate(&self) -> &sui_tls::SelfSignedCertificate {
        &self.certificate
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }
}

/// Starts a task to periodically push metrics to a configured endpoint if a metrics push endpoint
/// is configured.
pub fn start_metrics_push_task(config: &sui_config::NodeConfig, registry: RegistryService) {
    use fastcrypto::traits::KeyPair;
    use sui_config::node::MetricsConfig;

    const DEFAULT_METRICS_PUSH_INTERVAL: Duration = Duration::from_secs(60);

    let (interval, url) = match &config.metrics {
        Some(MetricsConfig {
            push_interval_seconds,
            push_url: Some(url),
        }) => {
            let interval = push_interval_seconds
                .map(Duration::from_secs)
                .unwrap_or(DEFAULT_METRICS_PUSH_INTERVAL);
            let url = reqwest::Url::parse(url).expect("unable to parse metrics push url");
            (interval, url)
        }
        _ => return,
    };

    let client = MetricsPushClient::new(config.network_key_pair().copy());

    async fn push_metrics(
        client: &MetricsPushClient,
        url: &reqwest::Url,
        registry: &RegistryService,
    ) -> Result<(), anyhow::Error> {
        // now represents a collection timestamp for all of the metrics we send to the proxy
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let mut metric_families = registry.gather_all();
        for mf in metric_families.iter_mut() {
            for m in mf.mut_metric() {
                m.set_timestamp_ms(now);
            }
        }

        let mut buf: Vec<u8> = vec![];
        let encoder = prometheus::ProtobufEncoder::new();
        encoder.encode(&metric_families, &mut buf)?;

        let mut s = snap::raw::Encoder::new();
        let compressed = s.compress_vec(&buf).map_err(|err| {
            error!("unable to snappy encode; {err}");
            err
        })?;

        let response = client
            .client()
            .post(url.to_owned())
            .header(reqwest::header::CONTENT_ENCODING, "snappy")
            .header(header::CONTENT_TYPE, PROTOBUF_FORMAT)
            .body(compressed)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = match response.text().await {
                Ok(body) => body,
                Err(error) => format!("couldn't decode response body; {error}"),
            };
            return Err(anyhow::anyhow!(
                "metrics push failed: [{}]:{}",
                status,
                body
            ));
        }

        tracing::debug!("successfully pushed metrics to {url}");

        Ok(())
    }

    tokio::spawn(async move {
        tracing::info!(push_url =% url, interval =? interval, "Started Metrics Push Service");

        let mut interval = tokio::time::interval(interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            if let Err(error) = push_metrics(&client, &url, &registry).await {
                tracing::warn!("unable to push metrics: {error}");
            }
        }
    });
}

#[derive(Clone)]
pub struct GrpcMetrics {
    inflight_grpc: IntGaugeVec,
    grpc_requests: IntCounterVec,
    grpc_request_latency: HistogramVec,
}

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1., 2.5, 5., 10., 20., 30., 60., 90.,
];

impl GrpcMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            inflight_grpc: register_int_gauge_vec_with_registry!(
                "inflight_grpc",
                "Total in-flight GRPC requests per route",
                &["path"],
                registry,
            )
            .unwrap(),
            grpc_requests: register_int_counter_vec_with_registry!(
                "grpc_requests",
                "Total GRPC requests per route",
                &["path", "status"],
                registry,
            )
            .unwrap(),
            grpc_request_latency: register_histogram_vec_with_registry!(
                "grpc_request_latency",
                "Latency of GRPC requests per route",
                &["path"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
        }
    }
}

impl MetricsCallbackProvider for GrpcMetrics {
    fn on_request(&self, _path: String) {}

    fn on_response(&self, path: String, latency: Duration, _status: u16, grpc_status_code: Code) {
        self.grpc_requests
            .with_label_values(&[path.as_str(), format!("{grpc_status_code:?}").as_str()])
            .inc();
        self.grpc_request_latency
            .with_label_values(&[path.as_str()])
            .observe(latency.as_secs_f64());
    }

    fn on_start(&self, path: &str) {
        self.inflight_grpc.with_label_values(&[path]).inc();
    }

    fn on_drop(&self, path: &str) {
        self.inflight_grpc.with_label_values(&[path]).dec();
    }
}

#[cfg(test)]
mod tests {
    use crate::metrics::start_prometheus_server;
    use prometheus::{IntCounter, Registry};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[tokio::test]
    pub async fn test_metrics_endpoint_with_multiple_registries_add_remove() {
        let port: u16 = 8081;
        let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

        let registry_service = start_prometheus_server(socket);

        tokio::task::yield_now().await;

        // now add a few registries to the service along side with metrics
        let registry_1 = Registry::new_custom(Some("narwhal".to_string()), None).unwrap();
        let counter_1 = IntCounter::new("counter_1", "a sample counter 1").unwrap();
        registry_1.register(Box::new(counter_1)).unwrap();

        let registry_2 = Registry::new_custom(Some("sui".to_string()), None).unwrap();
        let counter_2 = IntCounter::new("counter_2", "a sample counter 2").unwrap();
        registry_2.register(Box::new(counter_2.clone())).unwrap();

        let registry_1_id = registry_service.add(registry_1);
        let _registry_2_id = registry_service.add(registry_2);

        // request the endpoint
        let result = get_metrics(port).await;

        assert!(result.contains(
            "# HELP sui_counter_2 a sample counter 2
# TYPE sui_counter_2 counter
sui_counter_2 0"
        ));

        assert!(result.contains(
            "# HELP narwhal_counter_1 a sample counter 1
# TYPE narwhal_counter_1 counter
narwhal_counter_1 0"
        ));

        // Now remove registry 1
        assert!(registry_service.remove(registry_1_id));

        // AND increase metric 2
        counter_2.inc();

        // Now pull again metrics
        // request the endpoint
        let result = get_metrics(port).await;

        // Registry 1 metrics should not be present anymore
        assert!(!result.contains(
            "# HELP narwhal_counter_1 a sample counter 1
# TYPE narwhal_counter_1 counter
narwhal_counter_1 0"
        ));

        // Registry 2 metric should have increased by 1
        assert!(result.contains(
            "# HELP sui_counter_2 a sample counter 2
# TYPE sui_counter_2 counter
sui_counter_2 1"
        ));
    }

    async fn get_metrics(port: u16) -> String {
        let client = reqwest::Client::new();
        let response = client
            .get(format!("http://127.0.0.1:{}/metrics", port))
            .send()
            .await
            .unwrap();
        response.text().await.unwrap()
    }
}
