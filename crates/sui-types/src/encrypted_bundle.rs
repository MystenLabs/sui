// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Encrypted transaction bundles.
//!
//! A bundle holds a list of transactions encrypted to an identity under the random beacon's
//! threshold public key. The validators release the identity's key only after the bundle is
//! committed, so the plaintext cannot influence its own position in the order.
//!
//! Encryption uses a Boneh-Franklin IBE KEM over BLS12-381 with the master public key in G2
//! and user secret keys in G1. The user secret key for an identity equals the threshold BLS
//! signature on the identity message (`ThresholdBls12381MinSig`), so the beacon's key shares
//! produce it.
//!
//! The identity contains a one-time Ed25519 verification key, and the bundle is signed with
//! the matching signing key (the Canetti-Halevi-Katz transform). Validators verify the
//! signature before they release key shares. A party that alters any field of the bundle must
//! either forge the signature or change the verification key, and a different verification
//! key gives a different identity whose key does not decrypt the original bundle.

use crate::base_types::AuthorityName;
use crate::committee::EpochId;
use crate::crypto::RandomnessSignature;
use crate::transaction::Transaction;
use fastcrypto::aes::{Aes256GcmSiv, AesKey, AuthenticatedCipher};
use fastcrypto::ed25519::{Ed25519KeyPair, Ed25519PublicKey, Ed25519Signature};
use fastcrypto::groups::bls12381::{G1Element, G2Element};
use fastcrypto::groups::{GroupElement, HashToGroupElement, Pairing, Scalar as _};
use fastcrypto::hash::{Blake2b256, HashFunction, Sha3_256};
use fastcrypto::serde_helpers::ToFromByteArray;
use fastcrypto::traits::{AllowedRng, KeyPair, Signer, ToFromBytes, VerifyingKey};
use fastcrypto_threshold::tbls::ThresholdBls;
use fastcrypto_threshold::types::ThresholdBls12381MinSig;
use serde::{Deserialize, Serialize};

/// Prefix of every identity message. It must not share a prefix with
/// `RandomnessRound::signature_message`, or a bundle could make validators sign a future
/// randomness round.
pub const ID_TAG: &[u8] = b"sui encrypted bundle id v1";
const SIGNATURE_TAG: &[u8] = b"sui encrypted bundle signature v1";
const KDF_TAG: &[u8] = b"sui encrypted bundle kdf v1";

/// AES-256-GCM-SIV takes a 96-bit nonce. Each key encrypts one message, so the nonce is fixed.
const DEM_NONCE: [u8; 12] = [0; 12];

/// The random beacon's threshold public key for one epoch (`dkg_output.vss_pk.c0()`).
pub type BeaconPublicKey = G2Element;
/// The IBE user secret key for a bundle identity. It is a threshold BLS signature on the
/// identity message.
pub type BundleKey = RandomnessSignature;

pub type EncryptedBundleId = [u8; 32];

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EncryptedBundleError {
    #[error("malformed bundle field: {0}")]
    Malformed(&'static str),
    #[error("bundle signature does not verify")]
    InvalidSignature,
    #[error("bundle key does not verify against the beacon public key")]
    InvalidKey,
    #[error("bundle ciphertext does not decrypt")]
    DecryptionFailed,
    #[error("bundle plaintext does not deserialize")]
    InvalidPlaintext,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EncryptedBundle {
    pub epoch: EpochId,
    /// The validator that includes the bundle in its consensus block.
    pub author: AuthorityName,
    /// One-time Ed25519 verification key (32 bytes).
    pub vk: Vec<u8>,
    /// IBE encapsulation `r * g2` (96 bytes, compressed G2).
    pub nonce: Vec<u8>,
    /// AES-256-GCM-SIV encryption of `bcs(Vec<Transaction>)` with the identity message as AAD.
    pub ciphertext: Vec<u8>,
    /// Ed25519 signature by `vk` over `signing_message()` (64 bytes).
    pub signature: Vec<u8>,
}

/// What a validator operator submits to its node: the bundle and the signed transaction that
/// pays the bundle fee. The node sequences both in one consensus block.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedBundleSubmission {
    pub bundle: EncryptedBundle,
    pub fee_tx: Transaction,
}

/// What a node reports about the beacon key, so that the operator can configure the enclave.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeaconKeyInfo {
    pub epoch: EpochId,
    pub authority: AuthorityName,
    /// Compressed G2 element (96 bytes).
    pub beacon_public_key: Vec<u8>,
}

impl BeaconKeyInfo {
    pub fn public_key(&self) -> Result<BeaconPublicKey, EncryptedBundleError> {
        let bytes: [u8; 96] = self
            .beacon_public_key
            .as_slice()
            .try_into()
            .map_err(|_| EncryptedBundleError::Malformed("beacon public key"))?;
        G2Element::from_byte_array(&bytes)
            .map_err(|_| EncryptedBundleError::Malformed("beacon public key"))
    }
}

/// The decrypted content of a bundle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundlePlaintext {
    pub transactions: Vec<Transaction>,
}

