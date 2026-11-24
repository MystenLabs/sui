// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::base_types::{ObjectID, random_object_ref};
use crate::crypto::RandomnessRound;
use crate::programmable_transaction_builder::ProgrammableTransactionBuilder;
use crate::transaction::{
    TransactionData, TransactionDataAPI, TransactionKind, TxValidityCheckContext,
};
use fastcrypto::groups::bls12381::Scalar;
use fastcrypto_threshold::polynomial::{Eval, Poly};
use fastcrypto_threshold::types::ShareIndex;
use rand::SeedableRng;
use rand::rngs::StdRng;
use sui_protocol_config::ProtocolConfig;

const THRESHOLD: u16 = 3;
const NUM_SHARES: u16 = 5;

struct TestBeacon {
    public_key: BeaconPublicKey,
    shares: Vec<Eval<Scalar>>,
}

impl TestBeacon {
    fn new(rng: &mut StdRng) -> Self {
        let private_poly = Poly::<Scalar>::rand(THRESHOLD - 1, rng);
        let public_key = private_poly.commit::<G2Element>().c0();
        let shares = (1..=NUM_SHARES)
            .map(|i| private_poly.eval(ShareIndex::new(i).unwrap()))
            .collect();
        Self { public_key, shares }
    }

    /// Derives the key from `THRESHOLD` partial signatures, as the validators do.
    fn derive_key(&self, id_message: &[u8]) -> EncryptedTransactionKey {
        let partials =
            ThresholdBls12381MinSig::partial_sign_batch(self.shares.iter().skip(1), id_message);
        ThresholdBls12381MinSig::aggregate(THRESHOLD, partials.iter()).unwrap()
    }
}

fn programmable() -> ProgrammableTransaction {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.pure(42u64).unwrap();
    builder.finish()
}

#[test]
fn seal_and_open() {
    let mut rng = StdRng::from_seed([1; 32]);
    let beacon = TestBeacon::new(&mut rng);
    let sender = SuiAddress::random_for_testing_only();
    let transaction = programmable();

    let encrypted =
        EncryptedTransaction::seal(&mut rng, &beacon.public_key, 7, sender, &transaction);
    encrypted.check_format().unwrap();

    let key = beacon.derive_key(&encrypted.id_message(&sender));
    encrypted
        .verify_key(&beacon.public_key, &sender, &key)
        .unwrap();
    assert_eq!(encrypted.open(&sender, &key).unwrap(), transaction);

    let decoded: EncryptedTransaction =
        bcs::from_bytes(&bcs::to_bytes(&encrypted).unwrap()).unwrap();
    assert_eq!(decoded.open(&sender, &key).unwrap(), transaction);

    let round_trip = key_from_bytes(&key_to_bytes(&key)).unwrap();
    assert_eq!(round_trip, key);
}

#[test]
fn key_of_another_sender_does_not_open() {
    let mut rng = StdRng::from_seed([2; 32]);
    let beacon = TestBeacon::new(&mut rng);
    let sender = SuiAddress::random_for_testing_only();
    let attacker = SuiAddress::random_for_testing_only();
    let encrypted =
        EncryptedTransaction::seal(&mut rng, &beacon.public_key, 7, sender, &programmable());

    // An attacker that puts the ciphertext into its own transaction gets the key of its own
    // identity, which does not decrypt the ciphertext.
    let attacker_key = beacon.derive_key(&encrypted.id_message(&attacker));
    assert_eq!(
        encrypted.verify_key(&beacon.public_key, &sender, &attacker_key),
        Err(EncryptedTransactionError::InvalidKey)
    );
    assert_eq!(
        encrypted.open(&attacker, &attacker_key),
        Err(EncryptedTransactionError::DecryptionFailed)
    );
    assert_eq!(
        encrypted.open(&sender, &attacker_key),
        Err(EncryptedTransactionError::DecryptionFailed)
    );
}

#[test]
fn tampered_ciphertext_does_not_open() {
    let mut rng = StdRng::from_seed([3; 32]);
    let beacon = TestBeacon::new(&mut rng);
    let sender = SuiAddress::random_for_testing_only();
    let encrypted =
        EncryptedTransaction::seal(&mut rng, &beacon.public_key, 7, sender, &programmable());
    let key = beacon.derive_key(&encrypted.id_message(&sender));

    let mut changed = encrypted.clone();
    changed.ciphertext[0] ^= 1;
    assert_eq!(
        changed.open(&sender, &key),
        Err(EncryptedTransactionError::DecryptionFailed)
    );

    let mut changed_epoch = encrypted;
    changed_epoch.epoch += 1;
    let other_key = beacon.derive_key(&changed_epoch.id_message(&sender));
    assert_eq!(
        changed_epoch.open(&sender, &other_key),
        Err(EncryptedTransactionError::DecryptionFailed)
    );
}

