// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// @generated automatically by Diesel CLI.

diesel::table! {
    tokens (message_key) {
        message_key -> Bytea,
        checkpoint -> Int8,
        epoch -> Int8,
        token_type -> Int4,
        source_chain -> Int4,
        destination_chain -> Int4,
    }
}
