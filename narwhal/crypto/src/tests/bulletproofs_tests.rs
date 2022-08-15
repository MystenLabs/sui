// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    bulletproofs::{BulletproofsRangeProof, PedersenCommitment},
    traits::ToFromBytes,
};

///
/// Test Pedersen Commitments
///

#[test]
fn test_commit_and_open() {
    // Should we create a wrapper scalar type or is using this fine?
    let value = [0; 32];
    let blinding = [0; 32];

    // Commit
    let commitment = PedersenCommitment::new(value, blinding);
    // Open
    let commitment_2 = PedersenCommitment::new(value, blinding);
    assert_eq!(commitment, commitment_2);
}

#[test]
fn test_binding_commitment() {
    let value = [0; 32];
    let other_value = [1; 32];
    let blinding = [2; 32];

    let commitment = PedersenCommitment::new(value, blinding);
    let other_commitment = PedersenCommitment::new(other_value, blinding);

    assert_ne!(commitment, other_commitment);
}

#[test]
fn test_pedersen_to_from_bytes() {
    let value = [0; 32];
    let blinding = [1; 32];

    let commitment = PedersenCommitment::new(value, blinding);
    let commitment_dup = PedersenCommitment::from_bytes(commitment.as_bytes()).unwrap();

    assert_eq!(commitment, commitment_dup);
}

#[test]
fn test_pedersen_serde() {
    let value = [0; 32];
    let blinding = [1; 32];

    let commitment = PedersenCommitment::new(value, blinding);
    let ser = bincode::serialize(&commitment).unwrap();
    let commitment_dup: PedersenCommitment = bincode::deserialize(&ser).unwrap();

    assert_eq!(commitment, commitment_dup);
}

///
/// Test Range Proofs
///
const TEST_DOMAIN: &[u8; 7] = b"NARWHAL";

#[test]
fn test_range_proof_valid() {
    let upper_bound: usize = 64;
    let blinding = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,
        0, 1,
    ];

    let (commitment, range_proof) =
        BulletproofsRangeProof::prove_bit_length(1u64, blinding, upper_bound, TEST_DOMAIN).unwrap();

    assert!(range_proof
        .verify_bit_length(&commitment, upper_bound, TEST_DOMAIN)
        .is_ok());
}

#[test]
fn test_range_proof_invalid() {
    let upper_bound: usize = 64;
    let blinding = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,
        0, 1,
    ];

    let (commitment, range_proof) =
        BulletproofsRangeProof::prove_bit_length(1u64, blinding, upper_bound, TEST_DOMAIN).unwrap();

    let mut range_proof_bytes = range_proof.as_bytes().to_vec();
    // Change it a little
    range_proof_bytes[0] += 1;
    let invalid_range_proof = BulletproofsRangeProof::from_bytes(&range_proof_bytes[..]).unwrap();

    assert!(invalid_range_proof
        .verify_bit_length(&commitment, upper_bound, TEST_DOMAIN)
        .is_err());
}

#[test]
fn test_handle_prove_invalid_upper_bound() {
    let invalid_upper_bound = 22;
    let blinding = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,
        0, 1,
    ];

    assert!(BulletproofsRangeProof::prove_bit_length(
        1u64,
        blinding,
        invalid_upper_bound,
        TEST_DOMAIN
    )
    .is_err());
}

#[test]
fn test_handle_verify_invalid_upper_bound() {
    let valid_upper_bound = 64;
    let invalid_upper_bound = 22;
    let blinding = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,
        0, 1,
    ];

    let (commitment, range_proof) =
        BulletproofsRangeProof::prove_bit_length(1u64, blinding, valid_upper_bound, TEST_DOMAIN)
            .unwrap();

    assert!(range_proof
        .verify_bit_length(&commitment, invalid_upper_bound, TEST_DOMAIN)
        .is_err());
}

use proptest::arbitrary::Arbitrary;

proptest::proptest! {
    #[test]
    fn proptest_0_to_2_pow_64(
        secret in <u64>::arbitrary(),
    ) {
        let upper_bound = 64;
        let blinding = [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,
            0, 1,
        ];

        let (commitment, range_proof) =
            BulletproofsRangeProof::prove_bit_length(secret, blinding, upper_bound, TEST_DOMAIN).unwrap();

        assert!(range_proof.verify_bit_length(&commitment, upper_bound, TEST_DOMAIN).is_ok());
    }
}
