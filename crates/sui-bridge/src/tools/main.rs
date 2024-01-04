// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use ethers::abi::RawLog;
use ethers::prelude::*;
use mysten_metrics::start_prometheus_server;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use sui_bridge::abi::EthSuiBridge;
use sui_bridge::abi::EthSuiBridgeEvents;
use sui_bridge::eth_client::EthClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let private_key = std::env::var("BRIDGE_TEST_PRIVATE_KEY").unwrap();
    let url = "https://ethereum-sepolia.publicnode.com";
    let contract_address: Address = "0x7ee2fdbb3401b5b6B0db9c078b3C9F6B1d05534F".parse()?;

    // Init metrics server
    let metrics_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9192);
    let registry_service = start_prometheus_server(metrics_address);
    let prometheus_registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&prometheus_registry);
    tracing::info!("Metrics server started at port {}", 9192);

    // Init logging
    let (_guard, _filter_handle) = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .with_prom_registry(&prometheus_registry)
        .init();

    let provider =
        Provider::<Http>::try_from(url)?.interval(std::time::Duration::from_millis(2000));
    let provider = Arc::new(provider);
    let wallet = Wallet::from_str(private_key)?.with_chain_id(11155111u64);
    let address = wallet.address();
    println!("address: {:?}", address);
    let client = SignerMiddleware::new(provider, wallet);

    let contract = EthSuiBridge::new(contract_address, client.into());
    let recipient_address: Address = Address::zero();
    let tx = contract
        .bridge_eth_to_sui(Bytes::from(recipient_address.as_bytes().to_vec()), 0)
        .value(1u64); // Amount in wei

    // let wallet = wallet.connect(provider.clone());

    println!("sending tx: {:?}", tx);
    let tx_hash = tx.send().await?;

    println!("Transaction sent with hash: {:?}", tx_hash);
    tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;
    println!("wake up");

    let eth_client = EthClient::new(url).await?;
    let logs = eth_client
        .get_events_in_range(contract_address, 5021533, 5022533)
        .await
        .unwrap();

    for log in logs {
        let raw_log = RawLog {
            topics: log.log.topics.clone(),
            data: log.log.data.to_vec(),
        };
        if let Ok(decoded) = EthSuiBridgeEvents::decode_log(&raw_log) {
            println!("decoded: {:?}", decoded);
        } else {
            println!("failed to decode log: {:?}", raw_log);
        }
    }
    Ok(())
}
