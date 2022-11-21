// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::anyhow;
use bip32::{DerivationPath, Mnemonic};
use clap::*;
use fastcrypto::encoding::{decode_bytes_hex, Base64, Encoding};
use fastcrypto::traits::{ToFromBytes, VerifyingKey};
use signature::rand_core::OsRng;
use sui_keys::key_derive::derive_key_pair_from_path;
use tracing::info;

use fastcrypto::ed25519::{Ed25519KeyPair, Ed25519PrivateKey, Ed25519PublicKey};
use sui_keys::keystore::{AccountKeystore, Keystore};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{
    get_key_pair, AuthorityKeyPair, Ed25519SuiSignature, EncodeDecodeBase64, NetworkKeyPair,
    SignatureScheme, SuiKeyPair, SuiSignatureInner,
};
#[cfg(test)]
#[path = "unit_tests/keytool_tests.rs"]
mod keytool_tests;

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum KeyToolCommand {
    /// Generate a new keypair with keypair scheme flag {ed25519 | secp256k1}
    /// with optional derivation path, default to m/44'/784'/0'/0'/0' for ed25519 or m/54'/784'/0'/0/0 for secp256k1.
    /// And output file to current dir (to generate keypair and add to sui.keystore, use `sui client new-address`)
    Generate {
        key_scheme: SignatureScheme,
        derivation_path: Option<DerivationPath>,
    },
    Show {
        file: PathBuf,
    },
    /// Extract components of a base64-encoded keypair to reveal the Sui address, public key, and key scheme flag.
    Unpack {
        keypair: SuiKeyPair,
    },
    /// List all keys by its address, public key, key scheme in the keystore
    List,
    /// Create signature using the sui keystore and provided data.
    Sign {
        #[clap(long, parse(try_from_str = decode_bytes_hex))]
        address: SuiAddress,
        #[clap(long)]
        data: String,
    },
    /// Import mnemonic phrase and generate keypair based on key scheme flag {ed25519 | secp256k1}
    /// with optional derivation path, default to m/44'/784'/0'/0'/0' for ed25519 or m/54'/784'/0'/0/0 for secp256k1.
    Import {
        mnemonic_phrase: String,
        key_scheme: SignatureScheme,
        derivation_path: Option<DerivationPath>,
    },
    /// Read keypair from path and show its base64 encoded value with flag. This is useful
    /// to generate protocol, account, worker, network keys in NodeConfig with its expected encoding.
    LoadKeypair {
        file: PathBuf,
    },
}

