// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// @generated automatically by Diesel CLI.

diesel::table! {
    events (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Bigint,
        event_sequence_number -> Bigint,
        transaction_digest -> Blob,
        checkpoint_sequence_number -> Bigint,
        senders -> Json,
        package -> Blob,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Bigint,
        bcs -> Blob,
    }
}

diesel::table! {
    objects (object_id) {
        object_id -> Blob,
        object_version -> Bigint,
        object_digest -> Blob,
        checkpoint_sequence_number -> Bigint,
        owner_type -> Smallint,
        owner_id -> Nullable<Blob>,
        object_type -> Nullable<Text>,
        serialized_object -> Blob,
        coin_type -> Nullable<Text>,
        coin_balance -> Nullable<Bigint>,
        df_kind -> Nullable<Smallint>,
        df_name -> Nullable<Blob>,
        df_object_type -> Nullable<Text>,
        df_object_id -> Nullable<Blob>,
    }
}

diesel::table! {
    objects_history (checkpoint_sequence_number, object_id, object_version) {
        object_id -> Blob,
        object_version -> Bigint,
        object_status -> Smallint,
        object_digest -> Nullable<Blob>,
        checkpoint_sequence_number -> Bigint,
        owner_type -> Nullable<Smallint>,
        owner_id -> Nullable<Blob>,
        object_type -> Nullable<Text>,
        serialized_object -> Nullable<Blob>,
        coin_type -> Nullable<Text>,
        coin_balance -> Nullable<Bigint>,
        df_kind -> Nullable<Smallint>,
        df_name -> Nullable<Blob>,
        df_object_type -> Nullable<Text>,
        df_object_id -> Nullable<Blob>,
    }
}

diesel::table! {
    objects_snapshot (object_id) {
        object_id -> Blob,
        object_version -> Bigint,
        object_status -> Smallint,
        object_digest -> Nullable<Blob>,
        checkpoint_sequence_number -> Bigint,
        owner_type -> Nullable<Smallint>,
        owner_id -> Nullable<Blob>,
        object_type -> Nullable<Text>,
        serialized_object -> Nullable<Blob>,
        coin_type -> Nullable<Text>,
        coin_balance -> Nullable<Bigint>,
        df_kind -> Nullable<Smallint>,
        df_name -> Nullable<Blob>,
        df_object_type -> Nullable<Text>,
        df_object_id -> Nullable<Blob>,
    }
}

diesel::table! {
    transactions (tx_sequence_number, checkpoint_sequence_number) {
        tx_sequence_number -> Bigint,
        transaction_digest -> Blob,
        raw_transaction -> Blob,
        raw_effects -> Blob,
        checkpoint_sequence_number -> Bigint,
        timestamp_ms -> Bigint,
        object_changes -> Json,
        balance_changes -> Json,
        events -> Json,
        transaction_kind -> Smallint,
        success_command_count -> Smallint,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    events,
    objects,
    objects_history,
    objects_snapshot,
    transactions,
);
