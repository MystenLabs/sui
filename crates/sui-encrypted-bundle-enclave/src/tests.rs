// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use axum::body::Body;
use axum::http::Request;
use fastcrypto::groups::HashToGroupElement;
use fastcrypto::groups::bls12381::{G1Element, G2Element, Scalar as BlsScalar};
use http_body_util::BodyExt;
use rand::SeedableRng;
use rand::rngs::StdRng;
use sui_types::base_types::{AuthorityName, SuiAddress};
use sui_types::crypto::{AccountKeyPair, get_key_pair_from_rng};
use sui_types::digests::ChainIdentifier;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::TransactionData;
use sui_types::utils::to_sender_signed_transaction;
use tower::ServiceExt;

const EPOCH: EpochId = 3;

struct TestBeacon {
    master: BlsScalar,
    info: BeaconKeyInfo,
}

impl TestBeacon {
    fn new(rng: &mut StdRng) -> Self {
        let master = BlsScalar::rand(rng);
        let info = BeaconKeyInfo {
            epoch: EPOCH,
            authority: AuthorityName::ZERO,
            beacon_public_key: (G2Element::generator() * master).to_byte_array().to_vec(),
        };
        Self { master, info }
    }

    fn key_for(&self, bundle: &EncryptedBundle) -> G1Element {
        G1Element::hash_to_group_element(&bundle.id_message()) * self.master
    }
}

fn transaction(rng: &mut StdRng, nonce: u32, epoch: EpochId) -> Transaction {
    let (sender, keypair): (SuiAddress, AccountKeyPair) = get_key_pair_from_rng(rng);
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(sender, Some(1));
    let data = TransactionData::new_programmable_with_address_balance_gas(
        sender,
        builder.finish(),
        10_000_000,
        1_000,
        ChainIdentifier::default(),
        epoch,
        nonce,
    );
    to_sender_signed_transaction(data, &keypair)
}

fn payload(transaction: Transaction, beacon: &BeaconKeyInfo) -> AddTransactionPayload {
    AddTransactionPayload {
        transaction,
        expected_epoch: beacon.epoch,
        expected_beacon_public_key: beacon.beacon_public_key.clone(),
    }
}

#[test]
fn bundle_holds_submitted_transactions() {
    let mut rng = StdRng::from_seed([1; 32]);
    let beacon = TestBeacon::new(&mut rng);
    let state = EnclaveState::new(&mut rng, Box::new(MockAttestor));
    state.update_key(beacon.info.clone()).unwrap();

    let transactions = vec![
        transaction(&mut rng, 1, EPOCH),
        transaction(&mut rng, 2, EPOCH),
    ];
    for tx in &transactions {
        let request = encrypt_request(
            &mut rng,
            &state.request_public_key(),
            &payload(tx.clone(), &beacon.info),
        );
        state.add_transaction(&request).unwrap();
    }

    let bundle = state.next_block(&mut rng).unwrap().unwrap();
    bundle.verify_signature().unwrap();
    assert_eq!(bundle.epoch, EPOCH);
    assert_eq!(bundle.author, beacon.info.authority);

    let key = beacon.key_for(&bundle);
    bundle
        .verify_key(&beacon.info.public_key().unwrap(), &key)
        .unwrap();
    let mut opened: Vec<_> = bundle
        .open(&key)
        .unwrap()
        .transactions
        .iter()
        .map(|tx| *tx.digest())
        .collect();
    let mut expected: Vec<_> = transactions.iter().map(|tx| *tx.digest()).collect();
    opened.sort();
    expected.sort();
    assert_eq!(opened, expected);

    // The pending block is cleared and the last block can be fetched again.
    assert!(state.next_block(&mut rng).unwrap().is_none());
    assert_eq!(state.last_block(), Some(bundle));
}

