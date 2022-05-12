// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use std::{fs, path::Path};
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
    let path_str = format!("{}.key", res.0).to_lowercase();
    let path = Path::new(&path_str);
    let address = format!("{}", res.0);
    let kp = serde_json::to_string(&res.1).unwrap();
    let kp = &kp[1..kp.len() - 1];
    let out_str = format!("address: {}\nkeypair: {}", address, kp);
    fs::write(path, out_str).unwrap();
    println!("Address and keypair written to {}", path.to_str().unwrap());
}
