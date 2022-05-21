// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bincode::Options;
use crypto::ed25519::Ed25519PublicKey;
use serde_test::{assert_tokens, Token};
use types::{Batch, WorkerMessage};

#[test]
fn test_serde_batch() {
    let tx = || vec![1; 5];

    let txes: Batch = Batch((0..2).map(|_| tx()).collect());

    assert_tokens(
        &txes,
        &[
            Token::NewtypeStruct { name: "Batch" },
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
        ],
    );
}

#[test]
fn test_bincode_serde_batch() {
    let tx = || vec![1; 5];

    let txes: Batch = Batch((0..2).map(|_| tx()).collect());

    let config = bincode::DefaultOptions::new()
        .with_big_endian()
        .with_fixint_encoding();

    let txes_bytes = config.serialize(&txes).unwrap();

    // Length as u64: 0000000000000002,
    let bytes: [u8; 8] = hex::decode("0000000000000002").unwrap().try_into().unwrap();
    assert_eq!(u64::from_be_bytes(bytes), 2u64);

    // Length-prefix 2, length-prefix 5, 11111, length-prefix 5, 11111
    let expected_bytes =
        hex::decode("00000000000000020000000000000005010101010100000000000000050101010101")
            .unwrap();

    assert_eq!(
        txes_bytes.clone(),
        expected_bytes,
        "received {}",
        hex::encode(txes_bytes)
    );
}

#[test]
fn test_bincode_serde_batch_message() {
    let tx = || vec![1; 5];

    let txes: WorkerMessage<Ed25519PublicKey> =
        WorkerMessage::Batch(Batch((0..2).map(|_| tx()).collect()));

    let config = bincode::DefaultOptions::new()
        .with_big_endian()
        .with_fixint_encoding();

    let txes_bytes = config.serialize(&txes).unwrap();

    // We expect the difference with the above test will be the enum variant above on 4 bytes,
    // see https://github.com/bincode-org/bincode/blob/75a2e0bc9d35cfa7537633b07a9307bf71da84b5/src/features/serde/ser.rs#L212-L224

    // Variant index 0 (4 bytes), Length-prefix 2, length-prefix 5, 11111, length-prefix 5, 11111
    let expected_bytes =
        hex::decode("0000000000000000000000020000000000000005010101010100000000000000050101010101")
            .unwrap();

    assert_eq!(
        txes_bytes.clone(),
        expected_bytes,
        "received {}",
        hex::encode(txes_bytes)
    );
}
