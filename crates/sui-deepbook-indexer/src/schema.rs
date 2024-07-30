// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// @generated automatically by Diesel CLI.

diesel::table! {
    deep_price (digest) {
        digest -> Text,
        sender -> Text,
        target_pool -> Text,
        reference_pool -> Text,
        checkpoint -> Int8,
        timestamp -> Timestamp,
    }
}

diesel::table! {
    deepbook (digest) {
        digest -> Text,
        sender -> Text,
        checkpoint -> Int8,
    }
}

diesel::table! {
    progress_store (task_name) {
        task_name -> Text,
        checkpoint -> Int8,
        target_checkpoint -> Int8,
        timestamp -> Int8,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    deep_price,
    deepbook,
    progress_store,
);
