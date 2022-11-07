// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use sui_sdk::types::base_types::SuiAddress;
use sui_sdk::SuiClient;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClient::new("https://fullnode.devnet.sui.io:443", None, None).await?;
    let address = SuiAddress::from_str("0xec11cad080d0496a53bafcea629fcbcfff2a9866")?;
    let objects = sui.read_api().get_objects_owned_by_address(address).await?;
    println!("{:?}", objects);
    Ok(())
}
