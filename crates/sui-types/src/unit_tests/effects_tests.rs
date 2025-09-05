// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{ObjectID, SequenceNumber, SuiAddress};
use crate::crypto::{get_key_pair_from_rng, AccountKeyPair};
use crate::digests::ObjectDigest;
use crate::effects::{TestEffectsBuilder, TransactionEffectsAPI};
use crate::object::Owner;
use crate::programmable_transaction_builder::ProgrammableTransactionBuilder;
use crate::transaction::{Transaction, TransactionData};
use crate::utils::to_sender_signed_transaction;
use fastcrypto::ed25519::Ed25519KeyPair;
use rand::rngs::StdRng;
use rand::SeedableRng;

fn make_test_transaction(
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas_object_id: ObjectID,
) -> Transaction {
    let pt = ProgrammableTransactionBuilder::new();
    // Create a gas object ref for the transaction
    let gas_object = (
        gas_object_id,
        SequenceNumber::from(1),
        ObjectDigest::random(),
    );
    let tx_data = TransactionData::new_programmable(sender, vec![gas_object], pt.finish(), 1000, 1);
    to_sender_signed_transaction(tx_data, keypair)
}

#[test]
fn test_written_with_created_objects() {
    let mut rng = StdRng::from_seed([0; 32]);
    let (sender, keypair): (SuiAddress, AccountKeyPair) =
        get_key_pair_from_rng::<Ed25519KeyPair, _>(&mut rng);
    let gas_object_id = ObjectID::random();
    let tx = make_test_transaction(sender, &keypair, gas_object_id);

    let obj1 = ObjectID::random();
    let obj2 = ObjectID::random();

    let effects = TestEffectsBuilder::new(tx.data())
        .with_created_objects(vec![
            (obj1, Owner::AddressOwner(sender)),
            (obj2, Owner::AddressOwner(sender)),
        ])
        .build();

    let written = effects.written();
    // Should include gas object (mutated) and the 2 created objects
    assert_eq!(written.len(), 3);

    // Check that created objects are included with their digests
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == obj1 && digest.is_alive() }));
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == obj2 && digest.is_alive() }));
    // Check gas object is included
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == gas_object_id && digest.is_alive() }));
}

#[test]
fn test_written_with_mutated_objects() {
    let mut rng = StdRng::from_seed([1; 32]);
    let (sender, keypair): (SuiAddress, AccountKeyPair) =
        get_key_pair_from_rng::<Ed25519KeyPair, _>(&mut rng);
    let gas_object_id = ObjectID::random();
    let tx = make_test_transaction(sender, &keypair, gas_object_id);

    let obj1 = ObjectID::random();
    let obj2 = ObjectID::random();
    let version1 = SequenceNumber::from(5);
    let version2 = SequenceNumber::from(10);

    let effects = TestEffectsBuilder::new(tx.data())
        .with_mutated_objects(vec![
            (obj1, version1, Owner::AddressOwner(sender)),
            (obj2, version2, Owner::AddressOwner(sender)),
        ])
        .build();

    let written = effects.written();
    // Should include gas object (mutated) and the 2 mutated objects
    assert_eq!(written.len(), 3);

    // Check that mutated objects are included with their new digests
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == obj1 && digest.is_alive() }));
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == obj2 && digest.is_alive() }));
    // Check gas object is included
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == gas_object_id && digest.is_alive() }));
}

#[test]
fn test_written_with_deleted_objects() {
    let mut rng = StdRng::from_seed([2; 32]);
    let (sender, keypair): (SuiAddress, AccountKeyPair) =
        get_key_pair_from_rng::<Ed25519KeyPair, _>(&mut rng);
    let gas_object_id = ObjectID::random();
    let tx = make_test_transaction(sender, &keypair, gas_object_id);

    let obj1 = ObjectID::random();
    let obj2 = ObjectID::random();
    let version1 = SequenceNumber::from(5);
    let version2 = SequenceNumber::from(10);

    let effects = TestEffectsBuilder::new(tx.data())
        .with_deleted_objects(vec![(obj1, version1), (obj2, version2)])
        .build();

    let written = effects.written();
    // Should include gas object (mutated) and the 2 deleted objects
    assert_eq!(written.len(), 3);

    // Check that deleted objects are included with OBJECT_DIGEST_DELETED
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == obj1 && *digest == ObjectDigest::OBJECT_DIGEST_DELETED }));
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == obj2 && *digest == ObjectDigest::OBJECT_DIGEST_DELETED }));
    // Check gas object is included
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == gas_object_id && digest.is_alive() }));
}

