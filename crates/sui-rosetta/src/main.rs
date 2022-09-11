use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use sui_rosetta::types::SuiEnv;
use sui_rosetta::RosettaOfflineServer;

#[tokio::main]
async fn main() {
    let (_guard, _) = telemetry_subscribers::TelemetryConfig::new(env!("CARGO_BIN_NAME"))
        .with_env()
        .init();

    let server = RosettaOfflineServer::new(SuiEnv::LocalNet);
    server
        .serve(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::UNSPECIFIED,
            9003,
        )))
        .await
        .unwrap()
        .unwrap();
}
