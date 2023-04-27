// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::anyhow;
use bip32::DerivationPath;
use clap::*;
use fastcrypto::encoding::{decode_bytes_hex, Base64, Encoding, Hex};
use fastcrypto::hash::HashFunction;
use fastcrypto::traits::KeyPair;
use shared_crypto::intent::{Intent, IntentMessage};
use std::fs;
use std::path::{Path, PathBuf};
use sui_keys::key_derive::generate_new_key;
use sui_keys::keypair_file::{
    read_authority_keypair_from_file, read_keypair_from_file, write_authority_keypair_to_file,
    write_keypair_to_file,
};
use sui_keys::keystore::{AccountKeystore, Keystore};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{get_authority_key_pair, EncodeDecodeBase64, SignatureScheme, SuiKeyPair};
use sui_types::crypto::{DefaultHash, PublicKey, Signature};
use sui_types::messages::TransactionData;
use sui_types::multisig::{MultiSig, MultiSigPublicKey, ThresholdUnit, WeightUnit};
use sui_types::signature::GenericSignature;
use tracing::info;
#[cfg(test)]
#[path = "unit_tests/keytool_tests.rs"]
mod keytool_tests;

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum KeyToolCommand {
    /// Generate a new keypair with key scheme flag {ed25519 | secp256k1 | secp256r1}
    /// with optional derivation path, default to m/44'/784'/0'/0'/0' for ed25519 or
    /// m/54'/784'/0'/0/0 for secp256k1 or m/74'/784'/0'/0/0 for secp256r1. Word
    /// length can be { word12 | word15 | word18 | word21 | word24} default to word12
    /// if not specified.
    ///
    /// The keypair file is output to the current directory. The content of the file is
    /// a Base64 encoded string of 33-byte `flag || privkey`. Note: To generate and add keypair
    /// to sui.keystore, use `sui client new-address`), see more at [enum SuiClientCommands].
    Generate {
        key_scheme: SignatureScheme,
        word_length: Option<String>,
        derivation_path: Option<DerivationPath>,
    },
    /// This reads the content at the provided file path. The accepted format can be
    /// [enum SuiKeyPair] (Base64 encoded of 33-byte `flag || privkey`) or `type AuthorityKeyPair`
    /// (Base64 encoded `privkey`). It prints its Base64 encoded public key and the key scheme flag.
    Show {
        file: PathBuf,
    },
    /// This takes [enum SuiKeyPair] of Base64 encoded of 33-byte `flag || privkey`). It
    /// outputs the keypair into a file at the current directory, and prints out its Sui
    /// address, Base64 encoded public key, and the key scheme flag.
    Unpack {
        keypair: SuiKeyPair,
    },
    /// List all keys by its Sui address, Base64 encoded public key, key scheme name in
    /// sui.keystore.
    List,
    /// Create signature using the private key for for the given address in sui keystore.
    /// Any signature commits to a [struct IntentMessage] consisting of the Base64 encoded
    /// of the BCS serialized transaction bytes itself (the result of
    /// [transaction builder API](https://docs.sui.io/sui-jsonrpc) and its intent. If
    /// intent is absent, default will be used. See [struct IntentMessage] and [struct Intent]
    /// for more details.
    Sign {
        #[clap(long, parse(try_from_str = decode_bytes_hex))]
        address: SuiAddress,
        #[clap(long)]
        data: String,
        #[clap(long)]
        intent: Option<Intent>,
    },
    /// Add a new key to sui.keystore based on the input mnemonic phrase, the key scheme flag {ed25519 | secp256k1 | secp256r1}
    /// and an optional derivation path, default to m/44'/784'/0'/0'/0' for ed25519 or m/54'/784'/0'/0/0 for secp256k1
    /// or m/74'/784'/0'/0/0 for secp256r1. Supports mnemonic phrase of word length 12, 15, 18`, 21, 24.
    Import {
        mnemonic_phrase: String,
        key_scheme: SignatureScheme,
        derivation_path: Option<DerivationPath>,
    },
    /// Convert private key from wallet format (hex of 32 byte private key) to sui.keystore format
    /// (base64 of 33 byte flag || private key) or vice versa.
    Convert {
        value: String,
    },

    /// This reads the content at the provided file path. The accepted format can be
    /// [enum SuiKeyPair] (Base64 encoded of 33-byte `flag || privkey`) or `type AuthorityKeyPair`
    /// (Base64 encoded `privkey`). This prints out the account keypair as Base64 encoded `flag || privkey`,
    /// the network keypair, worker keypair, protocol keypair as Base64 encoded `privkey`.
    LoadKeypair {
        file: PathBuf,
    },

    Base64PubKeyToAddress {
        base64_key: String,
    },

    /// To MultiSig Sui Address. Pass in a list of all public keys `flag || pk` in Base64.
    /// See `keytool list` for example public keys.
    MultiSigAddress {
        #[clap(long)]
        threshold: ThresholdUnit,
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        pks: Vec<PublicKey>,
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        weights: Vec<WeightUnit>,
    },

    /// Provides a list of signatures (`flag || sig || pk` encoded in Base64), threshold, a list of public keys.
    /// Returns a valid MultiSig and its sender address. The result can be used as signature field for `sui client execute-signed-tx`.
    /// The number of sigs must be greater than the threshold. The number of sigs must be smaller than the number of pks.
    MultiSigCombinePartialSig {
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        sigs: Vec<Signature>,
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        pks: Vec<PublicKey>,
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        weights: Vec<WeightUnit>,
        #[clap(long)]
        threshold: ThresholdUnit,
    },

    /// Converts a Base64 encoded string to its hexademical representation.
    Base64ToHex {
        base64: String,
    },

    /// Converts a hexademical string to its Base64 encoded representation.
    HexToBase64 {
        hex: String,
    },

    /// Converts a hexademical string to its correspinding bytes.
    HexToBytes {
        hex: String,
    },

    /// Converts an array of bytes to its hexademical string representation.
    BytesToHex {
        bytes: Vec<u8>,
    },

    /// Decodes a Base64 encoded string to its corresponding bytes.
    Base64ToBytes {
        base64: String,
    },

    /// Encodes an array of bytes to its Base64 string representation.
    BytesToBase64 {
        bytes: Vec<u8>,
    },
}