#[test]
fn malformed_fields_are_rejected() {
    let mut rng = StdRng::from_seed([4; 32]);
    let beacon = TestBeacon::new(&mut rng);
    let sender = SuiAddress::random_for_testing_only();
    let encrypted =
        EncryptedTransaction::seal(&mut rng, &beacon.public_key, 7, sender, &programmable());

    let mut short_nonce = encrypted.clone();
    short_nonce.nonce.pop();
    assert_eq!(
        short_nonce.check_format(),
        Err(EncryptedTransactionError::Malformed("nonce"))
    );

    let mut zero_nonce = encrypted.clone();
    zero_nonce.nonce = G2Element::zero().to_byte_array().to_vec();
    assert_eq!(
        zero_nonce.check_format(),
        Err(EncryptedTransactionError::Malformed("nonce"))
    );

    let mut short_salt = encrypted;
    short_salt.salt.pop();
    assert_eq!(
        short_salt.check_format(),
        Err(EncryptedTransactionError::Malformed("salt"))
    );
}

#[test]
fn identity_messages_are_separated_from_beacon_rounds() {
    let mut rng = StdRng::from_seed([5; 32]);
    let beacon = TestBeacon::new(&mut rng);
    let sender = SuiAddress::random_for_testing_only();
    let encrypted =
        EncryptedTransaction::seal(&mut rng, &beacon.public_key, 0, sender, &programmable());

    let round_prefix = RandomnessRound::new(0).signature_message();
    let round_prefix = &round_prefix[..round_prefix.len() - 8];
    assert!(!ID_TAG.starts_with(round_prefix));
    assert!(!round_prefix.starts_with(ID_TAG));
    assert_ne!(ID_TAG[0], round_prefix[0]);
    assert!(encrypted.id_message(&sender).starts_with(ID_TAG));
}

#[test]
fn decrypted_plaintext_does_not_change_digest() {
    let mut rng = StdRng::from_seed([6; 32]);
    let beacon = TestBeacon::new(&mut rng);
    let sender = SuiAddress::random_for_testing_only();
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .obj(crate::transaction::ObjectArg::SharedObject {
            id: ObjectID::random(),
            initial_shared_version: 1.into(),
            mutability: crate::transaction::SharedObjectMutability::Mutable,
        })
        .unwrap();
    let transaction = builder.finish();
    let encrypted =
        EncryptedTransaction::seal(&mut rng, &beacon.public_key, 7, sender, &transaction);

    let data = TransactionData::new_with_gas_coins(
        TransactionKind::EncryptedTransaction(encrypted),
        sender,
        vec![random_object_ref()],
        1_000_000,
        1_000,
    );
    assert_eq!(data.kind().shared_input_objects().count(), 0);

    let mut decrypted = data.clone();
    let TransactionKind::EncryptedTransaction(et) = decrypted.kind_mut() else {
        unreachable!()
    };
    et.set_decrypted(Some(transaction));
    assert_eq!(decrypted.digest(), data.digest());
    assert_eq!(decrypted, data);
    assert_eq!(decrypted.kind().shared_input_objects().count(), 1);
    assert_eq!(decrypted.input_objects().unwrap().len(), 2);
}

#[test]
fn vote_checks() {
    let mut rng = StdRng::from_seed([7; 32]);
    let beacon = TestBeacon::new(&mut rng);
    let sender = SuiAddress::random_for_testing_only();
    let encrypted =
        EncryptedTransaction::seal(&mut rng, &beacon.public_key, 0, sender, &programmable());

    let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
    config.set_encrypted_transaction_fee_for_testing(1_000_000);
    let data = |price| {
        TransactionData::new_with_gas_coins(
            TransactionKind::EncryptedTransaction(encrypted.clone()),
            sender,
            vec![random_object_ref()],
            100_000_000,
            price,
        )
    };

    // The flag is off in every protocol version.
    let context = TxValidityCheckContext::from_cfg_for_testing(&config);
    assert!(data(1_000).validity_check(&context).is_err());

    config.set_encrypted_transactions_for_testing(true);
    let context = TxValidityCheckContext::from_cfg_for_testing(&config);
    data(1_000).validity_check(&context).unwrap();
    // A gas price of 999 charges less than the fee for 1,000 computation units.
    assert!(data(999).validity_check(&context).is_err());

    let mut other_epoch = context;
    other_epoch.epoch = 1;
    assert!(data(1_000).validity_check(&other_epoch).is_err());
}
