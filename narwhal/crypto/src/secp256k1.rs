// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    pubkey_bytes::PublicKeyBytes,
    serde_helpers::keypair_decode_base64,
    traits::{Authenticator, EncodeDecodeBase64, KeyPair, SigningKey, ToFromBytes, VerifyingKey},
};
use base64ct::{Base64, Encoding};
use once_cell::sync::OnceCell;
use rust_secp256k1::{constants, rand::rngs::OsRng, Message, PublicKey, Secp256k1, SecretKey};
use serde::{de, Deserialize, Serialize};
use signature::{Signature, Signer, Verifier};
use std::fmt::{self, Debug, Display};
use std::str::FromStr;

#[readonly::make]
#[derive(Debug, Clone)]
pub struct Secp256k1PublicKey {
    pub pubkey: PublicKey,
    pub bytes: OnceCell<[u8; constants::PUBLIC_KEY_SIZE]>,
}

pub type Secp256k1PublicKeyBytes =
    PublicKeyBytes<Secp256k1PublicKey, { Secp256k1PublicKey::LENGTH }>;

#[readonly::make]
#[derive(Debug)]
pub struct Secp256k1PrivateKey {
    pub privkey: SecretKey,
    pub bytes: OnceCell<[u8; constants::SECRET_KEY_SIZE]>,
}

// Compact signature followed by one extra byte for recover id, used to recover public key from signature.
pub const RECOVERABLE_SIGNATURE_SIZE: usize = constants::COMPACT_SIGNATURE_SIZE + 1;

#[readonly::make]
#[derive(Debug, Clone)]
pub struct Secp256k1Signature {
    pub sig: rust_secp256k1::ecdsa::RecoverableSignature,
    pub bytes: OnceCell<[u8; RECOVERABLE_SIGNATURE_SIZE]>,
}

impl std::hash::Hash for Secp256k1PublicKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

impl PartialOrd for Secp256k1PublicKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.as_ref().partial_cmp(other.as_ref())
    }
}

impl Ord for Secp256k1PublicKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_ref().cmp(other.as_ref())
    }
}

impl PartialEq for Secp256k1PublicKey {
    fn eq(&self, other: &Self) -> bool {
        self.pubkey == other.pubkey
    }
}

impl Eq for Secp256k1PublicKey {}

impl VerifyingKey for Secp256k1PublicKey {
    type PrivKey = Secp256k1PrivateKey;
    type Sig = Secp256k1Signature;
    const LENGTH: usize = constants::PUBLIC_KEY_SIZE;
}

impl Verifier<Secp256k1Signature> for Secp256k1PublicKey {
    fn verify(&self, msg: &[u8], signature: &Secp256k1Signature) -> Result<(), signature::Error> {
        // k256 defaults to keccak256 as digest to hash message for sign/verify, thus use this hash function to match in proptest.
        #[cfg(test)]
        let message =
            Message::from_slice(<sha3::Keccak256 as sha3::digest::Digest>::digest(msg).as_slice())
                .unwrap();

        #[cfg(not(test))]
        let message = Message::from_hashed_data::<rust_secp256k1::hashes::sha256::Hash>(msg);

        let vrfy = Secp256k1::verification_only();
        vrfy.verify_ecdsa(&message, &signature.sig.to_standard(), &self.pubkey)
            .map_err(|_e| signature::Error::new())
    }
}

impl AsRef<[u8]> for Secp256k1PublicKey {
    fn as_ref(&self) -> &[u8] {
        self.bytes
            .get_or_try_init::<_, eyre::Report>(|| {
                Ok(self
                    .pubkey
                    .serialize()
                    .as_slice()
                    .try_into()
                    .expect("wrong length"))
            })
            .expect("OnceCell invariant violated")
    }
}

impl ToFromBytes for Secp256k1PublicKey {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        match PublicKey::from_slice(bytes) {
            Ok(pubkey) => Ok(Secp256k1PublicKey {
                pubkey,
                bytes: OnceCell::new(),
            }),
            Err(_) => Err(signature::Error::new()),
        }
    }
}