#[test]
fn requests_bound_to_another_key_are_rejected() {
    let mut rng = StdRng::from_seed([2; 32]);
    let beacon = TestBeacon::new(&mut rng);
    let other_beacon = TestBeacon::new(&mut rng);
    let state = EnclaveState::new(&mut rng, Box::new(MockAttestor));
    let tx = transaction(&mut rng, 1, EPOCH);

    let request = encrypt_request(
        &mut rng,
        &state.request_public_key(),
        &payload(tx.clone(), &beacon.info),
    );
    assert!(matches!(
        state.add_transaction(&request),
        Err(EnclaveError::NotConfigured)
    ));

    // The operator configures a key that the user did not expect.
    state.update_key(other_beacon.info.clone()).unwrap();
    assert!(matches!(
        state.add_transaction(&request),
        Err(EnclaveError::BeaconKeyMismatch)
    ));

    let mut wrong_epoch = payload(tx, &other_beacon.info);
    wrong_epoch.expected_epoch += 1;
    let request = encrypt_request(&mut rng, &state.request_public_key(), &wrong_epoch);
    assert!(matches!(
        state.add_transaction(&request),
        Err(EnclaveError::EpochMismatch { .. })
    ));
}

#[test]
fn key_update_drops_pending_transactions() {
    let mut rng = StdRng::from_seed([3; 32]);
    let beacon = TestBeacon::new(&mut rng);
    let next_beacon = TestBeacon::new(&mut rng);
    let state = EnclaveState::new(&mut rng, Box::new(MockAttestor));
    state.update_key(beacon.info.clone()).unwrap();
    let tx = transaction(&mut rng, 1, EPOCH);
    let request = encrypt_request(
        &mut rng,
        &state.request_public_key(),
        &payload(tx, &beacon.info),
    );
    state.add_transaction(&request).unwrap();

    state.update_key(next_beacon.info).unwrap();
    assert!(state.next_block(&mut rng).unwrap().is_none());
}

#[test]
fn malformed_requests_and_transactions_are_rejected() {
    let mut rng = StdRng::from_seed([4; 32]);
    let beacon = TestBeacon::new(&mut rng);
    let state = EnclaveState::new(&mut rng, Box::new(MockAttestor));
    state.update_key(beacon.info.clone()).unwrap();

    let tx = transaction(&mut rng, 1, EPOCH);

    let mut request = encrypt_request(
        &mut rng,
        &state.request_public_key(),
        &payload(tx, &beacon.info),
    );
    request.ciphertext[0] ^= 1;
    assert!(matches!(
        state.add_transaction(&request),
        Err(EnclaveError::InvalidRequest)
    ));

    // A transaction for a later epoch outlives the beacon key of this epoch.
    let tx = transaction(&mut rng, 2, EPOCH + 1);
    let request = encrypt_request(
        &mut rng,
        &state.request_public_key(),
        &payload(tx, &beacon.info),
    );
    assert!(matches!(
        state.add_transaction(&request),
        Err(EnclaveError::InvalidTransaction(_))
    ));
}

#[tokio::test]
async fn attest_route_binds_request_key_and_beacon() {
    let mut rng = StdRng::from_seed([5; 32]);
    let beacon = TestBeacon::new(&mut rng);
    let state = Arc::new(EnclaveState::new(&mut rng, Box::new(MockAttestor)));
    state.update_key(beacon.info.clone()).unwrap();

    let response = router(state.clone())
        .oneshot(Request::get("/attest").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let response: AttestationResponse = serde_json::from_slice(&body).unwrap();

    let attestation: MockAttestation =
        bcs::from_bytes(&Hex::decode(&response.attestation).unwrap()).unwrap();
    assert_eq!(
        attestation.public_key,
        state.request_public_key().to_byte_array().to_vec()
    );
    let attested_beacon: Option<BeaconKeyInfo> = bcs::from_bytes(&attestation.user_data).unwrap();
    assert_eq!(attested_beacon, Some(beacon.info));
}
