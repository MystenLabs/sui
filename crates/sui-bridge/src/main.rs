// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use sui_bridge::server::run_server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // TODO: init logging
    // TODO: allow configuration of port
    let socket_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9000);
    run_server(&socket_address).await;
    Ok(())
}
