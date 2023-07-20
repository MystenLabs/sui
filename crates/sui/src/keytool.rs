// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::anyhow;
use bip32::DerivationPath;
use clap::*;
use fastcrypto::ed25519::Ed25519KeyPair;
use fastcrypto::encoding::{decode_bytes_hex, Base64, Encoding, Hex};
use fastcrypto::hash::HashFunction;
use fastcrypto::secp256k1::recoverable::Secp256k1Sig;
use fastcrypto::traits::{KeyPair, ToFromBytes};
use fastcrypto_zkp::bn254::api::Bn254Fr;
use fastcrypto_zkp::bn254::poseidon::PoseidonWrapper;
use fastcrypto_zkp::bn254::zk_login::OAuthProvider;
use fastcrypto_zkp::bn254::zk_login::{
    big_int_str_to_bytes, AuxInputs, PublicInputs, SupportedKeyClaim, ZkLoginProof,
};
use num_bigint::{BigInt, Sign};
use rand::rngs::StdRng;
use rand::SeedableRng;
use rusoto_core::Region;
use rusoto_kms::{Kms, KmsClient, SignRequest};
use shared_crypto::intent::{Intent, IntentMessage};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use sui_keys::key_derive::generate_new_key;
use sui_keys::keypair_file::{
    read_authority_keypair_from_file, read_keypair_from_file, write_authority_keypair_to_file,
    write_keypair_to_file,
};
use sui_keys::keystore::{AccountKeystore, Keystore};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{
    get_authority_key_pair, get_key_pair_from_rng, EncodeDecodeBase64, SignatureScheme, SuiKeyPair,
};
use sui_types::crypto::{DefaultHash, PublicKey, Signature};
use sui_types::multisig::{MultiSig, MultiSigPublicKey, ThresholdUnit, WeightUnit};
use sui_types::multisig_legacy::{MultiSigLegacy, MultiSigPublicKeyLegacy};
use sui_types::signature::{AuthenticatorTrait, GenericSignature, VerifyParams};
use sui_types::transaction::TransactionData;
use sui_types::zk_login_authenticator::ZkLoginAuthenticator;
use sui_types::zk_login_util::AddressParams;
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
    /// Creates a signature by leveraging AWS KMS. Pass in a key-id to leverage Amazon
    /// KMS to sign a message and the base64 pubkey.
    /// Generate PubKey from pem using MystenLabs/base64pemkey
    /// Any signature commits to a [struct IntentMessage] consisting of the Base64 encoded
    /// of the BCS serialized transaction bytes itself (the result of
    /// [transaction builder API](https://docs.sui.io/sui-jsonrpc) and its intent. If
    /// intent is absent, default will be used. See [struct IntentMessage] and [struct Intent]
    /// for more details.
    SignKMS {
        #[clap(long)]
        data: String,
        #[clap(long)]
        keyid: String,
        #[clap(long)]
        intent: Option<Intent>,
        #[clap(long)]
        base64pk: String,
    },
    /// Add a new key to sui.keystore using either the input mnemonic phrase or a private key (from the Wallet), the key scheme flag {ed25519 | secp256k1 | secp256r1}
    /// and an optional derivation path, default to m/44'/784'/0'/0'/0' for ed25519 or m/54'/784'/0'/0/0 for secp256k1
    /// or m/74'/784'/0'/0/0 for secp256r1. Supports mnemonic phrase of word length 12, 15, 18`, 21, 24.
    Import {
        input_string: String,
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

    /// Provides a list of participating signatures (`flag || sig || pk` encoded in Base64),
    /// threshold, a list of all public keys and a list of their weights that define the
    /// MultiSig address. Returns a valid MultiSig signature and its sender address. The
    /// result can be used as signature field for `sui client execute-signed-tx`. The sum
    /// of weights of all signatures must be >= the threshold.
    ///
    /// The order of `sigs` must be the same as the order of `pks`.
    /// e.g. for [pk1, pk2, pk3, pk4, pk5], [sig1, sig2, sig5] is valid, but
    /// [sig2, sig1, sig5] is invalid.
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

    MultiSigCombinePartialSigLegacy {
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        sigs: Vec<Signature>,
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        pks: Vec<PublicKey>,
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        weights: Vec<WeightUnit>,
        #[clap(long)]
        threshold: ThresholdUnit,
    },

    /// Given a Base64 encoded MultiSig signature, decode its components.
    /// If tx_bytes is passed in, verify the multisig.
    DecodeMultiSig {
        #[clap(long)]
        multisig: MultiSig,
        #[clap(long)]
        tx_bytes: Option<String>,
    },

    /// Given a Base64 encoded transaction bytes, decode its components.
    DecodeTxBytes {
        #[clap(long)]
        tx_bytes: String,
    },

    /// Converts a Base64 encoded string to its hexadecimal representation.
    Base64ToHex {
        base64: String,
    },

    /// Converts a hexadecimal string to its Base64 encoded representation.
    HexToBase64 {
        hex: String,
    },

    /// Converts a hexadecimal string to its corresponding bytes.
    HexToBytes {
        hex: String,
    },

    /// Converts an array of bytes to its hexadecimal string representation.
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

    /// Input the max epoch and generate a nonce with max_epoch,
    /// ephemeral_pubkey and a randomoness.
    ZkLogInPrepare {
        #[clap(long)]
        max_epoch: String,
    },

    /// Input the address seed and show the address based on iss,
    /// key_claim_name and address_sed.
    GenerateZkLoginAddress {
        #[clap(long)]
        address_seed: String,
    },

    /// Given the proof in string, public inputs in string, aux inputs in
    /// string and base64 encoded string user signature, serialize into
    /// a GenericSignature::ZkLoginAuthenticator.
    SerializeZkLoginAuthenticator {
        #[clap(long)]
        proof_str: String,
        #[clap(long)]
        public_inputs_str: String,
        #[clap(long)]
        aux_inputs_str: String,
        #[clap(long)]
        user_signature: String,
    },
}

