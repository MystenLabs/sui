// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A service that builds encrypted transaction bundles, meant to run in a Nitro enclave.
//!
//! Users encrypt each transaction to the service's ephemeral key, which `/attest` binds to an
//! attestation document. The service keeps pending transactions in memory and `/next_block`
//! seals them into an `EncryptedBundle` under the random beacon's public key. The operator
//! submits the bundle to its validator and never sees the plaintext.
//!
//! Attestation goes through the `Attestor` trait. This crate provides only a mock; a Nitro
//! build implements the trait with the NSM API.

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use fastcrypto::aes::{Aes256GcmSiv, AesKey, AuthenticatedCipher};
use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::groups::ristretto255::{RistrettoPoint, RistrettoScalar};
use fastcrypto::groups::{GroupElement, Scalar};
use fastcrypto::hmac::{HkdfIkm, hkdf_sha3_256};
use fastcrypto::serde_helpers::ToFromByteArray;
use fastcrypto::traits::{AllowedRng, ToFromBytes};
use parking_lot::Mutex;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use sui_types::committee::EpochId;
use sui_types::encrypted_bundle::{BeaconKeyInfo, BundlePlaintext, EncryptedBundle};
use sui_types::transaction::{Transaction, TransactionDataAPI, TransactionExpiration};
use tracing::info;

const REQUEST_KDF_SALT: &[u8] = b"sui encrypted bundle request v1";
const REQUEST_AAD: &[u8] = b"sui encrypted bundle request";
/// Each request key encrypts one message, so the AEAD nonce is fixed.
const REQUEST_NONCE: [u8; 12] = [0; 12];

/// Upper bound on the BCS size of one transaction accepted by `/add_tx`.
pub const MAX_TRANSACTION_SIZE: usize = 128 * 1024;
/// Upper bound on the number of transactions in one bundle.
pub const MAX_PENDING_TRANSACTIONS: usize = 100;

#[derive(Debug, thiserror::Error)]
pub enum EnclaveError {
    #[error("the beacon key is not configured")]
    NotConfigured,
    #[error("request does not decrypt")]
    InvalidRequest,
    #[error("request is for epoch {expected}, but the enclave is configured for epoch {actual}")]
    EpochMismatch { expected: EpochId, actual: EpochId },
    #[error("request binds a beacon key that the enclave does not use")]
    BeaconKeyMismatch,
    #[error("transaction rejected: {0}")]
    InvalidTransaction(String),
    #[error("the pending bundle is full")]
    BundleFull,
    #[error("attestation failed: {0}")]
    Attestation(String),
}

impl IntoResponse for EnclaveError {
    fn into_response(self) -> Response {
        let status = match self {
            EnclaveError::NotConfigured => StatusCode::SERVICE_UNAVAILABLE,
            EnclaveError::Attestation(_) => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::BAD_REQUEST,
        };
        (status, self.to_string()).into_response()
    }
}

/// Produces attestation documents that bind the enclave's request key and beacon config.
pub trait Attestor: Send + Sync {
    fn attest(&self, public_key: &[u8], user_data: &[u8]) -> Result<Vec<u8>, String>;
}

/// An attestation stand-in for local runs and tests. It signs nothing.
#[derive(Default)]
pub struct MockAttestor;

#[derive(Serialize, Deserialize)]
pub struct MockAttestation {
    pub public_key: Vec<u8>,
    pub user_data: Vec<u8>,
}

impl Attestor for MockAttestor {
    fn attest(&self, public_key: &[u8], user_data: &[u8]) -> Result<Vec<u8>, String> {
        bcs::to_bytes(&MockAttestation {
            public_key: public_key.to_vec(),
            user_data: user_data.to_vec(),
        })
        .map_err(|e| e.to_string())
    }
}

/// The plaintext of an `/add_tx` request. The user names the beacon key it expects, so a
/// request cannot be sealed under a key that the operator substituted.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddTransactionPayload {
    pub transaction: Transaction,
    pub expected_epoch: EpochId,
    pub expected_beacon_public_key: Vec<u8>,
}

