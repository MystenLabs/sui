// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use sui_json_rpc_types::{SuiObjectDataOptions, SuiObjectResponseQuery};
use sui_sdk::types::base_types::SuiAddress;
use sui_sdk::SuiClientBuilder;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClientBuilder::default()
        .build("https://fullnode.devnet.sui.io:443")
        .await?;
    let address = SuiAddress::from_str("0x2567a1d98a4d703229e4fee6b61f7b8c80ca366c99e9d4408c0c5eb25699760a")?;
    let objects = sui
        .read_api()
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new(),
            )),
            None,
            None,
        )
        .await?;
    println!("{:?}", objects);
    Ok(())
}
