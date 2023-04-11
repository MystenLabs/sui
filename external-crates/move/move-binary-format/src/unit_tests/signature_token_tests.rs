// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    deserializer::load_signature_token_test_entry,
    file_format::{SignatureToken, StructHandleIndex},
    file_format_common::{BinaryData, SerializedType, SIGNATURE_TOKEN_DEPTH_MAX},
    serializer::{serialize_signature_token, serialize_signature_token_unchecked},
};
use std::io::Cursor;

#[test]
fn serialize_and_deserialize_nested_types_max() {
    let mut ty = SignatureToken::Struct(StructHandleIndex::new(0));
    for _ in 1..SIGNATURE_TOKEN_DEPTH_MAX {
        ty = SignatureToken::Vector(Box::new(ty));
        let mut binary = BinaryData::new();
        serialize_signature_token(&mut binary, &ty).expect("serialization should succeed");

        let cursor = Cursor::new(binary.as_inner());
        load_signature_token_test_entry(cursor).expect("deserialization should succeed");
    }
}

#[test]
fn serialize_nested_types_too_deep() {
    let mut ty = SignatureToken::Struct(StructHandleIndex::new(0));
    for _ in 1..SIGNATURE_TOKEN_DEPTH_MAX {
        ty = SignatureToken::Vector(Box::new(ty));
    }

    for _ in 0..10 {
        ty = SignatureToken::Vector(Box::new(ty));

        let mut binary = BinaryData::new();
        serialize_signature_token(&mut binary, &ty).expect_err("serialization should fail");

        let mut binary = BinaryData::new();
        serialize_signature_token_unchecked(&mut binary, &ty)
            .expect("serialization (unchecked) should succeed");

        let cursor = Cursor::new(binary.as_inner());
        load_signature_token_test_entry(cursor).expect_err("deserialization should fail");
    }
}

#[test]
fn deserialize_struct_inst_arity_0() {
    let cursor = Cursor::new(
        [
            SerializedType::STRUCT_INST as u8,
            0x0, /* struct handle idx */
            0x0, /* arity */
            SerializedType::BOOL as u8,
        ]
        .as_slice(),
    );
    load_signature_token_test_entry(cursor).expect_err("deserialization should fail");
}

#[test]
fn deserialize_struct_inst_arity_1() {
    let cursor = Cursor::new(
        [
            SerializedType::STRUCT_INST as u8,
            0x0, /* struct handle idx */
            0x1, /* arity */
            SerializedType::BOOL as u8,
        ]
        .as_slice(),
    );
    load_signature_token_test_entry(cursor).expect("deserialization should succeed");
}

#[test]
fn deserialize_struct_inst_arity_2() {
    let cursor = Cursor::new(
        [
            SerializedType::STRUCT_INST as u8,
            0x0, /* struct handle idx */
            0x2, /* arity */
            SerializedType::BOOL as u8,
            SerializedType::BOOL as u8,
        ]
        .as_slice(),
    );
    load_signature_token_test_entry(cursor).expect("deserialization should succeed");
}
