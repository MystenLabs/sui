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
use json_to_table::{json_to_table, Orientation};
use num_bigint::{BigInt, Sign};
use rand::rngs::StdRng;
use rand::SeedableRng;
use rusoto_core::Region;
use rusoto_kms::{Kms, KmsClient, SignRequest};
use serde::Serialize;
use serde_json::json;
use shared_crypto::intent::{Intent, IntentMessage};
use std::fmt::{Debug, Display, Formatter};
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

use tabled::builder::Builder;
use tabled::settings::Rotate;
use tabled::settings::{object::Rows, Modify, Width};

#[cfg(test)]
#[path = "unit_tests/keytool_tests.rs"]
mod keytool_tests;

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum KeyToolCommand {
    /// Convert private key from wallet format (hex of 32 byte private key) to sui.keystore format
    /// (base64 of 33 byte flag || private key) or vice versa.
    Convert { value: String },
    /// Given a Base64 encoded transaction bytes, decode its components.
    DecodeTxBytes {
        #[clap(long)]
        tx_bytes: String,
    },
    /// Given a Base64 encoded MultiSig signature, decode its components.
    /// If tx_bytes is passed in, verify the multisig.
    DecodeMultiSig {
        #[clap(long)]
        multisig: MultiSig,
        #[clap(long)]
        tx_bytes: Option<String>,
    },
    /// Generate a new keypair with key scheme flag {ed25519 | secp256k1 | secp256r1}
    /// with optional derivation path, default to m/44'/784'/0'/0'/0' for ed25519 or
    /// m/54'/784'/0'/0/0 for secp256k1 or m/74'/784'/0'/0/0 for secp256r1. Word
    /// length can be { word12 | word15 | word18 | word21 | word24} default to word12
    /// if not specified.
    ///
    /// The keypair file is output to the current directory. The content of the file is
    /// a Base64 encoded string of 33-byte `flag || privkey`. Note: To generate and add keypair
    /// to sui.keystore, use `sui client new-address`).
    Generate {
        key_scheme: SignatureScheme,
        word_length: Option<String>,
        derivation_path: Option<DerivationPath>,
    },
    /// Input the address seed and show the address based on iss,
    /// key_claim_name and address_sed.
    GenerateZkLoginAddress {
        #[clap(long)]
        address_seed: String,
    },
    /// Add a new key to sui.keystore using either the input mnemonic phrase or a private key (from the Wallet),
    /// the key scheme flag {ed25519 | secp256k1 | secp256r1} and an optional derivation path,
    /// default to m/44'/784'/0'/0'/0' for ed25519 or m/54'/784'/0'/0/0 for secp256k1
    /// or m/74'/784'/0'/0/0 for secp256r1. Supports mnemonic phrase of word length 12, 15, 18`, 21, 24.
    Import {
        input_string: String,
        key_scheme: SignatureScheme,
        derivation_path: Option<DerivationPath>,
    },
    /// List all keys by its Sui address, Base64 encoded public key, key scheme name in
    /// sui.keystore.
    List,
    /// This reads the content at the provided file path. The accepted format can be
    /// [enum SuiKeyPair] (Base64 encoded of 33-byte `flag || privkey`) or `type AuthorityKeyPair`
    /// (Base64 encoded `privkey`). This prints out the account keypair as Base64 encoded `flag || privkey`,
    /// the network keypair, worker keypair, protocol keypair as Base64 encoded `privkey`.
    LoadKeypair { file: PathBuf },
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
    /// Read the content at the provided file path. The accepted format can be
    /// [enum SuiKeyPair] (Base64 encoded of 33-byte `flag || privkey`) or `type AuthorityKeyPair`
    /// (Base64 encoded `privkey`). It prints its Base64 encoded public key and the key scheme flag.
    Show { file: PathBuf },
    /// Create signature using the private key for for the given address in sui keystore.
    /// Any signature commits to a [struct IntentMessage] consisting of the Base64 encoded
    /// of the BCS serialized transaction bytes itself and its intent. If intent is absent,
    /// default will be used.
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
    /// of the BCS serialized transaction bytes itself and its intent. If intent is absent,
    /// default will be used.
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
    /// This takes [enum SuiKeyPair] of Base64 encoded of 33-byte `flag || privkey`). It
    /// outputs the keypair into a file at the current directory where the address is the filename,
    /// and prints out its Sui address, Base64 encoded public key, the key scheme, and the key scheme flag.
    Unpack { keypair: SuiKeyPair },
    /// Input the max epoch and generate a nonce with max_epoch,
    /// ephemeral_pubkey and a randomoness.
    ZkLogInPrepare {
        #[clap(long)]
        max_epoch: String,
    },
}

