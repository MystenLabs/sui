// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// @generated automatically by Diesel CLI.

diesel::table! {
    progress_store (task_name) {
        task_name -> Text,
        checkpoint -> Int8,
        target_checkpoint -> Int8,
        timestamp -> Int8,
    }
}

diesel::table! {
    deepbook (digest) {
        sender -> Text,
        digest -> Text,
        checkpoint -> Int8,
    }
}
