// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// @generated automatically by Diesel CLI.

diesel::table! {
    address_logs (last_processed_id) {
        last_processed_id -> Int8,
    }
}

diesel::table! {
    addresses (id) {
        id -> Int8,
        account_address -> Varchar,
        first_appearance_tx -> Varchar,
        first_appearance_time -> Nullable<Timestamp>,
    }
}

diesel::table! {
    checkpoint_logs (next_cursor_sequence_number) {
        next_cursor_sequence_number -> Int8,
    }
}

diesel::table! {
    checkpoints (sequence_number) {
        sequence_number -> Int8,
        content_digest -> Varchar,
        epoch -> Int8,
        total_gas_cost -> Int8,
        total_computation_cost -> Int8,
        total_storage_cost -> Int8,
        total_storage_rebate -> Int8,
        total_transactions -> Int8,
        previous_digest -> Nullable<Varchar>,
        next_epoch_committee -> Nullable<Text>,
        timestamp_ms -> Int8,
    }
}

diesel::table! {
    error_logs (id) {
        id -> Int8,
        error_type -> Varchar,
        error -> Text,
        error_time -> Timestamp,
    }
}

diesel::table! {
    events (id) {
        id -> Int8,
        transaction_digest -> Varchar,
        event_sequence -> Int8,
        event_time -> Nullable<Timestamp>,
        event_type -> Varchar,
        event_content -> Varchar,
        next_cursor_transaction_digest -> Nullable<Varchar>,
    }
}

diesel::table! {
    object_event_logs (id) {
        id -> Int4,
        next_cursor_tx_dig -> Nullable<Text>,
        next_cursor_event_seq -> Nullable<Int8>,
    }
}

diesel::table! {
    object_events (id) {
        id -> Int8,
        transaction_digest -> Nullable<Varchar>,
        event_sequence -> Int8,
        event_time -> Nullable<Timestamp>,
        event_type -> Varchar,
        event_content -> Varchar,
    }
}

diesel::table! {
    object_logs (last_processed_id) {
        last_processed_id -> Int8,
    }
}

diesel::table! {
    objects (id) {
        id -> Int8,
        object_id -> Varchar,
        version -> Int8,
        owner_type -> Varchar,
        owner_address -> Nullable<Varchar>,
        initial_shared_version -> Nullable<Int8>,
        package_id -> Text,
        transaction_module -> Text,
        object_type -> Nullable<Text>,
        object_status -> Varchar,
    }
}

diesel::table! {
    package_logs (last_processed_id) {
        last_processed_id -> Int8,
    }
}

diesel::table! {
    packages (id) {
        id -> Int8,
        package_id -> Text,
        author -> Text,
        module_names -> Array<Nullable<Text>>,
        package_content -> Text,
    }
}

diesel::table! {
    publish_event_logs (id) {
        id -> Int4,
        next_cursor_tx_dig -> Nullable<Text>,
        next_cursor_event_seq -> Nullable<Int8>,
    }
}

diesel::table! {
    publish_events (id) {
        id -> Int8,
        transaction_digest -> Nullable<Varchar>,
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
        gas_price -> Int8,
        transaction_content -> Text,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    address_logs,
    addresses,
    checkpoint_logs,
    checkpoints,
    error_logs,
    events,
    object_event_logs,
    object_events,
    object_logs,
    objects,
    package_logs,
    packages,
    publish_event_logs,
    publish_events,
    transaction_logs,
    transactions,
);