// Command Output types
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodedMultiSig {
    public_base64_key: String,
    sig_base64: String,
    weight: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodedMultiSigOutput {
    multisig_address: SuiAddress,
    participating_keys_signatures: Vec<DecodedMultiSig>,
    pub_keys: Vec<MultiSigOutput>,
    threshold: usize,
    transaction_result: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    sui_address: SuiAddress,
    public_base64_key: String,
    key_scheme: String,
    flag: u8,
    mnemonic: Option<String>,
    peer_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeypairData {
    account_keypair: String,
    network_keypair: Option<String>,
    worker_keypair: Option<String>,
    key_scheme: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiSigAddress {
    multisig_address: String,
    multisig: Vec<MultiSigOutput>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiSigCombinePartialSig {
    multisig_address: SuiAddress,
    multisig_parsed: GenericSignature,
    multisig_serialized: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiSigCombinePartialSigLegacyOutput {
    multisig_address: SuiAddress,
    multisig_legacy_parsed: GenericSignature,
    multisig_legacy_serialized: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiSigOutput {
    address: SuiAddress,
    public_base64_key: String,
    weight: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateKeyBase64 {
    base64: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerializedSig {
    serialized_sig_base64: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignData {
    sui_address: SuiAddress,
    // Base64 encoded string of serialized transaction data.
    raw_tx_data: String,
    // Intent struct used, see [struct Intent] for field definitions.
    intent: Intent,
    // Base64 encoded [struct IntentMessage] consisting of (intent || message)
    // where message can be `TransactionData` etc.
    raw_intent_msg: String,
    // Base64 encoded blake2b hash of the intent message, this is what the signature commits to.
    digest: String,
    // Base64 encoded `flag || signature || pubkey` for a complete
    // serialized Sui signature to be send for executing the transaction.
    sui_signature: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZkLogInPrepare {
    ephemeral_pubkey: String,
    ephemeral_keypair: String,
    nonce: String,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum CommandOutput {
    DecodeMultiSig(DecodedMultiSigOutput),
    DecodeTxBytes(TransactionData),
    Error(String),
    Generate(Key),
    GenerateZkLoginAddress(SuiAddress, AddressParams),
    Import(Key),
    List(Vec<Key>),
    LoadKeypair(KeypairData),
    MultiSigAddress(MultiSigAddress),
    MultiSigCombinePartialSig(MultiSigCombinePartialSig),
    MultiSigCombinePartialSigLegacy(MultiSigCombinePartialSigLegacyOutput),
    PrivateKeyBase64(PrivateKeyBase64),
    SerializeZkLoginAuthenticator(SerializedSig),
    Show(Key),
    Sign(SignData),
    SignKMS(SerializedSig),
    ZkLogInPrepare(ZkLogInPrepare),
}

impl KeyToolCommand {
    pub async fn execute(self, keystore: &mut Keystore) -> Result<CommandOutput, anyhow::Error> {
        let cmd_result = Ok(match self {
            KeyToolCommand::Convert { value } => {
                let result = convert_private_key_to_base64(value)?;
                CommandOutput::PrivateKeyBase64(PrivateKeyBase64 { base64: result })
            }

            KeyToolCommand::DecodeMultiSig { multisig, tx_bytes } => {
                let pks = multisig.get_pk().pubkeys();
                let sigs = multisig.get_sigs();
                let bitmap = multisig.get_indices()?;
                let address = SuiAddress::from(multisig.get_pk());

                let pub_keys = pks
                    .iter()
                    .map(|(pk, w)| MultiSigOutput {
                        address: (pk).into(),
                        public_base64_key: pk.encode_base64(),
                        weight: *w,
                    })
                    .collect::<Vec<MultiSigOutput>>();

                let threshold = *multisig.get_pk().threshold() as usize;

                let mut output = DecodedMultiSigOutput {
                    multisig_address: address,
                    participating_keys_signatures: vec![],
                    pub_keys,
                    threshold,
                    transaction_result: "".to_string(),
                };

                for (sig, i) in sigs.iter().zip(bitmap) {
                    let (pk, w) = pks
                        .get(i as usize)
                        .ok_or(anyhow!("Invalid public keys index".to_string()))?;
                    output.participating_keys_signatures.push(DecodedMultiSig {
                        public_base64_key: Base64::encode(sig.as_ref()).clone(),
                        sig_base64: pk.encode_base64().clone(),
                        weight: w.to_string(),
                    })
                }

                if tx_bytes.is_some() {
                    let tx_bytes = Base64::decode(&tx_bytes.unwrap())
                        .map_err(|e| anyhow!("Invalid base64 tx bytes: {:?}", e))?;
                    let tx_data: TransactionData = bcs::from_bytes(&tx_bytes)?;
                    let res = GenericSignature::MultiSig(multisig).verify_authenticator(
                        &IntentMessage::new(Intent::sui_transaction(), tx_data),
                        address,
                        None,
                        &VerifyParams::default(),
                    );
                    output.transaction_result = format!("{:?}", res);
                };

                CommandOutput::DecodeMultiSig(output)
            }

            KeyToolCommand::DecodeTxBytes { tx_bytes } => {
                let tx_bytes = Base64::decode(&tx_bytes)
                    .map_err(|e| anyhow!("Invalid base64 key: {:?}", e))?;
                let tx_data: TransactionData = bcs::from_bytes(&tx_bytes)?;
                CommandOutput::DecodeTxBytes(tx_data)
            }

            KeyToolCommand::Generate {
                key_scheme,
                derivation_path,
                word_length,
            } => match key_scheme {
                SignatureScheme::BLS12381 => {
                    let (sui_address, kp) = get_authority_key_pair();
                    let file_name = format!("bls-{sui_address}.key");
                    write_authority_keypair_to_file(&kp, file_name)?;
                    CommandOutput::Generate(Key {
                        sui_address,
                        public_base64_key: kp.public().encode_base64(),
                        key_scheme: key_scheme.to_string(),
                        flag: SignatureScheme::BLS12381.flag(),
                        mnemonic: None,
                        peer_id: None,
                    })
                }
                _ => {
                    let (sui_address, skp, _scheme, phrase) =
                        generate_new_key(key_scheme, derivation_path, word_length)?;
                    let file = format!("{sui_address}.key");
                    write_keypair_to_file(&skp, file)?;
                    let mut key = Key::from(&skp);
                    key.mnemonic = Some(phrase);
                    CommandOutput::Generate(key)
                }
            },

            KeyToolCommand::GenerateZkLoginAddress { address_seed } => {
                let mut hasher = DefaultHash::default();
                hasher.update([SignatureScheme::ZkLoginAuthenticator.flag()]);
                let address_params = AddressParams::new(
                    OAuthProvider::Google.get_config().0.to_owned(),
                    SupportedKeyClaim::Sub.to_string(),
                );
                hasher.update(bcs::to_bytes(&address_params).unwrap());
                hasher.update(big_int_str_to_bytes(&address_seed));
                let user_address = SuiAddress::from_bytes(hasher.finalize().digest)?;
                CommandOutput::GenerateZkLoginAddress(user_address, address_params)
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
                                let key = Key::from(&skp);
                                keystore.add_key(skp)?;
                                CommandOutput::Import(key)
                            }
                            _ => return Err(anyhow!(
                                "Only ed25519 signature scheme is supported for importing private keys at the moment."
                            ))
                        }
                } else {
                    let sui_address = keystore.import_from_mnemonic(
                        &input_string,
                        key_scheme,
                        derivation_path,
                    )?;
                    let skp = keystore.get_key(&sui_address)?;
                    let key = Key::from(skp);
                    CommandOutput::Import(key)
                }
            }

            KeyToolCommand::List => {
                let keys = keystore
                    .keys()
                    .into_iter()
                    .map(Key::from)
                    .collect::<Vec<_>>();

                CommandOutput::List(keys)
            }

            KeyToolCommand::LoadKeypair { file } => {
                let output = match read_keypair_from_file(&file) {
                    Ok(keypair) => {
                        // Account keypair is encoded with the key scheme flag {},
                        // and network and worker keypair are not.
                        let network_worker_keypair = match &keypair {
                            SuiKeyPair::Ed25519(kp) => kp.encode_base64(),
                            SuiKeyPair::Secp256k1(kp) => kp.encode_base64(),
                            SuiKeyPair::Secp256r1(kp) => kp.encode_base64(),
                        };
                        KeypairData {
                            account_keypair: keypair.encode_base64(),
                            network_keypair: Some(network_worker_keypair.clone()),
                            worker_keypair: Some(network_worker_keypair),
                            key_scheme: keypair.public().scheme().to_string(),
                        }
                    }
                    Err(_) => {
                        // Authority keypair file is not stored with the flag, it will try read as BLS keypair..
                        match read_authority_keypair_from_file(&file) {
                            Ok(keypair) => KeypairData {
                                account_keypair: keypair.encode_base64(),
                                network_keypair: None,
                                worker_keypair: None,
                                key_scheme: SignatureScheme::BLS12381.to_string(),
                            },
                            Err(e) => {
                                return Err(anyhow!(format!(
                                    "Failed to read keypair at path {:?} err: {:?}",
                                    file, e
                                )));
                            }
                        }
                    }
                };
                CommandOutput::LoadKeypair(output)
            }

            KeyToolCommand::MultiSigAddress {
                threshold,
                pks,
                weights,
            } => {
                let multisig_pk = MultiSigPublicKey::new(pks.clone(), weights.clone(), threshold)?;
                let address: SuiAddress = (&multisig_pk).into();
                let mut output = MultiSigAddress {
                    multisig_address: address.to_string(),
                    multisig: vec![],
                };

                for (pk, w) in pks.into_iter().zip(weights.into_iter()) {
                    output.multisig.push(MultiSigOutput {
                        address: Into::<SuiAddress>::into(&pk),
                        public_base64_key: pk.encode_base64(),
                        weight: w,
                    });
                }
                CommandOutput::MultiSigAddress(output)
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
                let multisig_serialized = generic_sig.encode_base64();
                CommandOutput::MultiSigCombinePartialSig(MultiSigCombinePartialSig {
                    multisig_address: address,
                    multisig_parsed: generic_sig,
                    multisig_serialized,
                })
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
                let multisig_legacy_serialized = generic_sig.encode_base64();

                CommandOutput::MultiSigCombinePartialSigLegacy(
                    MultiSigCombinePartialSigLegacyOutput {
                        multisig_address: address,
                        multisig_legacy_parsed: generic_sig,
                        multisig_legacy_serialized,
                    },
                )
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
                CommandOutput::SerializeZkLoginAuthenticator(SerializedSig {
                    serialized_sig_base64: sig.encode_base64(),
                })
            }

            KeyToolCommand::Show { file } => {
                let res = read_keypair_from_file(&file);
                match res {
                    Ok(skp) => {
                        let key = Key::from(&skp);
                        CommandOutput::Show(key)
                    }
                    Err(_) => match read_authority_keypair_from_file(&file) {
                        Ok(keypair) => {
                            let public_base64_key = keypair.public().encode_base64();
                            CommandOutput::Show(Key {
                                sui_address: (keypair.public()).into(),
                                public_base64_key,
                                key_scheme: SignatureScheme::BLS12381.to_string(),
                                flag: SignatureScheme::BLS12381.flag(),
                                peer_id: None,
                                mnemonic: None,
                            })
                        }
                        Err(e) => CommandOutput::Error(format!(
                            "Failed to read keypair at path {:?}, err: {e}",
                            file
                        )),
                    },
                }
            }

            KeyToolCommand::Sign {
                address,
                data,
                intent,
            } => {
                let intent = intent.unwrap_or_else(Intent::sui_transaction);
                let intent_clone = intent.clone();
                let msg: TransactionData =
                    bcs::from_bytes(&Base64::decode(&data).map_err(|e| {
                        anyhow!("Cannot deserialize data as TransactionData {:?}", e)
                    })?)?;
                let intent_msg = IntentMessage::new(intent, msg);
                let raw_intent_msg: String = Base64::encode(bcs::to_bytes(&intent_msg)?);
                let mut hasher = DefaultHash::default();
                hasher.update(bcs::to_bytes(&intent_msg)?);
                let digest = hasher.finalize().digest;
                let sui_signature =
                    keystore.sign_secure(&address, &intent_msg.value, intent_msg.intent)?;
                CommandOutput::Sign(SignData {
                    sui_address: address,
                    raw_tx_data: data,
                    intent: intent_clone,
                    raw_intent_msg,
                    digest: Base64::encode(digest),
                    sui_signature: sui_signature.encode_base64(),
                })
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
                info!("Address For Corresponding KMS Key: {}", address_owner);
                info!("Raw tx_bytes to execute: {}", data);
                let intent = intent.unwrap_or_else(Intent::sui_transaction);
                info!("Intent: {:?}", intent);
                let msg: TransactionData =
                    bcs::from_bytes(&Base64::decode(&data).map_err(|e| {
                        anyhow!("Cannot deserialize data as TransactionData {:?}", e)
                    })?)?;
                let intent_msg = IntentMessage::new(intent, msg);
                info!(
                    "Raw intent message: {:?}",
                    Base64::encode(bcs::to_bytes(&intent_msg)?)
                );
                let mut hasher = DefaultHash::default();
                hasher.update(bcs::to_bytes(&intent_msg)?);
                let digest = hasher.finalize().digest;
                info!("Digest to sign: {:?}", Base64::encode(digest));

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
                CommandOutput::SignKMS(SerializedSig {
                    serialized_sig_base64: serialized_sig,
                })
            }

            KeyToolCommand::Unpack { keypair } => {
                let key = Key::from(&keypair);
                let path_str = format!("{}.key", key.sui_address).to_lowercase();
                let path = Path::new(&path_str);
                let out_str = format!(
                    "address: {}\nkeypair: {}\nflag: {}",
                    key.sui_address,
                    keypair.encode_base64(),
                    key.flag
                );
                fs::write(path, out_str).unwrap();
                CommandOutput::Show(key)
            }

            KeyToolCommand::ZkLogInPrepare { max_epoch } => {
                // todo: unhardcode keypair and jwt_randomness and max_epoch.
                let kp: Ed25519KeyPair = get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
                let skp = SuiKeyPair::Ed25519(kp.copy());

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
                CommandOutput::ZkLogInPrepare(ZkLogInPrepare {
                    ephemeral_pubkey: skp.public().encode_base64(),
                    ephemeral_keypair: skp.encode_base64(),
                    nonce: hash.to_string(),
                })
            }
        });

        cmd_result
    }
}

impl From<&SuiKeyPair> for Key {
    fn from(skp: &SuiKeyPair) -> Self {
        Key::from(skp.public())
    }
}

impl From<PublicKey> for Key {
    fn from(key: PublicKey) -> Self {
        Key {
            sui_address: Into::<SuiAddress>::into(&key),
            public_base64_key: key.encode_base64(),
            key_scheme: key.scheme().to_string(),
            mnemonic: None,
            flag: key.flag(),
            peer_id: anemo_styling(&key),
        }
    }
}

impl Display for CommandOutput {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            // Sign needs to be manually built because we need to wrap the very long
            // rawTxData string and rawIntentMsg strings into multiple rows due to
            // their lengths, which we cannot do with a JsonTable
            CommandOutput::Sign(data) => {
                let intent_table = json_to_table(&json!(&data.intent))
                    .with(tabled::settings::Style::rounded().horizontals([]))
                    .to_string();

                let mut builder = Builder::default();
                builder
                    .set_header([
                        "suiSignature",
                        "digest",
                        "rawIntentMsg",
                        "intent",
                        "rawTxData",
                        "suiAddress",
                    ])
                    .push_record([
                        &data.sui_signature,
                        &data.digest,
                        &data.raw_intent_msg,
                        &intent_table,
                        &data.raw_tx_data,
                        &data.sui_address.to_string(),
                    ]);
                let mut table = builder.build();
                table.with(Rotate::Left);
                table.with(tabled::settings::Style::rounded().horizontals([]));
                table.with(Modify::new(Rows::new(0..)).with(Width::wrap(160).keep_words()));
                write!(formatter, "{}", table)
            }
            _ => {
                let json_obj = json![self];
                let mut table = json_to_table(&json_obj);
                let style = tabled::settings::Style::rounded().horizontals([]);
                table.with(style);
                table.array_orientation(Orientation::Column);
                write!(formatter, "{}", table)
            }
        }
    }
}

impl CommandOutput {
    pub fn print(&self, pretty: bool) {
        let line = if pretty {
            format!("{self}")
        } else {
            format!("{:?}", self)
        };
        // Log line by line
        for line in line.lines() {
            // Logs write to a file on the side.  Print to stdout and also log to file, for tests to pass.
            println!("{line}");
            info!("{line}")
        }
    }
}

// when --json flag is used, any output result is transformed into a JSON pretty string and sent to std output
impl Debug for CommandOutput {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match serde_json::to_string_pretty(self) {
            Ok(json) => write!(f, "{json}"),
            Err(err) => write!(f, "Error serializing JSON: {err}"),
        }
    }
}

fn convert_private_key_to_base64(value: String) -> Result<String, anyhow::Error> {
    match Base64::decode(&value) {
        Ok(decoded) => {
            if decoded.len() != 33 {
                return Err(anyhow!(format!("Private key is malformed and cannot base64 decode it. Fed 33 length but got {}", decoded.len())));
            }
            Ok(Hex::encode(&decoded[1..]))
        }
        Err(_) => match Hex::decode(&value) {
            Ok(decoded) => {
                if decoded.len() != 32 {
                    return Err(anyhow!(format!("Private key is malformed and cannot hex decode it. Expected 32 length but got {}", decoded.len())));
                }
                let mut res = Vec::new();
                res.extend_from_slice(&[SignatureScheme::ED25519.flag()]);
                res.extend_from_slice(&decoded);
                Ok(Base64::encode(&res))
            }
            Err(_) => Err(anyhow!("Invalid private key format".to_string())),
        },
    }
}

fn anemo_styling(pk: &PublicKey) -> Option<String> {
    if let PublicKey::Ed25519(public_key) = pk {
        Some(anemo::PeerId(public_key.0).to_string())
    } else {
        None
    }
}