impl Default for Secp256k1PublicKey {
    fn default() -> Self {
        Secp256k1PublicKey::from_bytes(&[0u8; constants::PUBLIC_KEY_SIZE]).unwrap()
    }
}

impl Display for Secp256k1PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", Base64::encode_string(self.as_ref()))
    }
}

// There is a strong requirement for this specific impl. in Fab benchmarks
impl Serialize for Secp256k1PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.encode_base64())
    }
}

impl<'de> Deserialize<'de> for Secp256k1PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s = <String as serde::Deserialize>::deserialize(deserializer)?;
        let value = Self::decode_base64(&s).map_err(|e| de::Error::custom(e.to_string()))?;
        Ok(value)
    }
}

impl<'a> From<&'a Secp256k1PrivateKey> for Secp256k1PublicKey {
    fn from(secret: &'a Secp256k1PrivateKey) -> Self {
        let secp = Secp256k1::new();
        Secp256k1PublicKey {
            pubkey: secret.privkey.public_key(&secp),
            bytes: OnceCell::new(),
        }
    }
}

impl SigningKey for Secp256k1PrivateKey {
    type PubKey = Secp256k1PublicKey;
    type Sig = Secp256k1Signature;
    const LENGTH: usize = constants::SECRET_KEY_SIZE;
}

impl ToFromBytes for Secp256k1PrivateKey {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        match SecretKey::from_slice(bytes) {
            Ok(privkey) => Ok(Secp256k1PrivateKey {
                privkey,
                bytes: OnceCell::new(),
            }),
            Err(_) => Err(signature::Error::new()),
        }
    }
}

// There is a strong requirement for this specific impl. in Fab benchmarks
impl Serialize for Secp256k1PrivateKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.encode_base64())
    }
}

// There is a strong requirement for this specific impl. in Fab benchmarks
impl<'de> Deserialize<'de> for Secp256k1PrivateKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let value = Self::decode_base64(&s).map_err(|e| de::Error::custom(e.to_string()))?;
        Ok(value)
    }
}

impl AsRef<[u8]> for Secp256k1PrivateKey {
    fn as_ref(&self) -> &[u8] {
        self.bytes
            .get_or_try_init::<_, eyre::Report>(|| Ok(self.privkey.secret_bytes()))
            .expect("OnceCell invariant violated")
    }
}

impl Serialize for Secp256k1Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.as_ref().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Secp256k1Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let data: Vec<u8> = Vec::deserialize(deserializer)?;
        <Secp256k1Signature as signature::Signature>::from_bytes(&data)
            .map_err(|e| de::Error::custom(e.to_string()))
    }
}

impl Signature for Secp256k1Signature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        let recovery_id = rust_secp256k1::ecdsa::RecoveryId::from_i32(bytes[64] as i32).unwrap();
        match rust_secp256k1::ecdsa::RecoverableSignature::from_compact(&bytes[..64], recovery_id) {
            Ok(sig) => Ok(Secp256k1Signature {
                sig,
                bytes: OnceCell::new(),
            }),
            Err(_) => Err(signature::Error::new()),
        }
    }
}

impl Authenticator for Secp256k1Signature {
    type PubKey = Secp256k1PublicKey;
    type PrivKey = Secp256k1PrivateKey;
    const LENGTH: usize = RECOVERABLE_SIGNATURE_SIZE;
}

impl AsRef<[u8]> for Secp256k1Signature {
    fn as_ref(&self) -> &[u8] {
        let mut bytes = [0u8; RECOVERABLE_SIGNATURE_SIZE];
        let (recovery_id, sig) = self.sig.serialize_compact();
        bytes[..64].copy_from_slice(&sig);
        bytes[64] = recovery_id.to_i32() as u8;
        self.bytes
            .get_or_try_init::<_, eyre::Report>(|| Ok(bytes))
            .expect("OnceCell invariant violated")
    }
}

impl std::hash::Hash for Secp256k1Signature {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

impl PartialEq for Secp256k1Signature {
    fn eq(&self, other: &Self) -> bool {
        self.sig == other.sig
    }
}

impl Eq for Secp256k1Signature {}

impl Display for Secp256k1Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", Base64::encode_string(self.as_ref()))
    }
}

