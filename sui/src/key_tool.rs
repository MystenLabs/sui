// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use sui_types::{
    base_types::SuiAddress,
    crypto::{get_key_pair, KeyPair},
};
#[allow(clippy::large_enum_variant)]
#[derive(Parser)]
#[clap(
    name = "Sui Key Tool",
    about = "Utility For Generating Keys and Addresses Encoded as Base64 Bytes",
    rename_all = "kebab-case"
)]
enum KeyToolOpt {
    /// Generate a keypair
    Generate {},

    /// Extract components
    Unpack { keypair: KeyPair },
}

fn main() {
    let res = match KeyToolOpt::parse() {
        KeyToolOpt::Generate {} => get_key_pair(),
        KeyToolOpt::Unpack { keypair } => (SuiAddress::from(keypair.public_key_bytes()), keypair),
    };

    println!("Address: {} \nKeyPair: {:?}", res.0, res.1);
}
