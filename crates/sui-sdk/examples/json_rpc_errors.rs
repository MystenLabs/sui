// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod utils;
use anyhow::bail;
use sui_sdk::error::{Error, JsonRpcError};
use utils::setup_for_read;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let (sui, active_address) = setup_for_read().await?;
    let coin_type = Some("0x42".to_string());
    let coins = sui
        .coin_read_api()
        .get_coins(active_address, coin_type.clone(), None, Some(5))
        .await;
    let error = coins.unwrap_err();
    if let Error::RpcError(rpc_error) = error {
        let converted: JsonRpcError = rpc_error.into();
        println!(" *** RpcError ***");
        println!("{converted}");
        println!("{}", converted.is_client_error());
    } else {
        bail!("Expected Error::RpcError, got {:?}", error);
    }
    Ok(())
}
