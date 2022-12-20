// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::anyhow;
use bip32::DerivationPath;
use clap::*;
use fastcrypto::encoding::{decode_bytes_hex, Base64, Encoding, Hex};
use fastcrypto::traits::{ToFromBytes, VerifyingKey};
use sui_keys::key_derive::generate_new_key;
use sui_types::intent::Intent;
use sui_types::messages::TransactionData;
use tracing::info;

use fastcrypto::ed25519::{Ed25519KeyPair, Ed25519PrivateKey, Ed25519PublicKey};
use sui_keys::keystore::{AccountKeystore, Keystore};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{
    get_authority_key_pair, AuthorityKeyPair, Ed25519SuiSignature, EncodeDecodeBase64,
    NetworkKeyPair, SignatureScheme, SuiKeyPair, SuiSignatureInner,
};
#[cfg(test)]
#[path = "unit_tests/keytool_tests.rs"]
mod keytool_tests;

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum KeyToolCommand {
    /// Generate a new keypair given a key scheme, can be one of ed25519, secp256k1, secp256r1, bls12381.
    /// The optional derivation path is supported, the default values are used if not provided:
    /// m/44'/784'/0'/0'/0' for ed25519 or m/54'/784'/0'/0/0 for secp256k1 or m/74'/784'/0'/0/0 for secp256r1.
    /// The output file is saved to current dir (to generate keypair and add to sui.keystore, use `sui client new-address`)
    /// The file content is a Base64 encoded string: `flag || privkey || pubkey`.
    Generate {
        key_scheme: SignatureScheme,
        derivation_path: Option<DerivationPath>,
    },
    /// Given the file at path, read it as Base64 encoded string of `flag || privkey || pubkey`.
    /// Outputs its address, public key and key scheme.
    Show { file: PathBuf },
    /// Given a Base64 encoded `flag || privkey || pubkey` string, outputs its address, public key and key scheme.
    Unpack { keypair: SuiKeyPair },
    /// List all keys in ~/.sui/sui.keystore by its address, public key and key scheme.
    List,
    /// Create signature using the sui keystore and provided data.
    Sign {
        #[clap(long, parse(try_from_str = decode_bytes_hex))]
        address: SuiAddress,
        #[clap(long)]
        data: String,
    },
    /// Import mnemonic phrase and generate keypair based on key scheme flag {ed25519 | secp256k1}
    /// with optional derivation path, default to m/44'/784'/0'/0'/0' for ed25519 or m/54'/784'/0'/0/0 for secp256k1 or m/74'/784'/0'/0/0 for secp256r1.
    Import {
        mnemonic_phrase: String,
        key_scheme: SignatureScheme,
        derivation_path: Option<DerivationPath>,
    },
    /// Given the file at path, read it as Base64 encoded string of `flag || privkey || pubkey`.
    /// Output a Base64 encoded string of `flag || privkey || pubkey`.
    /// Also outputs Base64 encoded string of `privkey || pubkey` accepted by ValidatorConfig and NodeConfig
    LoadKeypair { file: PathBuf },
}

impl KeyToolCommand {
    pub fn execute(self, keystore: &mut Keystore) -> Result<(), anyhow::Error> {
        match self {
            KeyToolCommand::Generate {
                key_scheme,
                derivation_path,
            } => {
                if "bls12381" == key_scheme.to_string() {
                    // Generate BLS12381 key for authority without key derivation.
                    // The saved keypair is encoded `privkey || pubkey` without the scheme flag.
                    let (address, keypair) = get_authority_key_pair();
                    let file_name = format!("bls-{address}.key");
                    write_authority_keypair_to_file(&keypair, &file_name)?;
                } else {
                    let (address, kp, scheme, _) = generate_new_key(key_scheme, derivation_path)?;
                    let file = format!("{address}.key");
                    write_keypair_to_file(&kp, &file)?;
                    println!(
                        "Keypair wrote to file path: {:?} with scheme: {:?}",
                        file, scheme
                    );
                }
            }
            KeyToolCommand::Show { file } => {
                let res: Result<SuiKeyPair, anyhow::Error> = read_keypair_from_file(&file);
                match res {
                    Ok(keypair) => {
                        println!("Keypair Base64: {:?}", keypair.encode_base64());
                        println!("Address: {:?}", SuiAddress::from(&keypair.public()));
                        println!("Pubkey Base64: {:?}", keypair.public());
                        println!("Pubkey Hex: {:?}", Hex::encode(keypair.public().as_ref()));
                        println!("Scheme: {:?}", keypair.public().scheme());
                    }
                    Err(e) => {
                        println!("Failed to read keypair at path {:?} err: {:?}", file, e)
                    }
                }
            }

            KeyToolCommand::Unpack { keypair } => {
                println!("Keypair Base64: {:?}", keypair.encode_base64());
                println!("Address: {:?}", SuiAddress::from(&keypair.public()));
                println!("Pubkey Base64: {:?}", keypair.public());
                println!("Pubkey Hex: {:?}", Hex::encode(keypair.public().as_ref()));
                println!("Scheme: {:?}", keypair.public().scheme());
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
                let tx_data: TransactionData = bcs::from_bytes(&message).map_err(|e| anyhow!(e))?;
                let sui_signature = keystore.sign_secure(&address, &tx_data, Intent::default())?;
                // Separate pub key and signature string, signature and pub key are concatenated with an '@' symbol.
                let signature_string = format!("{:?}", sui_signature);
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
                info!("Serialized signature Base64: {:?}", sui_signature);
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
                        println!(
                            "Encoded with flag (uses as input for `keytool unpack`): {}",
                            keypair.encode_base64()
                        );
                        // In validator config, the keypairs are encoded without flag.
                        match keypair {
                            SuiKeyPair::Ed25519(kp) => {
                                println!("Ed25519 Keypair: {}", kp.encode_base64())
                            }
                            SuiKeyPair::Secp256k1(kp) => {
                                println!("Secp256k1 Keypair: {}", kp.encode_base64())
                            }
                            SuiKeyPair::Secp256r1(kp) => {
                                println!("Secp256r1 Keypair: {}", kp.encode_base64())
                            }
                        }
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