impl KeyToolCommand {
    pub async fn execute(self, keystore: &mut Keystore) -> Result<(), anyhow::Error> {
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
                    let (address, kp, scheme, phrase) =
                        generate_new_key(key_scheme, derivation_path, word_length)?;
                    let file = format!("{address}.key");
                    write_keypair_to_file(&kp, &file)?;
                    println!(
                        "Created new keypair for address wrote to file path {:?} with scheme {:?}: [{address}]",
                        file, scheme
                    );
                    println!("Secret Recovery Phrase : [{phrase}]");
                }
            }
            KeyToolCommand::Show { file } => {
                let res = read_keypair_from_file(&file);
                match res {
                    Ok(keypair) => {
                        println!("Public Key: {}", keypair.public().encode_base64());
                        println!("Flag: {}", keypair.public().flag());
                        if let PublicKey::Ed25519(public_key) = keypair.public() {
                            let peer_id = anemo::PeerId(public_key.0);
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
                input_string,
                key_scheme,
                derivation_path,
            } => {
                // check if input is a private key -- should start with 0x
                if input_string.starts_with("0x") {
                    let bytes = Hex::decode(&input_string).map_err(|_| {
                        anyhow!("Private key is malformed. Importing private key failed.")
                    })?;
                    match key_scheme {
                        SignatureScheme::ED25519 => {
                            let kp = Ed25519KeyPair::from_bytes(&bytes).map_err(|_| anyhow!("Cannot decode ed25519 keypair from the private key. Importing private key failed."))?;
                            let skp = SuiKeyPair::Ed25519(kp);
                            let address: SuiAddress = Into::<SuiAddress>::into(&skp.public());
                            keystore.add_key(skp)?;
                            eprintln!("Private key imported successfully.");
                            println!("{address}")
                        }
                        _ => return Err(anyhow!(
                            "Only ed25519 signature scheme is supported for private keys at the moment."
                        ))
                    }
                } else {
                    let address = keystore.import_from_mnemonic(
                        &input_string,
                        key_scheme,
                        derivation_path,
                    )?;
                    eprintln!("Mnemonic imported successfully.");
                    println!("{address}")
                }
            }

            KeyToolCommand::Convert { value } => {
                let base64 = convert_string_to_base64(value)?;
                eprintln!("Successfully converted private key to base64.");
                println!("{base64}");
            }

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
            KeyToolCommand::SignKMS {
                data,
                keyid,
                intent,
                base64pk,
            } => {
                // Currently only supports secp256k1 keys
                let pk_owner = PublicKey::decode_base64(&base64pk)
                    .map_err(|e| anyhow!("Invalid base64 key: {:?}", e))?;
                let address_owner = SuiAddress::from(&pk_owner);
                println!("Address For Corresponding KMS Key: {}", address_owner);
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

                // Set up the KMS client in default region.
                let region: Region = Region::default();
                let kms: KmsClient = KmsClient::new(region);

                // Construct the signing request.
                let request: SignRequest = SignRequest {
                    key_id: keyid.to_string(),
                    message: digest.to_vec().into(),
                    message_type: Some("RAW".to_string()),
                    signing_algorithm: "ECDSA_SHA_256".to_string(),
                    ..Default::default()
                };

                // Sign the message, normalize the signature and then compacts it
                // serialize_compact is loaded as bytes for Secp256k1Sinaturere
                let response = kms.sign(request).await?;
                let sig_bytes_der = response
                    .signature
                    .map(|b| b.to_vec())
                    .expect("Requires Asymmetric Key Generated in KMS");

                let mut external_sig = Secp256k1Sig::from_der(&sig_bytes_der)?;
                external_sig.normalize_s();
                let sig_compact = external_sig.serialize_compact();

                let mut serialized_sig = vec![SignatureScheme::Secp256k1.flag()];
                serialized_sig.extend_from_slice(&sig_compact);
                serialized_sig.extend_from_slice(pk_owner.as_ref());
                let serialized_sig = Base64::encode(&serialized_sig);
                println!(
                    "Serialized signature (`flag || sig || pk` in Base64): {:?}",
                    serialized_sig
                );
                return Ok(());
            }
            KeyToolCommand::MultiSigAddress {
                threshold,
                pks,
                weights,
            } => {
                let multisig_pk = MultiSigPublicKey::new(pks.clone(), weights.clone(), threshold)?;
                let address: SuiAddress = (&multisig_pk).into();
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
                let address: SuiAddress = (&multisig_pk).into();
                let multisig = MultiSig::combine(sigs, multisig_pk)?;
                let generic_sig: GenericSignature = multisig.into();
                println!("MultiSig address: {address}");
                println!("MultiSig parsed: {:?}", generic_sig);
                println!("MultiSig serialized: {:?}", generic_sig.encode_base64());
            }

            KeyToolCommand::MultiSigCombinePartialSigLegacy {
                sigs,
                pks,
                weights,
                threshold,
            } => {
                let multisig_pk = MultiSigPublicKeyLegacy::new(pks, weights, threshold)?;
                let address: SuiAddress = (&multisig_pk).into();
                let multisig = MultiSigLegacy::combine(sigs, multisig_pk)?;
                let generic_sig: GenericSignature = multisig.into();
                println!("MultiSig address: {address}");
                println!("MultiSig legacy parsed: {:?}", generic_sig);
                println!(
                    "MultiSig legacy serialized: {:?}",
                    generic_sig.encode_base64()
                );
            }

            KeyToolCommand::DecodeMultiSig { multisig, tx_bytes } => {
                let pks = multisig.get_pk().pubkeys();
                let sigs = multisig.get_sigs();
                let bitmap = multisig.get_indices()?;
                println!(
                    "All pubkeys: {:?}, threshold: {:?}",
                    pks.iter()
                        .map(|(pk, w)| format!("{:?} - {:?}", pk.encode_base64(), w))
                        .collect::<Vec<String>>(),
                    multisig.get_pk().threshold()
                );
                println!("Participating signatures and pubkeys");
                println!(
                    " {0: ^45} | {1: ^45} | {2: ^6}",
                    "Public Key (Base64)", "Sig (Base64)", "Weight"
                );
                println!("{}", ["-"; 100].join(""));
                for (sig, i) in sigs.iter().zip(bitmap) {
                    let (pk, w) = pks
                        .get(i as usize)
                        .ok_or(anyhow!("Invalid public keys index".to_string()))?;
                    println!(
                        " {0: ^45} | {1: ^45} | {2: ^6}",
                        Base64::encode(sig.as_ref()),
                        pk.encode_base64(),
                        w
                    );
                }

                let author = SuiAddress::from(multisig.get_pk());
                println!("Multisig address: {:?}", author);

                if tx_bytes.is_some() {
                    let tx_bytes = Base64::decode(&tx_bytes.unwrap())
                        .map_err(|e| anyhow!("Invalid base64 tx bytes: {:?}", e))?;
                    let tx_data: TransactionData = bcs::from_bytes(&tx_bytes)?;
                    let res = GenericSignature::MultiSig(multisig).verify_authenticator(
                        &IntentMessage::new(Intent::sui_transaction(), tx_data),
                        author,
                        None,
                        &VerifyParams::default(),
                    );
                    println!("Verify multisig: {:?}", res);
                };
            }

            KeyToolCommand::DecodeTxBytes { tx_bytes } => {
                let tx_bytes = Base64::decode(&tx_bytes)
                    .map_err(|e| anyhow!("Invalid base64 key: {:?}", e))?;
                let tx_data: TransactionData = bcs::from_bytes(&tx_bytes)?;
                println!("Transaction data: {:?}", tx_data);
            }

            KeyToolCommand::ZkLogInPrepare { max_epoch } => {
                // todo: unhardcode keypair and jwt_randomness and max_epoch.
                let kp: Ed25519KeyPair = get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
                let skp = SuiKeyPair::Ed25519(kp.copy());
                println!("Ephemeral pubkey: {:?}", skp.public().encode_base64());
                println!("Ephemeral keypair: {:?}", skp.encode_base64());

                // Nonce is defined as the base64Url encoded of the poseidon hash of 4 inputs:
                // first half of eph_pubkey bytes in BigInt, second half, max_epoch, randomness.
                let bytes = kp.public().as_ref();
                let (first_half, second_half) = bytes.split_at(bytes.len() / 2);
                let first_bigint = BigInt::from_bytes_be(Sign::Plus, first_half);
                let second_bigint = BigInt::from_bytes_be(Sign::Plus, second_half);

                let mut poseidon = PoseidonWrapper::new();
                let first = Bn254Fr::from_str(&first_bigint.to_string()).unwrap();
                let second = Bn254Fr::from_str(&second_bigint.to_string()).unwrap();
                let max_epoch = Bn254Fr::from_str(max_epoch.as_str()).unwrap();
                let jwt_randomness = Bn254Fr::from_str(
                    "50683480294434968413708503290439057629605340925620961559740848568164438166",
                )
                .unwrap();
                let hash = poseidon.hash(vec![first, second, max_epoch, jwt_randomness])?;
                println!("Nonce: {:?}", hash.to_string());
            }

            KeyToolCommand::GenerateZkLoginAddress { address_seed } => {
                let mut hasher = DefaultHash::default();
                hasher.update([SignatureScheme::ZkLoginAuthenticator.flag()]);
                let address_params = AddressParams::new(
                    OAuthProvider::Google.get_config().0.to_owned(),
                    SupportedKeyClaim::Sub.to_string(),
                );
                println!("Address params: {:?}", address_params);
                hasher.update(bcs::to_bytes(&address_params).unwrap());
                hasher.update(big_int_str_to_bytes(&address_seed));
                let user_address = SuiAddress::from_bytes(hasher.finalize().digest)?;
                println!("Sui Address: {:?}", user_address);
            }

            KeyToolCommand::SerializeZkLoginAuthenticator {
                proof_str,
                public_inputs_str,
                aux_inputs_str,
                user_signature,
            } => {
                let authenticator = ZkLoginAuthenticator::new(
                    ZkLoginProof::from_json(&proof_str)?,
                    PublicInputs::from_json(&public_inputs_str)?,
                    AuxInputs::from_json(&aux_inputs_str)?,
                    Signature::from_str(&user_signature).map_err(|e| anyhow!(e))?,
                );
                let sig = GenericSignature::from(authenticator);
                println!(
                    "ZkLogin Authenticator Signature Serialized: {:?}",
                    sig.encode_base64()
                );
            }
        }
        Ok(())
    }
}

fn convert_string_to_base64(value: String) -> Result<String, anyhow::Error> {
    match Base64::decode(&value) {
        Ok(decoded) => {
            assert_eq!(decoded.len(), 33);
            let hex_encoded = Hex::encode(&decoded[1..]);
            info!("Wallet formatted private key: 0x{}", hex_encoded);
            Ok(hex_encoded)
        }
        Err(_) => match Hex::decode(&value) {
            Ok(decoded) => {
                assert_eq!(decoded.len(), 32);
                let mut res = Vec::new();
                res.extend_from_slice(&[SignatureScheme::ED25519.flag()]);
                res.extend_from_slice(&decoded);
                info!("Keystore formatted private key: {:?}", Base64::encode(&res));
                Ok(Base64::encode(&res))
            }
            Err(_) => {
                info!("Invalid private key format");
                Err(anyhow!("Invalid private key format"))
            }
        },
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