impl KeyToolCommand {
    pub fn execute(self, keystore: &mut Keystore) -> Result<(), anyhow::Error> {
        match self {
            KeyToolCommand::Generate {
                key_scheme,
                derivation_path,
            } => {
                let k = key_scheme.to_string();
                if "bls12381" == key_scheme.to_string() {
                    let (address, keypair): (_, AuthorityKeyPair) = get_key_pair();
                    let file_name = format!("bls-{address}.key");
                    write_authority_keypair_to_file(&keypair, &file_name)?;
                } else {
                    let mnemonic = Mnemonic::random(OsRng, Default::default());
                    let seed = mnemonic.to_seed("");
                    match derive_key_pair_from_path(seed.as_bytes(), derivation_path, &key_scheme) {
                        Ok((address, kp)) => {
                            let file_name = format!("{address}.key");
                            write_keypair_to_file(&kp, &file_name)?;
                            println!("{:?} key generated and saved to '{file_name}'", k);
                        }
                        Err(e) => println!("Failed to generate keypair: {:?}", e),
                    }
                }
            }
            KeyToolCommand::Show { file } => {
                let res: Result<SuiKeyPair, anyhow::Error> = read_keypair_from_file(&file);
                match res {
                    Ok(keypair) => {
                        println!("Public Key: {}", Base64::encode(keypair.public()));
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
                    " {0: ^42} | {1: ^45} | {2: ^6}",
                    "Sui Address", "Public Key (Base64)", "Scheme"
                );
                println!("{}", ["-"; 100].join(""));
                for pub_key in keystore.keys() {
                    println!(
                        " {0: ^42} | {1: ^45} | {2: ^6}",
                        Into::<SuiAddress>::into(&pub_key),
                        Base64::encode(&pub_key),
                        pub_key.scheme().to_string()
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
            KeyToolCommand::Import {
                mnemonic_phrase,
                key_scheme,
                derivation_path,
            } => {
                let address =
                    keystore.import_from_mnemonic(&mnemonic_phrase, key_scheme, derivation_path)?;
                info!("Key imported for address [{address}]");
            }

            KeyToolCommand::LoadKeypair { file } => {
                match read_keypair_from_file(&file) {
                    Ok(keypair) => {
                        // Account keypair is encoded with the key scheme flag {},
                        // and network and worker keypair are not.
                        println!("Account Keypair: {}", keypair.encode_base64());
                        if let SuiKeyPair::Ed25519SuiKeyPair(kp) = keypair {
                            println!("Network Keypair: {}", kp.encode_base64());
                            println!("Worker Keypair: {}", kp.encode_base64());
                        };
                    }
                    Err(_) => {
                        // Authority keypair file is not stored with the flag, it will try read as BLS keypair..
                        match read_authority_keypair_from_file(&file) {
                            Ok(kp) => println!("Protocol Keypair: {}", kp.encode_base64()),
                            Err(e) => {
                                println!("Failed to read keypair at path {:?} err: {:?}", file, e)
                            }
                        }
                    }
                }
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
    let flag = keypair.public().flag();
    let out_str = format!("address: {}\nkeypair: {}\nflag: {}", address, kp, flag);
    fs::write(path, out_str).unwrap();
    println!(
        "Address, keypair and key scheme written to {}",
        path.to_str().unwrap()
    );
}

pub fn write_keypair_to_file<P: AsRef<std::path::Path>>(
    keypair: &SuiKeyPair,
    path: P,
) -> anyhow::Result<()> {
    let contents = keypair.encode_base64();
    std::fs::write(path, contents)?;
    Ok(())
}

pub fn write_authority_keypair_to_file<P: AsRef<std::path::Path>>(
    keypair: &AuthorityKeyPair,
    path: P,
) -> anyhow::Result<()> {
    let contents = keypair.encode_base64();
    std::fs::write(path, contents)?;
    Ok(())
}

pub fn read_authority_keypair_from_file<P: AsRef<std::path::Path>>(
    path: P,
) -> anyhow::Result<AuthorityKeyPair> {
    let contents = std::fs::read_to_string(path)?;
    AuthorityKeyPair::decode_base64(contents.as_str().trim()).map_err(|e| anyhow!(e))
}

pub fn read_keypair_from_file<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<SuiKeyPair> {
    let contents = std::fs::read_to_string(path)?;
    SuiKeyPair::decode_base64(contents.as_str().trim()).map_err(|e| anyhow!(e))
}

pub fn read_network_keypair_from_file<P: AsRef<std::path::Path>>(
    path: P,
) -> anyhow::Result<NetworkKeyPair> {
    let value = std::fs::read_to_string(path)?;
    let bytes = Base64::decode(value.as_str()).map_err(|e| anyhow::anyhow!(e))?;
    if let Some(flag) = bytes.first() {
        if flag == &Ed25519SuiSignature::SCHEME.flag() {
            let priv_key_bytes = bytes
                .get(1 + Ed25519PublicKey::LENGTH..)
                .ok_or_else(|| anyhow!("Invalid length"))?;
            let sk = Ed25519PrivateKey::from_bytes(priv_key_bytes)?;
            return Ok(<Ed25519KeyPair as From<Ed25519PrivateKey>>::from(sk));
        }
    }
    Err(anyhow!("Invalid bytes"))
}