impl KeyToolCommand {
    pub fn execute(self, keystore: &mut Keystore) -> Result<(), anyhow::Error> {
        match self {
            KeyToolCommand::Generate {
                key_scheme,
                derivation_path,
                word_length,
            } => {
                if "bls12381" == key_scheme.to_string() {
                    // Generate BLS12381 key for authority without key derivation.
                    // The saved keypair is encoded `privkey || pubkey` without the scheme flag.
                    let (address, keypair) = get_authority_key_pair();
                    let file_name = format!("bls-{address}.key");
                    write_authority_keypair_to_file(&keypair, file_name)?;
                } else {
                    let (address, kp, scheme, _) =
                        generate_new_key(key_scheme, derivation_path, word_length)?;
                    let file = format!("{address}.key");
                    write_keypair_to_file(&kp, &file)?;
                    println!(
                        "Keypair wrote to file path: {:?} with scheme: {:?}",
                        file, scheme
                    );
                }
            }
            KeyToolCommand::Show { file } => {
                let res = read_keypair_from_file(&file);
                match res {
                    Ok(keypair) => {
                        println!("Public Key: {}", keypair.public().encode_base64());
                        println!("Flag: {}", keypair.public().flag());
                        if let PublicKey::Ed25519(public_key) = keypair.public() {
                            let peer_id = anemo::PeerId(public_key.0.into());
                            println!("PeerId: {}", peer_id);
                        }
                    }
                    Err(_) => {
                        let res = read_authority_keypair_from_file(&file);
                        match res {
                            Ok(keypair) => {
                                println!("Public Key: {}", keypair.public().encode_base64());
                                println!("Flag: {}", SignatureScheme::BLS12381);
                            }
                            Err(e) => {
                                println!("Failed to read keypair at path {:?} err: {:?}", file, e)
                            }
                        }
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
                        pub_key.encode_base64(),
                        pub_key.scheme().to_string()
                    );
                }
            }
            KeyToolCommand::Sign {
                address,
                data,
                intent,
            } => {
                println!("Signer address: {}", address);
                println!("Raw tx_bytes to execute: {}", data);
                let intent = intent.unwrap_or_else(Intent::sui_transaction);
                println!("Intent: {:?}", intent);
                let msg: TransactionData =
                    bcs::from_bytes(&Base64::decode(&data).map_err(|e| {
                        anyhow!("Cannot deserialize data as TransactionData {:?}", e)
                    })?)?;
                let intent_msg = IntentMessage::new(intent, msg);
                println!(
                    "Raw intent message: {:?}",
                    Base64::encode(bcs::to_bytes(&intent_msg)?)
                );
                let mut hasher = DefaultHash::default();
                hasher.update(bcs::to_bytes(&intent_msg)?);
                let digest = hasher.finalize().digest;
                println!("Digest to sign: {:?}", Base64::encode(digest));
                let sui_signature =
                    keystore.sign_secure(&address, &intent_msg.value, intent_msg.intent)?;
                println!(
                    "Serialized signature (`flag || sig || pk` in Base64): {:?}",
                    sui_signature.encode_base64()
                );
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

            KeyToolCommand::Convert { value } => match Base64::decode(&value) {
                Ok(decoded) => {
                    assert_eq!(decoded.len(), 33);
                    info!(
                        "Wallet formatted private key: 0x{}",
                        Hex::encode(&decoded[1..])
                    );
                }
                Err(_) => match Hex::decode(&value) {
                    Ok(decoded) => {
                        assert_eq!(decoded.len(), 32);
                        let mut res = Vec::new();
                        res.extend_from_slice(&[SignatureScheme::ED25519.flag()]);
                        res.extend_from_slice(&decoded);
                        info!("Keystore formatted private key: {:?}", Base64::encode(&res));
                    }
                    Err(_) => {
                        info!("Invalid private key format");
                    }
                },
            },

            KeyToolCommand::Base64PubKeyToAddress { base64_key } => {
                let pk = PublicKey::decode_base64(&base64_key)
                    .map_err(|e| anyhow!("Invalid base64 key: {:?}", e))?;
                let address = SuiAddress::from(&pk);
                println!("Address {:?}", address);
            }

            KeyToolCommand::Base64ToHex { base64 } => {
                let bytes =
                    Base64::decode(&base64).map_err(|e| anyhow!("Invalid base64 key: {:?}", e))?;
                let hex = Hex::from_bytes(&bytes);
                println!("{:?}", hex);
            }

            KeyToolCommand::HexToBase64 { hex } => {
                let bytes =
                    Hex::decode(&hex).map_err(|e| anyhow!("Invalid base64 key: {:?}", e))?;
                let base64 = Base64::from_bytes(&bytes);
                println!("{:?}", base64);
            }

            KeyToolCommand::HexToBytes { hex } => {
                let bytes =
                    Hex::decode(&hex).map_err(|e| anyhow!("Invalid base64 key: {:?}", e))?;
                println!("Bytes {:?}", bytes);
            }

            KeyToolCommand::BytesToHex { bytes } => {
                let hex = Hex::from_bytes(&bytes);
                println!("{:?}", hex);
            }

            KeyToolCommand::Base64ToBytes { base64 } => {
                let bytes =
                    Base64::decode(&base64).map_err(|e| anyhow!("Invalid base64 key: {:?}", e))?;
                println!("Bytes {:?}", bytes);
            }

            KeyToolCommand::BytesToBase64 { bytes } => {
                let base64 = Base64::from_bytes(&bytes);
                println!("{:?}", base64);
            }

            KeyToolCommand::LoadKeypair { file } => {
                match read_keypair_from_file(&file) {
                    Ok(keypair) => {
                        // Account keypair is encoded with the key scheme flag {},
                        // and network and worker keypair are not.
                        println!("Account Keypair: {}", keypair.encode_base64());
                        if let SuiKeyPair::Ed25519(kp) = keypair {
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
            KeyToolCommand::MultiSigAddress {
                threshold,
                pks,
                weights,
            } => {
                let multisig_pk = MultiSigPublicKey::new(pks.clone(), weights.clone(), threshold)?;
                let address: SuiAddress = multisig_pk.into();
                println!("MultiSig address: {address}");

                println!("Participating parties:");
                println!(
                    " {0: ^42} | {1: ^50} | {2: ^6}",
                    "Sui Address", "Public Key (Base64)", "Weight"
                );
                println!("{}", ["-"; 100].join(""));
                for (pk, w) in pks.into_iter().zip(weights.into_iter()) {
                    println!(
                        " {0: ^42} | {1: ^45} | {2: ^6}",
                        Into::<SuiAddress>::into(&pk),
                        pk.encode_base64(),
                        w
                    );
                }
            }
            KeyToolCommand::MultiSigCombinePartialSig {
                sigs,
                pks,
                weights,
                threshold,
            } => {
                let multisig_pk = MultiSigPublicKey::new(pks, weights, threshold)?;
                let address: SuiAddress = multisig_pk.clone().into();
                let multisig = MultiSig::combine(sigs, multisig_pk)?;
                let generic_sig: GenericSignature = multisig.into();
                println!("MultiSig address: {address}");
                println!("MultiSig parsed: {:?}", generic_sig);
                println!("MultiSig serialized: {:?}", generic_sig.encode_base64());
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
