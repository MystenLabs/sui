// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anvil::{eth::EthApi, NodeConfig, NodeHandle};

pub async fn spawn() -> (EthApi, NodeHandle) {
    let config = NodeConfig::default();
    anvil::spawn(config).await
}

mod tests {
    use super::*;

    #[tokio::test]
    async fn test_FIXME() {
        telemetry_subscribers::init_for_testing();
        let (api, handle) = spawn().await;
        println!("rpc address: {:?}", handle.socket_address());
    }
}