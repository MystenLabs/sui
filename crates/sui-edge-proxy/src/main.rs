use bytes::Bytes;
use clap::Parser;
use http::{Response, StatusCode};
use pingora::apps::http_app::ServeHttp;
use pingora::protocols::Digest;
use sui_edge_proxy::config::ProxyConfig;
use sui_edge_proxy::{certificate::TLSCertCallback, config::PeerConfig};

use async_trait::async_trait;
use pingora::{
    listeners::TlsSettings,
    prelude::{http_proxy_service, HttpPeer, ProxyHttp, Result, Session},
    server::Server,
    services::listening::Service,
};
// TODO: look into using pingora_load_balancing
// use pingora_load_balancing::{health_check, selection::RoundRobin, LoadBalancer};

// use pingora::protocols::http::error_resp;
use tracing::{debug, info, warn};

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

// testing tls termination
// RUST_LOG=debug cargo run --bin sui-edge-proxy -- --config=crates/sui-edge-proxy/proxy.yaml
// mkdir -p keys
// openssl req -x509 -sha256 -days 356 -nodes -newkey rsa:2048 -subj "/CN=fullnode.mainnet.sui.io/C=UK/L=London" -keyout keys/key.pem -out keys/cert.crt

fn main() -> Result<()> {
    let (_guard, _handle) = telemetry_subscribers::TelemetryConfig::new().init();
    let args = Args::parse();

    let config: ProxyConfig = sui_edge_proxy::config::load(&args.config).unwrap();
    info!("listening on {}", config.listen_address);

    let mut my_server = Server::new(None).unwrap();
    my_server.bootstrap();

    // Add health check service
    let health_check = health_check_service(&format!("0.0.0.0:9000"));
    my_server.add_service(health_check);

    // Initialize reusable HttpPeer instances
    let read_peer = Arc::new(HttpPeer::new(
        config.read_peer.address.clone(),
        config.read_peer.use_tls,
        config.read_peer.sni.clone(),
    ));

    let execution_peer = Arc::new(HttpPeer::new(
        config.execution_peer.address.clone(),
        config.execution_peer.use_tls,
        config.execution_peer.sni.clone(),
    ));

    let mut lb = http_proxy_service(
        &my_server.configuration,
        LB {
            read_peer,
            execution_peer,
        },
    );

    if let Some(tls_config) = config.tls {
        info!("TLS config found");
        let cert_callback =
            TLSCertCallback::new(tls_config.cert_path, tls_config.key_path, tls_config.sni);
        let cert_callback = Box::new(cert_callback);
        // TODO error handling
        let tls_config = TlsSettings::with_callbacks(cert_callback).unwrap();
        lb.add_tls_with_settings(&config.listen_address.as_str(), None, tls_config);
    } else {
        info!("No TLS config found");
        lb.add_tcp(&config.listen_address.as_str());
    }

    my_server.add_service(lb);

    my_server.run_forever();
}

use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct TimingContext {
    start_time: Instant,
    phase_timings: Vec<(String, Duration)>,
}

impl TimingContext {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            phase_timings: Vec::new(),
        }
    }

    pub fn record_phase(&mut self, phase: &str, start: Instant) {
        let duration = start.elapsed();
        self.phase_timings.push((phase.to_string(), duration));
    }

    pub fn total_duration(&self) -> Duration {
        self.start_time.elapsed()
    }
}

pub struct LB {
    read_peer: Arc<HttpPeer>,
    execution_peer: Arc<HttpPeer>,
}

#[async_trait]
impl ProxyHttp for LB {
    type CTX = TimingContext;

    fn new_ctx(&self) -> Self::CTX {
        TimingContext::new()
    }

    async fn upstream_peer(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let phase_start = Instant::now();

        let peer = if let Some(transaction_type) =
            session.req_header().headers.get("Sui-Transaction-Type")
        {
            match transaction_type.to_str() {
                Ok("execute") => {
                    info!("Using execution peer");
                    self.execution_peer.clone()
                }
                Ok(_) => {
                    info!("Using read peer");
                    self.read_peer.clone()
                }
                Err(e) => {
                    warn!("Failed to read transaction_type header: {}", e);
                    self.read_peer.clone()
                }
            }
        } else {
            self.read_peer.clone()
        };

        // try hardcoding the http version to h2
        let mut new_peer = (*peer).clone();
        new_peer.options.set_http_version(2, 2);
        ctx.record_phase("upstream_peer", phase_start);

        Ok(Box::new((new_peer).clone()))
    }

    async fn connected_to_upstream(
        &self,
        _session: &mut Session,
        _reused: bool,
        peer: &HttpPeer,
        _fd: std::os::unix::io::RawFd,
        _digest: Option<&Digest>,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        let phase_start = Instant::now();
        ctx.record_phase("connected_to_upstream", phase_start);
        if !matches!(peer.options.alpn, pingora::protocols::ALPN::H2) {
            warn!("Upstream peer is not using h2");
        }
        info!("Upstream peer is using {:?}", peer.options.alpn);
        Ok(())
    }

    fn upstream_response_filter(
        &self,
        _session: &mut Session,
        _upstream_response: &mut pingora::http::ResponseHeader,
        ctx: &mut Self::CTX,
    ) {
        let phase_start = Instant::now();
        ctx.record_phase("upstream_response_filter", phase_start);
    }

    async fn response_filter(
        &self,
        _session: &mut Session,
        _upstream_response: &mut pingora::http::ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        let phase_start = Instant::now();
        ctx.record_phase("response_filter", phase_start);
        Ok(())
    }
    async fn logging(
        &self,
        _session: &mut Session,
        _e: Option<&pingora::Error>,
        ctx: &mut Self::CTX,
    ) {
        let total_duration = ctx.total_duration();
        info!("Total request duration: {:?}", total_duration);

        for (phase, duration) in &ctx.phase_timings {
            info!("Phase {} took {:?}", phase, duration);
        }
    }
}

// Add this new function for the health check service
fn health_check_service(listen_addr: &str) -> Service<HealthCheckApp> {
    let mut service = Service::new("Health Check Service".to_string(), HealthCheckApp {});
    service.add_tcp(listen_addr);
    service
}

pub struct HealthCheckApp;

#[async_trait]
impl ServeHttp for HealthCheckApp {
    async fn response(
        &self,
        _http_stream: &mut pingora::protocols::http::ServerSession,
    ) -> Response<Vec<u8>> {
        let body = Bytes::from("up");

        Response::builder()
            .status(StatusCode::OK)
            .header(http::header::CONTENT_TYPE, "text/html")
            .header(http::header::CONTENT_LENGTH, body.len())
            .body(body.to_vec())
            .unwrap()
    }
}
