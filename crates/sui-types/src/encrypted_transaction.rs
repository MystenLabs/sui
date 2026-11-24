// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Encrypted transactions.
//!
//! A user encrypts the commands and inputs of a programmable transaction to an identity under
//! the random beacon's threshold public key. The sender, the gas payment and the signature stay
//! in plaintext, so validators vote on the transaction as on any other. Validators release the
//! identity's key only after the transaction is committed, so the plaintext cannot influence
//! its own position in the order.
//!
//! Encryption uses a Boneh-Franklin IBE KEM over BLS12-381 with the master public key in G2
//! and user secret keys in G1. The user secret key for an identity equals the threshold BLS
//! signature on the identity message (`ThresholdBls12381MinSig`), so the beacon's key shares
//! produce it.
//!
//! The identity contains the sender address. Validators release key shares only for a
//! committed transaction whose sender equals the identity's sender, and only the sender can
//! sign such a transaction. A party that copies the ciphertext into its own transaction gets
//! the key of a different identity, which does not decrypt the ciphertext.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::base_types::{SuiAddress, TransactionDigest};
use crate::committee::EpochId;
use crate::crypto::RandomnessSignature;
use crate::transaction::ProgrammableTransaction;
use fastcrypto::aes::{Aes256GcmSiv, AesKey, AuthenticatedCipher};
use fastcrypto::groups::bls12381::{G1Element, G2Element};
use fastcrypto::groups::{GroupElement, HashToGroupElement, Pairing, Scalar as _};
use fastcrypto::hash::{HashFunction, Sha3_256};
use fastcrypto::serde_helpers::ToFromByteArray;
use fastcrypto::traits::{AllowedRng, ToFromBytes};
use fastcrypto_threshold::tbls::ThresholdBls;
use fastcrypto_threshold::types::ThresholdBls12381MinSig;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Prefix of every identity message. It must not share a prefix with
/// `RandomnessRound::signature_message`, or a transaction could make validators sign a future
/// randomness round.
pub const ID_TAG: &[u8] = b"sui encrypted transaction id v1";
const KDF_TAG: &[u8] = b"sui encrypted transaction kdf v1";

/// AES-256-GCM-SIV takes a 96-bit nonce. Each key encrypts one message, so the nonce is fixed.
const DEM_NONCE: [u8; 12] = [0; 12];

pub const SALT_LENGTH: usize = 32;

/// The smallest computation charge in gas units (the first computation bucket). Every executed
/// transaction pays at least this many units times its gas price, so a gas price of
/// `fee / MIN_COMPUTATION_UNITS` pays the inclusion fee even if the transaction runs no command.
pub const MIN_COMPUTATION_UNITS: u64 = 1_000;

/// The random beacon's threshold public key for one epoch (`dkg_output.vss_pk.c0()`).
pub type BeaconPublicKey = G2Element;
/// The IBE user secret key for an identity. It is a threshold BLS signature on the identity
/// message.
pub type EncryptedTransactionKey = RandomnessSignature;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EncryptedTransactionError {
    #[error("malformed encrypted transaction field: {0}")]
    Malformed(&'static str),
    #[error("key does not verify against the beacon public key")]
    InvalidKey,
    #[error("ciphertext does not decrypt")]
    DecryptionFailed,
    #[error("plaintext does not deserialize")]
    InvalidPlaintext,
}

/// The kind of a transaction whose programmable transaction is encrypted.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptedTransaction {
    /// The epoch whose beacon key encrypts the transaction. The transaction is valid only in
    /// this epoch.
    pub epoch: EpochId,
    /// Chosen by the sender so that each transaction has its own identity (32 bytes).
    pub salt: Vec<u8>,
    /// IBE encapsulation `r * g2` (96 bytes, compressed G2).
    pub nonce: Vec<u8>,
    /// AES-256-GCM-SIV encryption of `bcs(ProgrammableTransaction)` with the identity message
    /// as associated data.
    pub ciphertext: Vec<u8>,
    /// The programmable transaction that this transaction executes, set after the key is
    /// derived. It is an empty programmable transaction when the ciphertext does not decrypt or
    /// the decrypted transaction fails validation. It is not serialized, so it is not part of
    /// the transaction digest.
    #[serde(skip)]
    decrypted: Option<Arc<ProgrammableTransaction>>,
}

impl PartialEq for EncryptedTransaction {
    fn eq(&self, other: &Self) -> bool {
        self.epoch == other.epoch
            && self.salt == other.salt
            && self.nonce == other.nonce
            && self.ciphertext == other.ciphertext
    }
}

impl Eq for EncryptedTransaction {}

impl Hash for EncryptedTransaction {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.epoch.hash(state);
        self.salt.hash(state);
        self.nonce.hash(state);
        self.ciphertext.hash(state);
    }
}

/// The key of an encrypted transaction that a commit schedules, recorded in the commit prologue
/// so that nodes that replay checkpoints can decrypt the transaction.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EncryptedTransactionResolution {
    pub transaction_digest: TransactionDigest,
    /// Compressed G1 key (48 bytes).
    pub key: Vec<u8>,
    /// False when the ciphertext does not decrypt or the decrypted transaction fails
    /// validation. The transaction then runs no command and only pays gas.
    pub executes: bool,
}