#[test]
fn test_written_with_wrapped_objects() {
    let mut rng = StdRng::from_seed([3; 32]);
    let (sender, keypair): (SuiAddress, AccountKeyPair) =
        get_key_pair_from_rng::<Ed25519KeyPair, _>(&mut rng);
    let gas_object_id = ObjectID::random();
    let tx = make_test_transaction(sender, &keypair, gas_object_id);

    let obj1 = ObjectID::random();
    let obj2 = ObjectID::random();
    let version1 = SequenceNumber::from(5);
    let version2 = SequenceNumber::from(10);

    let effects = TestEffectsBuilder::new(tx.data())
        .with_wrapped_objects(vec![(obj1, version1), (obj2, version2)])
        .build();

    let written = effects.written();
    // Should include gas object (mutated) and the 2 wrapped objects
    assert_eq!(written.len(), 3);

    // Check that wrapped objects are included with OBJECT_DIGEST_WRAPPED
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == obj1 && *digest == ObjectDigest::OBJECT_DIGEST_WRAPPED }));
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == obj2 && *digest == ObjectDigest::OBJECT_DIGEST_WRAPPED }));
    // Check gas object is included
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == gas_object_id && digest.is_alive() }));
}

#[test]
fn test_written_with_unwrapped_objects() {
    let mut rng = StdRng::from_seed([4; 32]);
    let (sender, keypair): (SuiAddress, AccountKeyPair) =
        get_key_pair_from_rng::<Ed25519KeyPair, _>(&mut rng);
    let gas_object_id = ObjectID::random();
    let tx = make_test_transaction(sender, &keypair, gas_object_id);

    let obj1 = ObjectID::random();
    let obj2 = ObjectID::random();

    let effects = TestEffectsBuilder::new(tx.data())
        .with_unwrapped_objects(vec![
            (obj1, Owner::AddressOwner(sender)),
            (obj2, Owner::AddressOwner(sender)),
        ])
        .build();

    let written = effects.written();
    // Should include gas object (mutated) and the 2 unwrapped objects
    assert_eq!(written.len(), 3);

    // Check that unwrapped objects are included with their new digests
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == obj1 && digest.is_alive() }));
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == obj2 && digest.is_alive() }));
    // Check gas object is included
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == gas_object_id && digest.is_alive() }));
}

#[test]
fn test_written_with_combination_of_all_types() {
    let mut rng = StdRng::from_seed([5; 32]);
    let (sender, keypair): (SuiAddress, AccountKeyPair) =
        get_key_pair_from_rng::<Ed25519KeyPair, _>(&mut rng);
    let gas_object_id = ObjectID::random();
    let tx = make_test_transaction(sender, &keypair, gas_object_id);

    let created1 = ObjectID::random();
    let created2 = ObjectID::random();
    let mutated1 = ObjectID::random();
    let mutated2 = ObjectID::random();
    let deleted1 = ObjectID::random();
    let deleted2 = ObjectID::random();
    let wrapped1 = ObjectID::random();
    let wrapped2 = ObjectID::random();
    let unwrapped1 = ObjectID::random();
    let unwrapped2 = ObjectID::random();

    let effects = TestEffectsBuilder::new(tx.data())
        .with_created_objects(vec![
            (created1, Owner::AddressOwner(sender)),
            (created2, Owner::AddressOwner(sender)),
        ])
        .with_mutated_objects(vec![
            (
                mutated1,
                SequenceNumber::from(5),
                Owner::AddressOwner(sender),
            ),
            (
                mutated2,
                SequenceNumber::from(10),
                Owner::AddressOwner(sender),
            ),
        ])
        .with_deleted_objects(vec![
            (deleted1, SequenceNumber::from(3)),
            (deleted2, SequenceNumber::from(7)),
        ])
        .with_wrapped_objects(vec![
            (wrapped1, SequenceNumber::from(4)),
            (wrapped2, SequenceNumber::from(8)),
        ])
        .with_unwrapped_objects(vec![
            (unwrapped1, Owner::AddressOwner(sender)),
            (unwrapped2, Owner::AddressOwner(sender)),
        ])
        .build();

    let written = effects.written();
    // Should include gas object (mutated) and all 10 other objects
    assert_eq!(written.len(), 11);

    // Verify created objects have alive digests
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == created1 && digest.is_alive() }));
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == created2 && digest.is_alive() }));

    // Verify mutated objects have alive digests
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == mutated1 && digest.is_alive() }));
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == mutated2 && digest.is_alive() }));

    // Verify deleted objects have OBJECT_DIGEST_DELETED
    assert!(written.iter().any(|(id, _, digest)| {
        *id == deleted1 && *digest == ObjectDigest::OBJECT_DIGEST_DELETED
    }));
    assert!(written.iter().any(|(id, _, digest)| {
        *id == deleted2 && *digest == ObjectDigest::OBJECT_DIGEST_DELETED
    }));

    // Verify wrapped objects have OBJECT_DIGEST_WRAPPED
    assert!(written.iter().any(|(id, _, digest)| {
        *id == wrapped1 && *digest == ObjectDigest::OBJECT_DIGEST_WRAPPED
    }));
    assert!(written.iter().any(|(id, _, digest)| {
        *id == wrapped2 && *digest == ObjectDigest::OBJECT_DIGEST_WRAPPED
    }));

    // Verify unwrapped objects have alive digests
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == unwrapped1 && digest.is_alive() }));
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == unwrapped2 && digest.is_alive() }));

    // Check gas object is included
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == gas_object_id && digest.is_alive() }));
}

