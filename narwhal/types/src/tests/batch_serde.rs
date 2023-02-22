// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{serialized_batch_digest, Batch, Metadata, WorkerBatchMessage};
use fastcrypto::{
    encoding::{Encoding, Hex},
    hash::Hash,
};
use proptest::arbitrary::Arbitrary;
use serde_test::{assert_tokens, Token};

#[test]
fn test_serde_batch() {
    let tx = || vec![1; 5];

    let batch = Batch {
        transactions: (0..2).map(|_| tx()).collect(),
        metadata: Metadata {
            created_at: 1666205365890,
        },
    };

    assert_tokens(
        &batch,
        &[
            Token::Struct {
                name: "Batch",
                len: 2,
            },
            Token::Str("transactions"),
            Token::Seq { len: Some(2) },
            Token::Seq { len: Some(5) },
            Token::U8(1),
            Token::U8(1),
            Token::U8(1),
            Token::U8(1),
            Token::U8(1),
            Token::SeqEnd,
            Token::Seq { len: Some(5) },
            Token::U8(1),
            Token::U8(1),
            Token::U8(1),
            Token::U8(1),
            Token::U8(1),
            Token::SeqEnd,
            Token::SeqEnd,
            Token::Str("metadata"),
            Token::Struct {
                name: "Metadata",
                len: 1,
            },
            Token::Str("created_at"),
            Token::U64(1666205365890),
            Token::StructEnd,
            Token::StructEnd,
        ],
    );
}

#[test]
fn test_bincode_serde_batch() {
    let tx = || vec![1; 5];

    let txes = Batch {
        transactions: (0..2).map(|_| tx()).collect(),
        metadata: Metadata {
            created_at: 1666205365890,
        },
    };

    let txes_bytes = bincode::serialize(&txes).unwrap();

    // Length as u64: 0000000000000002,
    let bytes: [u8; 8] = Hex::decode("0200000000000000").unwrap().try_into().unwrap();
    assert_eq!(u64::from_le_bytes(bytes), 2u64);

    // Length-prefix 2, length-prefix 5, 11111, length-prefix 5, 11111,
    let expected_bytes = Hex::decode(
        "02000000000000000500000000000000010101010105000000000000000101010101823694f183010000",
    )
    .unwrap();

    assert_eq!(
        txes_bytes.clone(),
        expected_bytes,
        "received {}",
        Hex::encode(txes_bytes)
    );
}

#[test]
fn test_bincode_serde_batch_message() {
    let tx = || vec![1; 5];

    let txes = WorkerBatchMessage {
        batch: Batch {
            transactions: (0..2).map(|_| tx()).collect(),
            metadata: Metadata {
                created_at: 1666205365890,
            },
        },
    };

    let txes_bytes = bincode::serialize(&txes).unwrap();

    // We expect this will be the same as the above.
    // Length-prefix 2, length-prefix 5, 11111, length-prefix 5, 11111
    let expected_bytes = Hex::decode(
        "02000000000000000500000000000000010101010105000000000000000101010101823694f183010000",
    )
    .unwrap();

    assert_eq!(
        txes_bytes.clone(),
        expected_bytes,
        "received {}",
        Hex::encode(txes_bytes)
    );
}

proptest::proptest! {

    #[test]
    fn test_batch_and_serialized(
        batch in Batch::arbitrary()
    ) {
        let digest = batch.digest();
        let message = WorkerBatchMessage{batch};
        let serialized = bincode::serialize(&message).expect("Failed to serialize our own batch");
        let digest_from_serialized = serialized_batch_digest(serialized).expect("Failed to hash serialized batch");
        assert_eq!(digest, digest_from_serialized);
    }
}
