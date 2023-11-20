// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anemo_benchmark::{server::Server, BenchmarkClient, BenchmarkServer};
use clap::Parser;
use rand::Rng;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Parser, Debug)]
#[command(
    name = "anemo-benchmark",
    about = "Benchmarking tool for Anemo",
    rename_all = "kebab-case",
    author,
    version
)]
struct Args {
    /// Remote peer addresses to connect to.
    #[arg(long)]
    addrs: Vec<String>,

    /// Number of concurrent upload requests to maintain per peer.
    #[arg(long, default_value_t = 1)]
    requests_up: u8,

    /// Number of bytes per individual upload request.
    #[arg(long, default_value_t = 16*1024)]
    size_up: u32,

    /// Number of concurrent download requests to maintain per peer.
    #[arg(long, default_value_t = 1)]
    requests_down: u8,

    /// Number of bytes per individual download request.
    #[arg(long, default_value_t = 16*1024)]
    size_down: u32,

    /// Port to bind to.
    #[arg(short, long)]
    port: u16,

    /// Frequency for printing statistics.
    #[arg(short, long, default_value_t = 10)]
    tick_secs: u64,

    /// UDP socket send buffer size.
    #[arg(long)]
    socket_send_buffer_size: Option<usize>,

    /// UDP socket receive buffer size.
    #[arg(long)]
    socket_receive_buffer_size: Option<usize>,
}

pub fn random_key() -> [u8; 32] {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rng, &mut bytes[..]);
    bytes
}

async fn start_server(
    config: anemo::Config,
    port: u16,
    addrs: Vec<String>,
) -> (anemo::Network, Vec<anemo::Peer>) {
    let routes = anemo::Router::new().add_rpc_service(BenchmarkServer::new(Server));
    let network = anemo::Network::bind(anemo::types::Address::HostAndPort {
        host: "0.0.0.0".into(),
        port,
    })
    .config(config)
    .private_key(random_key())
    .server_name("anemo_benchmark")
    .start(routes)
    .unwrap();

    let mut peers = Vec::new();
    for addr in addrs {
        let peer_id = match network.connect(addr.clone()).await {
            Ok(peer_id) => peer_id,
            Err(e) => panic!("could not connect to peer at address {addr}: {e}"),
        };
        // Configure known_peers after manual connection acquires peer ID.
        network.known_peers().insert(anemo::types::PeerInfo {
            peer_id,
            affinity: anemo::types::PeerAffinity::High,
            address: vec![addr.into()],
        });
        peers.push(
            network
                .peer(peer_id)
                .expect("network has peer we just connected to"),
        );
    }

    (network, peers)
}

async fn upload_to_peer(
    peer: anemo::Peer,
    bytes: Vec<u8>,
    notify: UnboundedSender<(anemo::PeerId, Option<anemo::rpc::Status>)>,
) {
    let peer_id = peer.peer_id();
    let mut client = BenchmarkClient::new(peer);
    loop {
        let result = client
            .send_bytes(anemo::Request::new(bytes.clone()))
            .await
            .map(|_| peer_id);
        notify.send((peer_id, result.err())).unwrap();
    }
}

async fn download_from_peer(
    peer: anemo::Peer,
    size: u32,
    notify: UnboundedSender<(anemo::PeerId, Option<anemo::rpc::Status>)>,
) {
    let peer_id = peer.peer_id();
    let mut client = BenchmarkClient::new(peer);
    loop {
        let result = client.request_bytes(anemo::Request::new(size)).await;
        notify.send((peer_id, result.err())).unwrap();
    }
}

#[tokio::main]
#[allow(clippy::disallowed_methods)] // unbounded_channel is ok for benchmark reporting
async fn main() {
    let args: Args = Args::parse();
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let mut config = anemo::Config::default();
    let mut quic_config = anemo::QuicConfig::default();
    quic_config.socket_send_buffer_size = args.socket_send_buffer_size;
    quic_config.socket_receive_buffer_size = args.socket_receive_buffer_size;
    config.quic = Some(quic_config);

    let (_network, peers) = start_server(config, args.port, args.addrs.clone()).await;

    let rng = rand::thread_rng();
    let send_bytes: Vec<_> = rng
        .sample_iter(rand::distributions::Standard)
        .take(args.size_up as usize)
        .collect();

    let mut tasks = tokio::task::JoinSet::new();
    let (upload_notify_tx, mut upload_notify) = tokio::sync::mpsc::unbounded_channel();
    let (download_notify_tx, mut download_notify) = tokio::sync::mpsc::unbounded_channel();

    for _ in 0..args.requests_up {
        for peer in peers.iter().cloned() {
            tasks.spawn(upload_to_peer(
                peer,
                send_bytes.clone(),
                upload_notify_tx.clone(),
            ));
        }
    }
    for _ in 0..args.requests_down {
        for peer in peers.iter().cloned() {
            tasks.spawn(download_from_peer(
                peer,
                args.size_down,
                download_notify_tx.clone(),
            ));
        }
    }

    let mut interval = tokio::time::interval(Duration::from_secs(args.tick_secs));
    let mut upload_requests: usize = 0;
    let mut upload_errors: usize = 0;
    let mut upload_bytes: usize = 0;
    let mut download_requests: usize = 0;
    let mut download_errors: usize = 0;
    let mut download_bytes: usize = 0;
    let mut instant = interval.tick().await;

    loop {
        tokio::select! {
            Some((peer_id, error)) = upload_notify.recv() => {
                upload_requests += 1;
                if let Some(error) = error {
                    upload_errors += 1;
                    println!("upload error on peer {peer_id}: {:?}", error);
                } else {
                    upload_bytes += args.size_up as usize;
                }
            },
            Some((peer_id, error)) = download_notify.recv() => {
                download_requests += 1;
                if let Some(error) = error {
                    download_errors += 1;
                    println!("upload error on peer {peer_id}: {:?}", error);
                } else {
                    download_bytes += args.size_down as usize;
                }
            },
            new_instant = interval.tick() => {
                let duration_secs = instant.elapsed().as_secs_f64();
                println!("upload: {upload_requests} requests, {upload_errors} errors, {} Mbps", (upload_bytes as f64 * 8. / 1_048_576.)/duration_secs);
                println!("download: {download_requests} requests, {download_errors} errors, {} Mbps", (download_bytes as f64 * 8. / 1_048_576.)/duration_secs);
                for peer in peers.iter() {
                    let stats = peer.connection_stats();
                    println!(
                        "peer {peer_id}:\n\trtt {rtt:?} congestion_events {ce:?} lost_packets {lp:?} cwnd {cwd:?}",
                        peer_id = peer.peer_id(),
                        rtt = stats.path.rtt,
                        ce = stats.path.congestion_events,
                        lp = stats.path.lost_packets,
                        cwd = stats.path.cwnd,
                    );
                }
                upload_requests = 0;
                upload_errors   = 0;
                upload_bytes    = 0;
                download_requests = 0;
                download_errors   = 0;
                download_bytes    = 0;
                instant = new_instant;
            }
        }
    }
}