/// A request encrypted to the enclave's ephemeral key.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptedRequest {
    /// The sender's one-time Ristretto public key (32 bytes).
    pub ephemeral_public_key: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttestationResponse {
    /// Hex-encoded attestation document.
    pub attestation: String,
    /// Hex-encoded Ristretto public key that `/add_tx` requests are encrypted to.
    pub request_public_key: String,
    pub beacon: Option<BeaconKeyInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NextBlockResponse {
    /// Hex-encoded BCS `EncryptedBundle`, or `None` if no transaction is pending.
    pub bundle: Option<String>,
}

/// Encrypts `payload` to the enclave's request key.
pub fn encrypt_request<R: AllowedRng>(
    rng: &mut R,
    enclave_public_key: &RistrettoPoint,
    payload: &AddTransactionPayload,
) -> EncryptedRequest {
    let secret = RistrettoScalar::rand(rng);
    let ephemeral_public_key = RistrettoPoint::generator() * secret;
    let key = request_key(
        &(*enclave_public_key * secret),
        &ephemeral_public_key,
        enclave_public_key,
    );
    let plaintext = bcs::to_bytes(payload).expect("serialization should not fail");
    EncryptedRequest {
        ephemeral_public_key: ephemeral_public_key.to_byte_array().to_vec(),
        ciphertext: request_cipher(&key).encrypt_authenticated(
            &request_nonce(),
            REQUEST_AAD,
            &plaintext,
        ),
    }
}

fn request_key(
    shared: &RistrettoPoint,
    sender_public_key: &RistrettoPoint,
    enclave_public_key: &RistrettoPoint,
) -> [u8; 32] {
    let ikm = HkdfIkm::from_bytes(&shared.to_byte_array()).expect("no length requirement");
    let info = [
        sender_public_key.to_byte_array(),
        enclave_public_key.to_byte_array(),
    ]
    .concat();
    hkdf_sha3_256(&ikm, REQUEST_KDF_SALT, &info, 32)
        .expect("valid length")
        .try_into()
        .expect("32 bytes")
}

fn request_cipher(key: &[u8; 32]) -> Aes256GcmSiv {
    Aes256GcmSiv::new(AesKey::from_bytes(key).expect("32-byte key"))
}

fn request_nonce() -> <Aes256GcmSiv as AuthenticatedCipher>::IVType {
    <Aes256GcmSiv as AuthenticatedCipher>::IVType::from_bytes(&REQUEST_NONCE)
        .expect("12-byte nonce")
}

pub struct EnclaveState {
    request_secret: RistrettoScalar,
    request_public_key: RistrettoPoint,
    attestor: Box<dyn Attestor>,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    beacon: Option<BeaconKeyInfo>,
    pending: Vec<Transaction>,
    last_bundle: Option<EncryptedBundle>,
}

impl EnclaveState {
    pub fn new<R: AllowedRng>(rng: &mut R, attestor: Box<dyn Attestor>) -> Self {
        let request_secret = RistrettoScalar::rand(rng);
        Self {
            request_secret,
            request_public_key: RistrettoPoint::generator() * request_secret,
            attestor,
            inner: Mutex::new(Inner::default()),
        }
    }

    pub fn request_public_key(&self) -> RistrettoPoint {
        self.request_public_key
    }

    /// Sets the beacon key and the validator that will submit the bundles. Pending
    /// transactions bound to another key are dropped.
    pub fn update_key(&self, beacon: BeaconKeyInfo) -> Result<(), EnclaveError> {
        beacon
            .public_key()
            .map_err(|_| EnclaveError::InvalidRequest)?;
        let mut inner = self.inner.lock();
        if inner.beacon.as_ref() != Some(&beacon) {
            let dropped = inner.pending.len();
            inner.pending.clear();
            info!(
                "beacon key updated for epoch {}; dropped {dropped} pending transactions",
                beacon.epoch
            );
        }
        inner.beacon = Some(beacon);
        Ok(())
    }

    pub fn attest(&self) -> Result<AttestationResponse, EnclaveError> {
        let beacon = self.inner.lock().beacon.clone();
        let user_data = bcs::to_bytes(&beacon).expect("serialization should not fail");
        let public_key = self.request_public_key.to_byte_array();
        let attestation = self
            .attestor
            .attest(&public_key, &user_data)
            .map_err(EnclaveError::Attestation)?;
        Ok(AttestationResponse {
            attestation: Hex::encode(attestation),
            request_public_key: Hex::encode(public_key),
            beacon,
        })
    }

    pub fn add_transaction(&self, request: &EncryptedRequest) -> Result<usize, EnclaveError> {
        let payload = self.decrypt_request(request)?;
        let mut inner = self.inner.lock();
        let beacon = inner.beacon.as_ref().ok_or(EnclaveError::NotConfigured)?;
        if payload.expected_epoch != beacon.epoch {
            return Err(EnclaveError::EpochMismatch {
                expected: payload.expected_epoch,
                actual: beacon.epoch,
            });
        }
        if payload.expected_beacon_public_key != beacon.beacon_public_key {
            return Err(EnclaveError::BeaconKeyMismatch);
        }
        check_transaction(&payload.transaction, beacon.epoch)?;
        if inner.pending.len() >= MAX_PENDING_TRANSACTIONS {
            return Err(EnclaveError::BundleFull);
        }
        let digest = *payload.transaction.digest();
        if !inner.pending.iter().any(|tx| *tx.digest() == digest) {
            inner.pending.push(payload.transaction);
        }
        Ok(inner.pending.len())
    }

    /// Seals the pending transactions into a bundle and clears them. The order within the
    /// bundle is random, so the operator cannot learn the submission order from it.
    pub fn next_block<R: AllowedRng>(
        &self,
        rng: &mut R,
    ) -> Result<Option<EncryptedBundle>, EnclaveError> {
        let mut inner = self.inner.lock();
        let beacon = inner.beacon.clone().ok_or(EnclaveError::NotConfigured)?;
        if inner.pending.is_empty() {
            return Ok(None);
        }
        let mut transactions = std::mem::take(&mut inner.pending);
        transactions.shuffle(rng);
        let beacon_public_key = beacon
            .public_key()
            .map_err(|_| EnclaveError::NotConfigured)?;
        let bundle = EncryptedBundle::seal(
            rng,
            &beacon_public_key,
            beacon.epoch,
            beacon.authority,
            &BundlePlaintext { transactions },
        );
        inner.last_bundle = Some(bundle.clone());
        Ok(Some(bundle))
    }

    pub fn last_block(&self) -> Option<EncryptedBundle> {
        self.inner.lock().last_bundle.clone()
    }

    fn decrypt_request(
        &self,
        request: &EncryptedRequest,
    ) -> Result<AddTransactionPayload, EnclaveError> {
        let sender_bytes: [u8; 32] = request
            .ephemeral_public_key
            .as_slice()
            .try_into()
            .map_err(|_| EnclaveError::InvalidRequest)?;
        let sender_public_key = RistrettoPoint::from_byte_array(&sender_bytes)
            .map_err(|_| EnclaveError::InvalidRequest)?;
        let key = request_key(
            &(sender_public_key * self.request_secret),
            &sender_public_key,
            &self.request_public_key,
        );
        let plaintext = request_cipher(&key)
            .decrypt_authenticated(&request_nonce(), REQUEST_AAD, &request.ciphertext)
            .map_err(|_| EnclaveError::InvalidRequest)?;
        bcs::from_bytes(&plaintext).map_err(|_| EnclaveError::InvalidRequest)
    }
}

/// Checks the shape that validators require of decrypted transactions. Validators check
/// signatures and object existence after decryption.
fn check_transaction(transaction: &Transaction, epoch: EpochId) -> Result<(), EnclaveError> {
    let size = bcs::serialized_size(transaction).expect("serialization should not fail");
    if size > MAX_TRANSACTION_SIZE {
        return Err(EnclaveError::InvalidTransaction(format!(
            "size {size} exceeds {MAX_TRANSACTION_SIZE}"
        )));
    }
    let data = transaction.data().transaction_data();
    if !data.gas().is_empty() {
        return Err(EnclaveError::InvalidTransaction(
            "gas must be paid from the address balance".to_string(),
        ));
    }
    // The bundle key exists only for the bundle's epoch. A transaction that is still valid
    // later could be read and replayed by whoever learns a beacon key of an earlier epoch.
    match data.expiration() {
        TransactionExpiration::ValidDuring {
            min_epoch: Some(min_epoch),
            max_epoch: Some(max_epoch),
            ..
        } if *min_epoch == epoch && *max_epoch <= epoch + 1 => Ok(()),
        _ => Err(EnclaveError::InvalidTransaction(format!(
            "expiration must be bounded to epoch {epoch}"
        ))),
    }
}

async fn update_key(
    State(state): State<Arc<EnclaveState>>,
    Json(beacon): Json<BeaconKeyInfo>,
) -> Result<StatusCode, EnclaveError> {
    state.update_key(beacon)?;
    Ok(StatusCode::OK)
}

async fn attest(
    State(state): State<Arc<EnclaveState>>,
) -> Result<Json<AttestationResponse>, EnclaveError> {
    state.attest().map(Json)
}

async fn add_tx(
    State(state): State<Arc<EnclaveState>>,
    Json(request): Json<EncryptedRequest>,
) -> Result<Json<usize>, EnclaveError> {
    state.add_transaction(&request).map(Json)
}

async fn next_block(
    State(state): State<Arc<EnclaveState>>,
) -> Result<Json<NextBlockResponse>, EnclaveError> {
    let bundle = state.next_block(&mut rand::thread_rng())?;
    Ok(Json(bundle_response(bundle)))
}

async fn last_block(State(state): State<Arc<EnclaveState>>) -> Json<NextBlockResponse> {
    Json(bundle_response(state.last_block()))
}

fn bundle_response(bundle: Option<EncryptedBundle>) -> NextBlockResponse {
    NextBlockResponse {
        bundle: bundle.map(|bundle| {
            Hex::encode(bcs::to_bytes(&bundle).expect("serialization should not fail"))
        }),
    }
}

/// `/update_key`, `/next_block` and `/last_block` are operator calls. A deployment exposes
/// only `/attest` and `/add_tx` to users.
pub fn router(state: Arc<EnclaveState>) -> Router {
    Router::new()
        .route("/update_key", post(update_key))
        .route("/attest", get(attest))
        .route("/add_tx", post(add_tx))
        .route("/next_block", post(next_block))
        .route("/last_block", get(last_block))
        .with_state(state)
}

#[cfg(test)]
mod tests;