impl EncryptedTransaction {
    /// Encrypts `transaction` to a fresh identity of `sender` in `epoch`.
    pub fn seal<R: AllowedRng>(
        rng: &mut R,
        beacon_pk: &BeaconPublicKey,
        epoch: EpochId,
        sender: SuiAddress,
        transaction: &ProgrammableTransaction,
    ) -> Self {
        let mut salt = vec![0; SALT_LENGTH];
        rng.fill_bytes(&mut salt);
        let id_message = id_message(epoch, &sender, &salt);

        let r = fastcrypto::groups::bls12381::Scalar::rand(rng);
        let nonce = G2Element::generator() * r;
        let shared = (hash_id(&id_message) * r).pairing(beacon_pk);
        let dem_key = kdf(&shared.to_byte_array(), &nonce, &id_message);

        let plaintext = bcs::to_bytes(transaction).expect("serialization should not fail");
        let ciphertext = dem(&dem_key).encrypt_authenticated(&dem_nonce(), &id_message, &plaintext);

        Self {
            epoch,
            salt,
            nonce: nonce.to_byte_array().to_vec(),
            ciphertext,
            decrypted: None,
        }
    }

    /// The message whose threshold signature is the key of this transaction.
    pub fn id_message(&self, sender: &SuiAddress) -> Vec<u8> {
        id_message(self.epoch, sender, &self.salt)
    }

    /// Checks field lengths. Validators run this when they vote on the transaction.
    pub fn check_format(&self) -> Result<(), EncryptedTransactionError> {
        if self.salt.len() != SALT_LENGTH {
            return Err(EncryptedTransactionError::Malformed("salt"));
        }
        self.parse_nonce()?;
        Ok(())
    }

    /// Checks that `key` is the IBE user secret key of this transaction under `beacon_pk`.
    pub fn verify_key(
        &self,
        beacon_pk: &BeaconPublicKey,
        sender: &SuiAddress,
        key: &EncryptedTransactionKey,
    ) -> Result<(), EncryptedTransactionError> {
        ThresholdBls12381MinSig::verify(beacon_pk, &self.id_message(sender), key)
            .map_err(|_| EncryptedTransactionError::InvalidKey)
    }

    /// Decrypts the transaction with its key.
    pub fn open(
        &self,
        sender: &SuiAddress,
        key: &EncryptedTransactionKey,
    ) -> Result<ProgrammableTransaction, EncryptedTransactionError> {
        let nonce = self.parse_nonce()?;
        let id_message = self.id_message(sender);
        let shared = key.pairing(&nonce);
        let dem_key = kdf(&shared.to_byte_array(), &nonce, &id_message);
        let plaintext = dem(&dem_key)
            .decrypt_authenticated(&dem_nonce(), &id_message, &self.ciphertext)
            .map_err(|_| EncryptedTransactionError::DecryptionFailed)?;
        bcs::from_bytes(&plaintext).map_err(|_| EncryptedTransactionError::InvalidPlaintext)
    }

    /// The programmable transaction that this transaction executes, once it is set.
    pub fn decrypted(&self) -> Option<&ProgrammableTransaction> {
        self.decrypted.as_deref()
    }

    /// Sets the programmable transaction that this transaction executes. Pass `None` when the
    /// transaction must run no command.
    pub fn set_decrypted(&mut self, transaction: Option<ProgrammableTransaction>) {
        self.decrypted = Some(Arc::new(transaction.unwrap_or(ProgrammableTransaction {
            inputs: vec![],
            commands: vec![],
        })));
    }

    fn parse_nonce(&self) -> Result<G2Element, EncryptedTransactionError> {
        let bytes: [u8; 96] = self
            .nonce
            .as_slice()
            .try_into()
            .map_err(|_| EncryptedTransactionError::Malformed("nonce"))?;
        let nonce = G2Element::from_byte_array(&bytes)
            .map_err(|_| EncryptedTransactionError::Malformed("nonce"))?;
        if nonce == G2Element::zero() {
            return Err(EncryptedTransactionError::Malformed("nonce"));
        }
        Ok(nonce)
    }
}

/// The smallest gas price that pays the inclusion fee.
pub fn min_gas_price_for_fee(fee: u64) -> u64 {
    fee.div_ceil(MIN_COMPUTATION_UNITS)
}

/// Parses a compressed key from an `EncryptedTransactionResolution`.
pub fn key_from_bytes(bytes: &[u8]) -> Result<EncryptedTransactionKey, EncryptedTransactionError> {
    let bytes: [u8; 48] = bytes
        .try_into()
        .map_err(|_| EncryptedTransactionError::Malformed("key"))?;
    G1Element::from_byte_array(&bytes).map_err(|_| EncryptedTransactionError::Malformed("key"))
}

pub fn key_to_bytes(key: &EncryptedTransactionKey) -> Vec<u8> {
    key.to_byte_array().to_vec()
}

/// What a node reports about the beacon key, so that users can encrypt transactions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeaconKeyInfo {
    pub epoch: EpochId,
    /// Compressed G2 element (96 bytes).
    pub beacon_public_key: Vec<u8>,
}

impl BeaconKeyInfo {
    pub fn public_key(&self) -> Result<BeaconPublicKey, EncryptedTransactionError> {
        let bytes: [u8; 96] = self
            .beacon_public_key
            .as_slice()
            .try_into()
            .map_err(|_| EncryptedTransactionError::Malformed("beacon public key"))?;
        G2Element::from_byte_array(&bytes)
            .map_err(|_| EncryptedTransactionError::Malformed("beacon public key"))
    }
}

fn id_message(epoch: EpochId, sender: &SuiAddress, salt: &[u8]) -> Vec<u8> {
    let mut message = ID_TAG.to_vec();
    message.extend(bcs::to_bytes(&(epoch, sender, salt)).expect("serialization should not fail"));
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
#[path = "unit_tests/encrypted_transaction_tests.rs"]
mod encrypted_transaction_tests;
