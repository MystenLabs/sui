use axum::{
    body::Body,
    extract::{Request, State},
    http::request::Parts,
    http::StatusCode,
    response::Response,
    routing::any,
    Router,
};
use bytes::Bytes;
use clap::Parser;
use mysten_metrics::start_prometheus_server;
use std::net::SocketAddr;
use sui_edge_proxy::config::{load, PeerConfig, ProxyConfig};
use sui_edge_proxy::metrics::AppMetrics;
use tracing::{info, warn};
use url::Url;

#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
struct Args {
    #[clap(
        long,
        short,
        default_value = "./sui-edge-proxy.yaml",
        help = "Specify the config file path to use"
    )]
    config: String,
}

#[derive(Clone)]
struct AppState {
    client: reqwest::Client,
    read_peer: PeerConfig,
    execution_peer: PeerConfig,
    metrics: AppMetrics,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let config: ProxyConfig = load(&args.config).unwrap();

    // Init metrics server
    let registry_service = start_prometheus_server(config.metrics_address);
    let prometheus_registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&prometheus_registry);
    // Init logging
    let (_guard, _filter_handle) = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .with_prom_registry(&prometheus_registry)
        .init();

    info!("Metrics server started at {}", config.metrics_address);

    // Build a reqwest client that supports HTTP/2
    let client = reqwest::ClientBuilder::new()
        .http2_prior_knowledge()
        .build()
        .unwrap();

    let app_metrics = AppMetrics::new(&prometheus_registry);

    let app_state = AppState {
        client,
        read_peer: config.read_peer.clone(),
        execution_peer: config.execution_peer.clone(),
        metrics: app_metrics,
    };

    let app = Router::new()
        .fallback(any(proxy_handler))
        .with_state(app_state);

    let addr: SocketAddr = config.listen_address.parse().unwrap();
    info!("Starting server on {}", addr);
    axum_server::Server::bind(addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn proxy_handler(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<Response, (StatusCode, String)> {
    info!(
        "Entered proxy_handler function for path: {}",
        request.uri().path()
    );
    let (parts, body) = request.into_parts();
    // check that content type is json
    if parts.headers.get("content-type")
        != Some(&reqwest::header::HeaderValue::from_static(
            "application/json",
        ))
    {
        info!("Content type is not application/json");
        return Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("Content type must be application/json"))
            .unwrap());
    }
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => {
            info!("Request body size: {} bytes", bytes.len());
            bytes
        }
        Err(e) => {
            warn!("Failed to read request body: {}", e);
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Failed to read request body"))
                .unwrap());
        }
    };

    match parts
        .headers
        // Sui-Method-Name will be added later on
        // .get("Sui-Method-Name")
        .get("Sui-Transaction-Type")
        .and_then(|h| h.to_str().ok())
    {
        Some("execute") => {
            info!("Using execution peer");
            // no need to check the request body, skip right to proxying to execution peer
            proxy_request(state, parts, body_bytes, true).await
        }
        _ => {
            let json_body = match serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                Ok(json_body) => json_body,
                Err(_) => {
                    return Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::from("Request body is not valid JSON"))
                        .unwrap());
                }
            };
            if let Some("sui_executeTransactionBlock") =
                json_body.get("method").and_then(|m| m.as_str())
            {
                proxy_request(state, parts, body_bytes, true).await
            } else {
                proxy_request(state, parts, body_bytes, false).await
            }
        }
    }
}

async fn proxy_request(
    state: AppState,
    parts: Parts,
    body_bytes: Bytes,
    use_execution_peer: bool,
) -> Result<Response, (StatusCode, String)> {
    let peer_type = if use_execution_peer {
        "execution"
    } else {
        "read"
    };

    let timer_histogram = state
        .metrics
        .request_latency
        .with_label_values(&[peer_type]);
    let _timer = timer_histogram.start_timer();

    let peer_config = if use_execution_peer {
        &state.execution_peer
    } else {
        &state.read_peer
    };
    // Construct the base URL
    let target_url = Url::parse(peer_config.address.clone().as_str()).map_err(|e| {
        warn!("Failed to parse base URL: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to construct target URL".to_string(),
        )
    })?;
    info!("Target URL: {}", target_url);
    // copy headers from incoming request to client request
    let mut headers = parts.headers.clone();

    headers.insert(
        "host",
        reqwest::header::HeaderValue::from_str(&peer_config.sni.clone()).unwrap(),
    );

    let request_builder = state
        .client
        .request(parts.method.clone(), target_url.clone())
        .header("content-type", "application/json")
        // .headers(headers)
        .body(body_bytes);

    let response = match request_builder.send().await {
        Ok(response) => {
            let status = response.status().as_u16().to_string();
            state
                .metrics
                .requests_total
                .with_label_values(&[peer_type, &status])
                .inc();
            response
        }
        Err(e) => {
            state
                .metrics
                .requests_total
                .with_label_values(&[peer_type, "error"])
                .inc();
            warn!("Failed to send request: {}", e);
            return Err((StatusCode::BAD_GATEWAY, format!("Request failed: {}", e)));
        }
    };

    let response_headers = response.headers().clone();
    let response_bytes = response.bytes().await.unwrap();
    let mut resp = Response::new(response_bytes.into());
    for (name, value) in response_headers {
        if let Some(name) = name {
            resp.headers_mut().insert(name, value);
        }
    }
    Ok(resp)
}
