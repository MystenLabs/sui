// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use clap::*;
use std::fs;
use std::path::{Path, PathBuf};
use sui_sdk::crypto::{Keystore, SuiKeystore};
use sui_types::base_types::{decode_bytes_hex, encode_bytes_hex};
use sui_types::crypto::{AccountKeyPair, AuthorityKeyPair, KeypairTraits};
use sui_types::sui_serde::{Base64, Encoding};
use sui_types::{base_types::SuiAddress, crypto::get_key_pair};
use tracing::info;

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum KeyToolCommand {
    /// Generate a new keypair
    Generate,
    Show {
        file: PathBuf,
    },
    /// Extract components
    Unpack {
        keypair: AccountKeyPair,
    },
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
                let (_address, keypair): (_, AccountKeyPair) = get_key_pair();

                let hex = encode_bytes_hex(keypair.public());
                let file_name = format!("{hex}.key");
                write_keypair_to_file(&keypair, &file_name)?;
                println!("Ed25519 key generated and saved to '{file_name}'");
            }

            KeyToolCommand::Show { file } => {
                let keypair: AuthorityKeyPair = read_keypair_from_file(file)?;
                println!("Public Key: {}", encode_bytes_hex(keypair.public()));
            }

            KeyToolCommand::Unpack { keypair } => {
                store_and_print_keypair(keypair.public().into(), keypair)
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
                        Into::<SuiAddress>::into(keypair.public()),
                        Base64::encode(keypair.public().as_ref()),
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
                let flag = sig_split
                    .first()
                    .ok_or_else(|| anyhow!("Error creating signature."))?;
                let signature = sig_split
                    .get(1)
                    .ok_or_else(|| anyhow!("Error creating signature."))?;
                let pub_key = sig_split
                    .last()
                    .ok_or_else(|| anyhow!("Error creating signature."))?;
                info!("Flag Base64: {}", flag);
                info!("Public Key Base64: {}", pub_key);
                info!("Signature : {}", signature);
            }
        }

        Ok(())
    }
}

fn store_and_print_keypair<K: KeypairTraits>(address: SuiAddress, keypair: K) {
    let path_str = format!("{}.key", address).to_lowercase();
    let path = Path::new(&path_str);
    let address = format!("{}", address);
    let kp = keypair.encode_base64();
    let kp = &kp[1..kp.len() - 1];
    let out_str = format!("address: {}\nkeypair: {}", address, kp);
    fs::write(path, out_str).unwrap();
    println!("Address and keypair written to {}", path.to_str().unwrap());
}

pub fn write_keypair_to_file<K: KeypairTraits, P: AsRef<std::path::Path>>(
    keypair: &K,
    path: P,
) -> anyhow::Result<()> {
    let contents = keypair.encode_base64();
    std::fs::write(path, contents)?;
    Ok(())
}

pub fn read_keypair_from_file<K: KeypairTraits, P: AsRef<std::path::Path>>(
    path: P,
) -> anyhow::Result<K> {
    let contents = std::fs::read_to_string(path)?;
    K::decode_base64(contents.as_str().trim()).map_err(|e| anyhow!(e))
}
