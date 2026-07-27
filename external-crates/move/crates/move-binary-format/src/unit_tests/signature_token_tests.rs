// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    deserializer::{load_signature_token_test_entry, load_signature_token_test_entry_with_version},
    file_format::{DatatypeHandleIndex, SignatureToken},
    file_format_common::{
        BinaryData, SIGNATURE_TOKEN_DEPTH_MAX, SerializedType, VERSION_7, VERSION_8, VERSION_MAX,
    },
    serializer::{serialize_signature_token, serialize_signature_token_unchecked},
};
use move_core_types::vm_status::StatusCode;
use std::io::Cursor;

#[test]
fn serialize_and_deserialize_nested_types_max() {
    let mut ty = SignatureToken::Datatype(DatatypeHandleIndex::new(0));
    for _ in 1..SIGNATURE_TOKEN_DEPTH_MAX {
        ty = SignatureToken::Vector(Box::new(ty));
        let mut binary = BinaryData::new();
        serialize_signature_token(VERSION_MAX, &mut binary, &ty)
            .expect("serialization should succeed");

        let cursor = Cursor::new(binary.as_inner());
        load_signature_token_test_entry(cursor).expect("deserialization should succeed");
    }
}

#[test]
fn serialize_nested_types_too_deep() {
    let mut ty = SignatureToken::Datatype(DatatypeHandleIndex::new(0));
    for _ in 1..SIGNATURE_TOKEN_DEPTH_MAX {
        ty = SignatureToken::Vector(Box::new(ty));
    }

    for _ in 0..10 {
        ty = SignatureToken::Vector(Box::new(ty));

        let mut binary = BinaryData::new();
        serialize_signature_token(VERSION_MAX, &mut binary, &ty)
            .expect_err("serialization should fail");

        let mut binary = BinaryData::new();
        serialize_signature_token_unchecked(VERSION_MAX, &mut binary, &ty)
            .expect("serialization (unchecked) should succeed");

        let cursor = Cursor::new(binary.as_inner());
        load_signature_token_test_entry(cursor).expect_err("deserialization should fail");
    }
}

#[test]
fn deserialize_datatype_inst_arity_0() {
    let cursor = Cursor::new(
        [
            SerializedType::DATATYPE_INST as u8,
            0x0, /* datatype handle idx */
            0x0, /* arity */
            SerializedType::BOOL as u8,
        ]
        .as_slice(),
    );
    load_signature_token_test_entry(cursor).expect_err("deserialization should fail");
}

#[test]
fn deserialize_datatype_inst_arity_1() {
    let cursor = Cursor::new(
        [
            SerializedType::DATATYPE_INST as u8,
            0x0, /* datatype handle idx */
            0x1, /* arity */
            SerializedType::BOOL as u8,
        ]
        .as_slice(),
    );
    load_signature_token_test_entry(cursor).expect("deserialization should succeed");
}

#[test]
fn deserialize_datatype_inst_arity_2() {
    let cursor = Cursor::new(
        [
            SerializedType::DATATYPE_INST as u8,
            0x0, /* datatype handle idx */
            0x2, /* arity */
            SerializedType::BOOL as u8,
            SerializedType::BOOL as u8,
        ]
        .as_slice(),
    );
    load_signature_token_test_entry(cursor).expect("deserialization should succeed");
}

const SIGNED_TOKENS: [SignatureToken; 6] = [
    SignatureToken::I8,
    SignatureToken::I16,
    SignatureToken::I32,
    SignatureToken::I64,
    SignatureToken::I128,
    SignatureToken::I256,
];

#[test]
fn serialize_signed_token_rejected_below_version_8() {
    for ty in SIGNED_TOKENS {
        let mut binary = BinaryData::new();
        let err = serialize_signature_token(VERSION_7, &mut binary, &ty)
            .expect_err("signed tokens must not serialize below VERSION_8");
        assert!(
            err.to_string().contains("Signed integer types"),
            "unexpected error for {ty:?}: {err}"
        );
        // Nested occurrences are caught too (the gate is at the token peel-off).
        let mut binary = BinaryData::new();
        serialize_signature_token(
            VERSION_7,
            &mut binary,
            &SignatureToken::Vector(Box::new(ty.clone())),
        )
        .expect_err("nested signed tokens must not serialize below VERSION_8");
    }
}

#[test]
fn signed_token_round_trips_at_version_8() {
    for ty in SIGNED_TOKENS {
        let ty = SignatureToken::Vector(Box::new(ty));
        let mut binary = BinaryData::new();
        serialize_signature_token(VERSION_8, &mut binary, &ty)
            .expect("signed tokens serialize at VERSION_8");

        // A VERSION_8 reader accepts the token...
        let cursor = Cursor::new(binary.as_inner());
        let loaded = load_signature_token_test_entry_with_version(VERSION_8, cursor)
            .expect("signed tokens deserialize at VERSION_8");
        assert_eq!(loaded, ty);

        // ...while a VERSION_7 reader rejects the same bytes as MALFORMED.
        let cursor = Cursor::new(binary.as_inner());
        let err = load_signature_token_test_entry_with_version(VERSION_7, cursor)
            .expect_err("signed tokens must not deserialize below VERSION_8");
        assert_eq!(err.major_status(), StatusCode::MALFORMED);
    }
}