impl EncryptedBundle {
    /// Encrypts `plaintext` to a fresh identity and signs the result with a one-time key.
    pub fn seal<R: AllowedRng>(
        rng: &mut R,
        beacon_pk: &BeaconPublicKey,
        epoch: EpochId,
        author: AuthorityName,
        plaintext: &BundlePlaintext,
    ) -> Self {
        let one_time_key = Ed25519KeyPair::generate(rng);
        let vk = one_time_key.public().as_bytes().to_vec();
        let id_message = id_message(epoch, &author, &vk);

        let r = fastcrypto::groups::bls12381::Scalar::rand(rng);
        let nonce = G2Element::generator() * r;
        let shared = (hash_id(&id_message) * r).pairing(beacon_pk);
        let dem_key = kdf(&shared.to_byte_array(), &nonce, &id_message);

        let plaintext_bytes = bcs::to_bytes(plaintext).expect("serialization should not fail");
        let ciphertext =
            dem(&dem_key).encrypt_authenticated(&dem_nonce(), &id_message, &plaintext_bytes);

        let mut bundle = Self {
            epoch,
            author,
            vk,
            nonce: nonce.to_byte_array().to_vec(),
            ciphertext,
            signature: vec![],
        };
        let signature: Ed25519Signature = one_time_key.sign(&bundle.signing_message());
        bundle.signature = signature.as_bytes().to_vec();
        bundle
    }

    /// The message whose threshold signature is the key of this bundle.
    pub fn id_message(&self) -> Vec<u8> {
        id_message(self.epoch, &self.author, &self.vk)
    }

    /// A 32-byte digest of the identity message, used for deduplication.
    pub fn id(&self) -> EncryptedBundleId {
        Blake2b256::digest(self.id_message()).digest
    }

    /// The message signed by the one-time key. It covers every field except the signature.
    pub fn signing_message(&self) -> Vec<u8> {
        let mut message = SIGNATURE_TAG.to_vec();
        message.extend(
            bcs::to_bytes(&(
                self.epoch,
                &self.author,
                &self.vk,
                &self.nonce,
                &self.ciphertext,
            ))
            .expect("serialization should not fail"),
        );
        message
    }

    /// Checks field lengths and the one-time signature. Validators run this before they
    /// release key shares for the bundle.
    pub fn verify_signature(&self) -> Result<(), EncryptedBundleError> {
        let vk = Ed25519PublicKey::from_bytes(&self.vk)
            .map_err(|_| EncryptedBundleError::Malformed("vk"))?;
        self.parse_nonce()?;
        let signature = Ed25519Signature::from_bytes(&self.signature)
            .map_err(|_| EncryptedBundleError::Malformed("signature"))?;
        vk.verify(&self.signing_message(), &signature)
            .map_err(|_| EncryptedBundleError::InvalidSignature)
    }

    /// Checks that `key` is the IBE user secret key of this bundle under `beacon_pk`.
    pub fn verify_key(
        &self,
        beacon_pk: &BeaconPublicKey,
        key: &BundleKey,
    ) -> Result<(), EncryptedBundleError> {
        ThresholdBls12381MinSig::verify(beacon_pk, &self.id_message(), key)
            .map_err(|_| EncryptedBundleError::InvalidKey)
    }

    /// Decrypts the bundle with its key. The caller verifies the key first.
    pub fn open(&self, key: &BundleKey) -> Result<BundlePlaintext, EncryptedBundleError> {
        let nonce = self.parse_nonce()?;
        let id_message = self.id_message();
        let shared = key.pairing(&nonce);
        let dem_key = kdf(&shared.to_byte_array(), &nonce, &id_message);
        let plaintext_bytes = dem(&dem_key)
            .decrypt_authenticated(&dem_nonce(), &id_message, &self.ciphertext)
            .map_err(|_| EncryptedBundleError::DecryptionFailed)?;
        bcs::from_bytes(&plaintext_bytes).map_err(|_| EncryptedBundleError::InvalidPlaintext)
    }

    fn parse_nonce(&self) -> Result<G2Element, EncryptedBundleError> {
        let bytes: [u8; 96] = self
            .nonce
            .as_slice()
            .try_into()
            .map_err(|_| EncryptedBundleError::Malformed("nonce"))?;
        let nonce = G2Element::from_byte_array(&bytes)
            .map_err(|_| EncryptedBundleError::Malformed("nonce"))?;
        if nonce == G2Element::zero() {
            return Err(EncryptedBundleError::Malformed("nonce"));
        }
        Ok(nonce)
    }
}

fn id_message(epoch: EpochId, author: &AuthorityName, vk: &[u8]) -> Vec<u8> {
    let mut message = ID_TAG.to_vec();
    message.extend(bcs::to_bytes(&(epoch, author, vk)).expect("serialization should not fail"));
    message
}

/// The IBE identity hash. It equals the hash that `ThresholdBls12381MinSig` signs, so a
/// threshold signature on `id_message` is the IBE user secret key.
fn hash_id(id_message: &[u8]) -> G1Element {
    G1Element::hash_to_group_element(id_message)
}

fn kdf(shared: &[u8], nonce: &G2Element, id_message: &[u8]) -> [u8; 32] {
    let mut hash = Sha3_256::new();
    hash.update(KDF_TAG);
    hash.update(shared);
    hash.update(nonce.to_byte_array());
    hash.update(id_message);
    hash.finalize().digest
}

fn dem(key: &[u8; 32]) -> Aes256GcmSiv {
    Aes256GcmSiv::new(AesKey::from_bytes(key).expect("32-byte key"))
}

fn dem_nonce() -> <Aes256GcmSiv as AuthenticatedCipher>::IVType {
    <Aes256GcmSiv as AuthenticatedCipher>::IVType::from_bytes(&DEM_NONCE).expect("12-byte nonce")
}

#[cfg(test)]
#[path = "unit_tests/encrypted_bundle_tests.rs"]
mod encrypted_bundle_tests;
