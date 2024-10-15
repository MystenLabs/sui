// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// @generated automatically by Diesel CLI.

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

diesel::allow_tables_to_appear_in_same_query!(
    kv_checkpoints,
    kv_objects,
);
