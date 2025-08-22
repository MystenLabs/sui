// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// @generated automatically by Diesel CLI.

diesel::table! {
    blog_post (dynamic_field_id) {
        dynamic_field_id -> Bytea,
        df_version -> Int8,
        publisher -> Bytea,
        blob_obj_id -> Bytea,
        view_count -> Int8,
        title -> Text,
    }
}
