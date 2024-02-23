use mysten_metrics::RegistryService;
use sui_config::node::HealthCheckConfig;

use axum::routing::get;
use axum::{http::StatusCode, Extension, Json, Router};
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Notify;
use tracing::{error, info};

#[derive(Clone)]
enum HealthStatus {
    HEALTHY,
    UNHEALTHY,
}

#[derive(Clone)]
enum ReadyStatus {
    READY,
    NOTREADY,
}

#[derive(Clone)]
pub struct HealthService {
    metric_registry_service: RegistryService,
    health_status: Arc<RwLock<HealthStatus>>,
    ready_status: Arc<RwLock<ReadyStatus>>,
}

impl HealthService {
    pub fn new(metric_registry_service: RegistryService) -> HealthService {
        HealthService {
            metric_registry_service,
            health_status: Arc::new(RwLock::new(HealthStatus::UNHEALTHY)),
            ready_status: Arc::new(RwLock::new(ReadyStatus::NOTREADY)),
        }
    }

    pub async fn monitor_and_update_status(&self, notify: Arc<Notify>) {
        // read from channel for metrics push tick, need to fallback to other timer if metrics
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            tokio::select! {
                // prefer notify channel on selects
                biased;

                _ = notify.notified() => {
                    self.set_ready_status();
                },
                _ = interval.tick() => {
                    info!("interval tick in health check - checking checkpoint metrics");
                    self.set_ready_status();
                }
            }
        }
    }

    fn set_ready_status(&self) {
        let max_lag: u64 = 300;
        let metrics_families = self.metric_registry_service.gather_all();
        // Iterate over metric families to find our counter
        for family in metrics_families {
            // also need last_executed_checkpoint_timestamp
            if family.get_name() == "last_executed_checkpoint_timestamp_ms" {
                // Found our metric family, now extract metric data
                for metric in family.get_metric() {
                    let last_executed_checkpoint_ts_ms = metric.get_gauge().get_value() as u64;
                    if is_delay_greater_than_max_lag(last_executed_checkpoint_ts_ms, max_lag) {
                        info!("server not ready due to last executed checkpoint lag");
                        let mut ready_status = self.ready_status.write().unwrap();
                        *ready_status = ReadyStatus::NOTREADY;
                        return;
                    } else {
                        let mut ready_status = self.ready_status.write().unwrap();
                        info!("server ready");
                        *ready_status = ReadyStatus::READY;
                    };
                }
            }
        }
    }
}

async fn health_check_handler(
    Extension(health_service): Extension<HealthService>,
) -> (StatusCode, Json<serde_json::Value>) {
    match health_service.health_status.read() {
        Ok(health_status) => match *health_status {
            HealthStatus::HEALTHY => (StatusCode::OK, Json(json!({"status": "ok"}))),
            HealthStatus::UNHEALTHY => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"status": "unhealthy"})),
            ),
        },
        Err(e) => {
            error!("error when reading health status {}", e);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"status": "unknown"})),
            )
        }
    }
}

async fn ready_check_handler(
    Extension(health_service): Extension<HealthService>,
) -> (StatusCode, Json<serde_json::Value>) {
    match health_service.ready_status.read() {
        Ok(ready_status) => match *ready_status {
            ReadyStatus::READY => (StatusCode::OK, Json(json!({"status": "ready"}))),
            ReadyStatus::NOTREADY => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"status": "not ready"})),
            ),
        },
        Err(e) => {
            error!("error when reading ready status {}", e);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"status": "unknown"})),
            )
        }
    }
}

pub async fn start_health_checks(
    registry_service: RegistryService,
    notify: Arc<Notify>,
    config: Option<HealthCheckConfig>,
) {
    // start basic health check endpoints
    let mut health_service = HealthService::new(registry_service);

    //tokio::spawn(async move {
    let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8000);
    let app = Router::new()
        .route("/health", get(health_check_handler))
        .route("/ready", get(ready_check_handler))
        .layer(Extension(health_service.clone()));

    tokio::spawn(async move {
        health_service.monitor_and_update_status(notify).await;
    });

    axum::Server::bind(&socket)
        .serve(app.into_make_service())
        .await
        .unwrap()
}

fn is_delay_greater_than_max_lag(unix_timestamp: u64, max_lag: u64) -> bool {
    let timestamp_time = UNIX_EPOCH + Duration::from_secs(unix_timestamp);
    match SystemTime::now().duration_since(timestamp_time) {
        Ok(duration_since_timestamp) => {
            // Compare the duration
            duration_since_timestamp > Duration::from_secs(max_lag)
        }
        Err(_) => {
            // SystemTime::now() is earlier than the unix timestamp
            // This could happen if the system clock is changed
            false
        }
    }
}
