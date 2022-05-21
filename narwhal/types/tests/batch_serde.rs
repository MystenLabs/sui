// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde_test::{assert_tokens, Token};
use types::Batch;

#[test]
fn test_ser_de() {
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
