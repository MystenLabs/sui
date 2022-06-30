// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use clap::*;
use std::fs;
use std::path::Path;
use sui_json_rpc_api::keystore::{Keystore, SuiKeystore};
use sui_types::base_types::decode_bytes_hex;
use sui_types::sui_serde::{Base64, Encoding};
use sui_types::{
    base_types::SuiAddress,
    crypto::{get_key_pair, KeyPair},
};
use tracing::info;

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum KeyToolCommand {
    /// Generate a new keypair
    Generate,
    /// Extract components
    Unpack { keypair: KeyPair },
    /// List all keys in the keystore
    List,
    /// Create signature using the sui keystore and provided data.
    Sign {
        #[clap(long, parse(try_from_str = decode_bytes_hex))]
        address: SuiAddress,
        #[clap(long)]
        data: String,
    },
}

impl KeyToolCommand {
    pub fn execute(self, keystore: SuiKeystore) -> Result<(), anyhow::Error> {
        match self {
            KeyToolCommand::Generate => {
                let (address, keypair) = get_key_pair();
                store_and_print_keypair(address, keypair)
            }
            KeyToolCommand::Unpack { keypair } => {
                store_and_print_keypair(SuiAddress::from(keypair.public_key_bytes()), keypair)
            }
            KeyToolCommand::List => {
                println!(
                    " {0: ^42} | {1: ^45} ",
                    "Sui Address", "Public Key (Base64)"
                );
                println!("{}", ["-"; 91].join(""));
                for keypair in keystore.key_pairs() {
                    println!(
                        " {0: ^42} | {1: ^45} ",
                        SuiAddress::from(keypair.public_key_bytes()),
                        Base64::encode(keypair.public_key_bytes().to_vec()),
                    );
                }
            }
            KeyToolCommand::Sign { address, data } => {
                info!("Data to sign : {}", data);
                info!("Address : {}", address);
                let message = Base64::decode(&data).map_err(|e| anyhow!(e))?;
                let signature = keystore.sign(&address, &message)?;
                // Separate pub key and signature string, signature and pub key are concatenated with an '@' symbol.
                let signature_string = format!("{:?}", signature);
                let sig_split = signature_string.split('@').collect::<Vec<_>>();
                let signature = sig_split
                    .first()
                    .ok_or_else(|| anyhow!("Error creating signature."))?;
                let pub_key = sig_split
                    .last()
                    .ok_or_else(|| anyhow!("Error creating signature."))?;
                info!("Public Key Base64: {}", pub_key);
                info!("Signature : {}", signature);
            }
        }

        Ok(())
    }
}

fn store_and_print_keypair(address: SuiAddress, keypair: KeyPair) {
    let path_str = format!("{}.key", address).to_lowercase();
    let path = Path::new(&path_str);
    let address = format!("{}", address);
    let kp = serde_json::to_string(&keypair).unwrap();
    let kp = &kp[1..kp.len() - 1];
    let out_str = format!("address: {}\nkeypair: {}", address, kp);
    fs::write(path, out_str).unwrap();
    println!("Address and keypair written to {}", path.to_str().unwrap());
}
