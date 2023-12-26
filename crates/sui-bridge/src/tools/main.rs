// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::Ipv4Addr;
use std::str::FromStr;
use std::sync::Arc;
use std::net::SocketAddr;
use std::net::IpAddr;
use ethers::abi::RawLog;
use ethers::prelude::*;
use mysten_metrics::start_prometheus_server;
use sui_bridge::abi::TestBridgeContract;
use sui_bridge::abi::TestBridgeContractEvents;
use sui_bridge::eth_client::EthClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {

    let private_key = "FIXME";
    let url = "https://1rpc.io/sepolia";
    let contract_address: Address = "0x715e31774dcf428dba721fab10d24286a36764c5".parse()?;

    // Init metrics server
    let metrics_address =
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9192);
    let registry_service = start_prometheus_server(metrics_address);
    let prometheus_registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&prometheus_registry);
    tracing::info!("Metrics server started at port {}", 9192);

    // Init logging
    let (_guard, _filter_handle) = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .with_prom_registry(&prometheus_registry)
        .init();

    // let provider = Provider::<Http>::try_from(url)?
    //     .interval(std::time::Duration::from_millis(2000));
    // let provider = Arc::new(provider);
    // let wallet = Wallet::from_str(private_key)?.with_chain_id(11155111u64);
    // let client = SignerMiddleware::new(provider, wallet);

    // let contract = TestBridgeContract::new(contract_address, client.into());

    // let recipient_address: Address = Address::zero();
    // let tx = contract.init_bridging_token_tx(recipient_address).value(100000000000u64); // Amount in wei

    // // let wallet = wallet.connect(provider.clone());

    // let tx_hash = tx.send().await?;
    

    // println!("Transaction sent with hash: {:?}", tx_hash);
    // tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;
    // println!("wake up");

    let eth_client = EthClient::new(url).await?;
    let logs = eth_client.get_events_in_range(contract_address, 4921568, 4921568).await.unwrap();

    for log in logs {
        let raw_log = RawLog {
            topics: log.log.topics.clone(),
            data: log.log.data.to_vec(),
        };
        if let Ok(decoded) = TestBridgeContractEvents::decode_log(&raw_log) {
            println!("decoded: {:?}", decoded);
        } else {
            println!("failed to decode log: {:?}", raw_log);
        }
    }
    Ok(())
}