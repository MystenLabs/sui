// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// @generated automatically by Diesel CLI.

diesel::table! {
    progress_store (task_name) {
        task_name -> Text,
        checkpoint -> Int8,
    }
}

diesel::table! {
    token_transfer (chain_id, nonce, status) {
        chain_id -> Int4,
        nonce -> Int8,
        status -> Text,
        block_height -> Int8,
        timestamp_ms -> Int8,
        txn_hash -> Bytea,
        txn_sender -> Bytea,
        gas_usage -> Int8,
        data_source -> Text,
    }
}

diesel::table! {
    token_transfer_data (chain_id, nonce) {
        chain_id -> Int4,
        nonce -> Int8,
        block_height -> Int8,
        timestamp_ms -> Int8,
        txn_hash -> Bytea,
        sender_address -> Bytea,
        destination_chain -> Int4,
        recipient_address -> Bytea,
        token_id -> Int4,
        amount -> Int8,
    }
}

diesel::allow_tables_to_appear_in_same_query!(progress_store, token_transfer, token_transfer_data,);