impl Default for Secp256k1Signature {
    fn default() -> Self {
        <Secp256k1Signature as Signature>::from_bytes(&[1u8; RECOVERABLE_SIGNATURE_SIZE]).unwrap()
    }
}

// There is a strong requirement for this specific impl. in Fab benchmarks
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")] // necessary so as not to deser under a != type
pub struct Secp256k1KeyPair {
    pub name: Secp256k1PublicKey,
    pub secret: Secp256k1PrivateKey,
}

impl EncodeDecodeBase64 for Secp256k1KeyPair {
    fn decode_base64(value: &str) -> Result<Self, eyre::Report> {
        keypair_decode_base64(value)
    }

    fn encode_base64(&self) -> String {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(self.secret.as_ref());
        bytes.extend_from_slice(self.name.as_ref());
        base64ct::Base64::encode_string(&bytes[..])
    }
}

impl KeyPair for Secp256k1KeyPair {
    type PubKey = Secp256k1PublicKey;
    type PrivKey = Secp256k1PrivateKey;
    type Sig = Secp256k1Signature;

    fn public(&'_ self) -> &'_ Self::PubKey {
        &self.name
    }

    fn private(self) -> Self::PrivKey {
        self.secret
    }

    fn generate<R: rand::CryptoRng + rand::RngCore>(_rng: &mut R) -> Self {
        let secp = Secp256k1::new();
        // TODO: use param rng instead of generate a fresh OsRng when rand is upgraded to match. https://github.com/MystenLabs/narwhal/issues/544
        let (privkey, pubkey) = secp.generate_keypair(&mut OsRng);

        Secp256k1KeyPair {
            name: Secp256k1PublicKey {
                pubkey,
                bytes: OnceCell::new(),
            },
            secret: Secp256k1PrivateKey {
                privkey,
                bytes: OnceCell::new(),
            },
        }
    }

    #[cfg(feature = "copy_key")]
    fn copy(&self) -> Self {
        Secp256k1KeyPair {
            name: self.name.clone(),
            secret: Secp256k1PrivateKey::from_bytes(self.secret.as_ref()).unwrap(),
        }
    }
}

impl FromStr for Secp256k1KeyPair {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let kp = Self::decode_base64(s).map_err(|e| anyhow::anyhow!("{}", e.to_string()))?;
        Ok(kp)
    }
}

impl Signer<Secp256k1Signature> for Secp256k1KeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<Secp256k1Signature, signature::Error> {
        let secp = Secp256k1::signing_only();
        #[cfg(test)]
        let message =
            Message::from_slice(<sha3::Keccak256 as sha3::digest::Digest>::digest(msg).as_slice())
                .unwrap();

        #[cfg(not(test))]
        let message = Message::from_hashed_data::<rust_secp256k1::hashes::sha256::Hash>(msg);

        Ok(Secp256k1Signature {
            sig: secp.sign_ecdsa_recoverable(&message, &self.secret.privkey),
            bytes: OnceCell::new(),
        })
    }
}

impl TryFrom<Secp256k1PublicKeyBytes> for Secp256k1PublicKey {
    type Error = signature::Error;

    fn try_from(bytes: Secp256k1PublicKeyBytes) -> Result<Secp256k1PublicKey, Self::Error> {
        Secp256k1PublicKey::from_bytes(bytes.as_ref()).map_err(|_| Self::Error::new())
    }
}

impl From<&Secp256k1PublicKey> for Secp256k1PublicKeyBytes {
    fn from(pk: &Secp256k1PublicKey) -> Self {
        Secp256k1PublicKeyBytes::from_bytes(pk.as_ref()).unwrap()
    }
}

impl From<Secp256k1PrivateKey> for Secp256k1KeyPair {
    fn from(secret: Secp256k1PrivateKey) -> Self {
        let name = Secp256k1PublicKey::from(&secret);
        Secp256k1KeyPair { name, secret }
    }
}