#[test]
fn test_written_excludes_frozen_objects() {
    let mut rng = StdRng::from_seed([6; 32]);
    let (sender, keypair): (SuiAddress, AccountKeyPair) =
        get_key_pair_from_rng::<Ed25519KeyPair, _>(&mut rng);
    let gas_object_id = ObjectID::random();
    let tx = make_test_transaction(sender, &keypair, gas_object_id);

    let frozen1 = ObjectID::random();
    let frozen2 = ObjectID::random();
    let mutated = ObjectID::random();

    let effects = TestEffectsBuilder::new(tx.data())
        .with_frozen_objects(vec![frozen1, frozen2])
        .with_mutated_objects(vec![(
            mutated,
            SequenceNumber::from(5),
            Owner::AddressOwner(sender),
        )])
        .build();

    let written = effects.written();
    // Should include gas object (mutated) and the mutated object, but not frozen objects
    assert_eq!(written.len(), 2);
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == mutated && digest.is_alive() }));

    // Verify frozen objects are not included
    assert!(!written.iter().any(|(id, _, _)| *id == frozen1));
    assert!(!written.iter().any(|(id, _, _)| *id == frozen2));

    // Check gas object is included
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == gas_object_id && digest.is_alive() }));
}

#[test]
fn test_written_empty_when_no_changes() {
    let mut rng = StdRng::from_seed([7; 32]);
    let (sender, keypair): (SuiAddress, AccountKeyPair) =
        get_key_pair_from_rng::<Ed25519KeyPair, _>(&mut rng);
    let gas_object_id = ObjectID::random();
    let tx = make_test_transaction(sender, &keypair, gas_object_id);

    let effects = TestEffectsBuilder::new(tx.data()).build();

    let written = effects.written();
    // Should only include the gas object (mutated)
    assert_eq!(written.len(), 1);
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == gas_object_id && digest.is_alive() }));
}

#[test]
fn test_written_version_numbers() {
    let mut rng = StdRng::from_seed([8; 32]);
    let (sender, keypair): (SuiAddress, AccountKeyPair) =
        get_key_pair_from_rng::<Ed25519KeyPair, _>(&mut rng);
    let gas_object_id = ObjectID::random();
    let tx = make_test_transaction(sender, &keypair, gas_object_id);

    let obj1 = ObjectID::random();
    let obj2 = ObjectID::random();
    let obj3 = ObjectID::random();

    let effects = TestEffectsBuilder::new(tx.data())
        .with_created_objects(vec![(obj1, Owner::AddressOwner(sender))])
        .with_mutated_objects(vec![(
            obj2,
            SequenceNumber::from(5),
            Owner::AddressOwner(sender),
        )])
        .with_deleted_objects(vec![(obj3, SequenceNumber::from(10))])
        .build();

    let written = effects.written();
    let lamport_version = effects.lamport_version();

    // All objects should have the same lamport version
    for (_, version, _) in &written {
        assert_eq!(*version, lamport_version);
    }
}

#[test]
fn test_written_with_wrapped_and_deleted_distinction() {
    let mut rng = StdRng::from_seed([9; 32]);
    let (sender, keypair): (SuiAddress, AccountKeyPair) =
        get_key_pair_from_rng::<Ed25519KeyPair, _>(&mut rng);
    let gas_object_id = ObjectID::random();
    let tx = make_test_transaction(sender, &keypair, gas_object_id);

    let wrapped = ObjectID::random();
    let deleted = ObjectID::random();

    let effects = TestEffectsBuilder::new(tx.data())
        .with_wrapped_objects(vec![(wrapped, SequenceNumber::from(5))])
        .with_deleted_objects(vec![(deleted, SequenceNumber::from(10))])
        .build();

    let written = effects.written();
    // Should include gas object (mutated), wrapped, and deleted objects
    assert_eq!(written.len(), 3);

    // Find the wrapped object
    let wrapped_ref = written.iter().find(|(id, _, _)| *id == wrapped);
    assert!(wrapped_ref.is_some());
    let (_, _, wrapped_digest) = wrapped_ref.unwrap();
    assert_eq!(*wrapped_digest, ObjectDigest::OBJECT_DIGEST_WRAPPED);
    assert!(wrapped_digest.is_wrapped());
    assert!(!wrapped_digest.is_deleted());

    // Find the deleted object
    let deleted_ref = written.iter().find(|(id, _, _)| *id == deleted);
    assert!(deleted_ref.is_some());
    let (_, _, deleted_digest) = deleted_ref.unwrap();
    assert_eq!(*deleted_digest, ObjectDigest::OBJECT_DIGEST_DELETED);
    assert!(deleted_digest.is_deleted());
    assert!(!deleted_digest.is_wrapped());

    // Check gas object is included
    assert!(written
        .iter()
        .any(|(id, _, digest)| { *id == gas_object_id && digest.is_alive() }));
}
