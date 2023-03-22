// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{Batch, Metadata};
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
