// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

diesel::table! {
    error_logs (id) {
        id -> Int8,
        error_type -> Varchar,
        error -> Text,
        error_time -> Timestamp,
    }
}

diesel::table! {
    event_logs (id) {
        id -> Int4,
        next_cursor_tx_seq -> Nullable<Int8>,
        next_cursor_event_seq -> Nullable<Int8>,
    }
}

diesel::table! {
    events (id) {
        id -> Int8,
        transaction_digest -> Nullable<Varchar>,
        transaction_sequence -> Int8,
        event_sequence -> Int8,
        event_time -> Nullable<Timestamp>,
        event_type -> Varchar,
        event_content -> Varchar,
    }
}

diesel::table! {
    transaction_logs (id) {
        id -> Int4,
        next_cursor_tx_digest -> Nullable<Text>,
    }
}

diesel::table! {
    transactions (id) {
        id -> Int8,
        transaction_digest -> Varchar,
        sender -> Varchar,
        transaction_time -> Nullable<Timestamp>,
        transaction_kinds -> Array<Nullable<Text>>,
        created -> Array<Nullable<Text>>,
        mutated -> Array<Nullable<Text>>,
        deleted -> Array<Nullable<Text>>,
        unwrapped -> Array<Nullable<Text>>,
        wrapped -> Array<Nullable<Text>>,
        gas_object_id -> Varchar,
        gas_object_sequence -> Int8,
        gas_object_digest -> Varchar,
        gas_budget -> Int8,
        total_gas_cost -> Int8,
        computation_cost -> Int8,
        storage_cost -> Int8,
        storage_rebate -> Int8,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    error_logs,
    event_logs,
    events,
    transaction_logs,
    transactions,
);
