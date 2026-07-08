// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_package_alt::schema::Environment;
use sui_sdk::types::{
    digests::{MAINNET_CHAIN_IDENTIFIER_BASE58, TESTNET_CHAIN_IDENTIFIER_BASE58},
    supported_protocol_versions::Chain,
};

pub fn testnet_environment() -> Environment {
    Environment {
        name: Chain::Testnet.as_str().to_string(),
        id: TESTNET_CHAIN_IDENTIFIER_BASE58.to_string(),
    }
}

pub fn mainnet_environment() -> Environment {
    Environment {
        name: Chain::Mainnet.as_str().to_string(),
        id: MAINNET_CHAIN_IDENTIFIER_BASE58.to_string(),
    }
}
