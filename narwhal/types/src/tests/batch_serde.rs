// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::worker::batch_serde::Token::NewtypeVariant;
use crate::{Batch, BatchV2, MetadataV1, VersionedMetadata};
use serde_test::{assert_tokens, Token};
#[test]
fn test_serde_batch() {
    let tx = || vec![1; 5];

    let batch = Batch::V2(BatchV2 {
        transactions: (0..2).map(|_| tx()).collect(),
        versioned_metadata: VersionedMetadata::V1(MetadataV1 {
            created_at: 2999309726980,
            received_at: None,
        }),
    });

    assert_tokens(
        &batch,
        &[
            NewtypeVariant {
                name: "Batch",
                variant: "V2",
            },
            Token::Struct {
                name: "BatchV2",
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
            Token::Str("versioned_metadata"),
            Token::NewtypeVariant {
                name: "VersionedMetadata",
                variant: "V1",
            },
            Token::Struct {
                name: "MetadataV1",
                len: 2,
            },
            Token::Str("created_at"),
            Token::U64(2999309726980),
            Token::Str("received_at"),
            Token::None,
            Token::StructEnd,
            Token::StructEnd,
        ],
    );
}
