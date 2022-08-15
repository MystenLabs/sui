// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::anyhow;
use clap::*;
use tracing::info;

use sui_sdk::crypto::SuiKeystore;
use sui_types::base_types::{decode_bytes_hex, encode_bytes_hex};
use sui_types::crypto::{
    AccountKeyPair, AuthorityKeyPair, EncodeDecodeBase64, KeypairTraits, SuiKeyPair,
};
use sui_types::sui_serde::{Base64, Encoding};
use sui_types::{base_types::SuiAddress, crypto::get_key_pair};

#[cfg(test)]
#[path = "unit_tests/keytool_tests.rs"]
mod keytool_tests;

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
        keypair: SuiKeyPair,
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
    Import {
        mnemonic_phrase: String,
    },
}

impl KeyToolCommand {
    pub fn execute(self, keystore: &mut SuiKeystore) -> Result<(), anyhow::Error> {
        match self {
            KeyToolCommand::Generate => {
                // TODO: add flag to this command to enable generate Secp256k1 keypair
                let (_address, keypair): (_, AccountKeyPair) = get_key_pair();

                let hex = encode_bytes_hex(keypair.public());
                let file_name = format!("{hex}.key");
                write_keypair_to_file(&SuiKeyPair::Ed25519SuiKeyPair(keypair), &file_name)?;
                println!("Ed25519 key generated and saved to '{file_name}'");
            }

            KeyToolCommand::Show { file } => {
                let res: Result<SuiKeyPair, anyhow::Error> = read_keypair_from_file(&file);
                match res {
                    Ok(keypair) => {
                        println!("Public Key: {}", encode_bytes_hex(keypair.public()));
                        println!("Flag: {}", keypair.public().flag());
                    }
                    Err(e) => {
                        println!("Failed to read keypair at path {:?} err: {:?}", file, e)
                    }
                }
            }

            KeyToolCommand::Unpack { keypair } => {
                store_and_print_keypair((&keypair.public()).into(), keypair)
            }
            KeyToolCommand::List => {
                println!(
                    " {0: ^42} | {1: ^45} | {2: ^1}",
                    "Sui Address", "Public Key (Base64)", "Flag"
                );
                println!("{}", ["-"; 100].join(""));
                for pub_key in keystore.keys() {
                    println!(
                        " {0: ^42} | {1: ^45} | {2: ^1}",
                        Into::<SuiAddress>::into(&pub_key),
                        Base64::encode(&pub_key),
                        pub_key.flag()
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
            KeyToolCommand::Import { mnemonic_phrase } => {
                let address = keystore.import_from_mnemonic(&mnemonic_phrase)?;
                info!("Key imported for address [{address}]");
            }
        }

        Ok(())
    }
}

fn store_and_print_keypair(address: SuiAddress, keypair: SuiKeyPair) {
    let path_str = format!("{}.key", address).to_lowercase();
    let path = Path::new(&path_str);
    let address = format!("{}", address);
    let kp = keypair.encode_base64();
    let kp = &kp[1..kp.len() - 1];
    let out_str = format!("address: {}\nkeypair: {}", address, kp);
    fs::write(path, out_str).unwrap();
    println!("Address and keypair written to {}", path.to_str().unwrap());
}

pub fn write_keypair_to_file<P: AsRef<std::path::Path>>(
    keypair: &SuiKeyPair,
    path: P,
) -> anyhow::Result<()> {
    let contents = keypair.encode_base64();
    std::fs::write(path, contents)?;
    Ok(())
}

pub fn read_authority_keypair_from_file<P: AsRef<std::path::Path>>(
    path: P,
) -> anyhow::Result<AuthorityKeyPair> {
    match read_keypair_from_file(path) {
        Ok(kp) => match kp {
            SuiKeyPair::Ed25519SuiKeyPair(k) => Ok(k),
            SuiKeyPair::Secp256k1SuiKeyPair(_) => Err(anyhow!("Invalid authority keypair type")),
        },
        Err(e) => Err(anyhow!("Failed to read keypair file {:?}", e)),
    }
}

pub fn read_keypair_from_file<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<SuiKeyPair> {
    let contents = std::fs::read_to_string(path)?;
    SuiKeyPair::decode_base64(contents.as_str().trim()).map_err(|e| anyhow!(e))
}
