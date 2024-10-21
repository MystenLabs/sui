// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// @generated automatically by Diesel CLI.

diesel::table! {
    ev_emit_mod (package, module, tx_sequence_number) {
        package -> Bytea,
        module -> Text,
        tx_sequence_number -> Int8,
        sender -> Bytea,
    }
}

diesel::table! {
    ev_emit_pkg (package, tx_sequence_number) {
        package -> Bytea,
        tx_sequence_number -> Int8,
        sender -> Bytea,
    }
}

diesel::table! {
    ev_struct_inst (package, module, instantiation, tx_sequence_number) {
        package -> Bytea,
        module -> Text,
        name -> Nullable<Text>,
        instantiation -> Bytea,
        tx_sequence_number -> Int8,
        sender -> Bytea,
    }
}

diesel::table! {
    ev_struct_mod (package, module, tx_sequence_number) {
        package -> Bytea,
        module -> Text,
        tx_sequence_number -> Int8,
        sender -> Bytea,
    }
}

diesel::table! {
    ev_struct_name (package, module, name, tx_sequence_number) {
        package -> Bytea,
        module -> Text,
        name -> Text,
        tx_sequence_number -> Int8,
        sender -> Bytea,
    }
}

diesel::table! {
    ev_struct_pkg (package, tx_sequence_number) {
        package -> Bytea,
        tx_sequence_number -> Int8,
        sender -> Bytea,
    }
}

diesel::table! {
    kv_checkpoints (sequence_number) {
        sequence_number -> Int8,
        certified_checkpoint -> Bytea,
        checkpoint_contents -> Bytea,
    }
}

diesel::table! {
    kv_objects (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        serialized_object -> Nullable<Bytea>,
    }
}

diesel::table! {
    kv_transactions (tx_digest) {
        tx_digest -> Bytea,
        cp_sequence_number -> Int8,
        timestamp_ms -> Int8,
        raw_transaction -> Bytea,
        raw_effects -> Bytea,
        events -> Bytea,
    }
}

diesel::table! {
    tx_affected_objects (affected, tx_sequence_number) {
        tx_sequence_number -> Int8,
        affected -> Bytea,
        sender -> Bytea,
    }
}

diesel::table! {
    tx_balance_changes (tx_sequence_number) {
        tx_sequence_number -> Int8,
        balance_changes -> Bytea,
    }
}

diesel::table! {
    watermarks (pipeline) {
        pipeline -> Text,
        epoch_hi_inclusive -> Int8,
        checkpoint_hi_inclusive -> Int8,
        tx_hi -> Int8,
        epoch_lo -> Int8,
        reader_lo -> Int8,
        timestamp_ms -> Int8,
        pruner_hi -> Int8,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    ev_emit_mod,
    ev_emit_pkg,
    ev_struct_inst,
    ev_struct_mod,
    ev_struct_name,
    ev_struct_pkg,
    kv_checkpoints,
    kv_objects,
    kv_transactions,
    tx_affected_objects,
    tx_balance_changes,
    watermarks,
);
