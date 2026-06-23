// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Diesel table definitions for the backtest's postgres sink. The watermark tables the framework's
//! committer relies on come from `sui-pg-db`'s own migrations, so only the backtest-specific tables
//! are declared here.

diesel::table! {
    divergence (task, tx_digest) {
        task -> Text,
        epoch -> Int8,
        checkpoint -> Int8,
        tx_digest -> Text,
        original_status -> Text,
        original_failure_kind -> Nullable<Text>,
        recomputed_status -> Text,
        recomputed_error_kind -> Nullable<Text>,
        recomputed_error_detail -> Nullable<Text>,
        missing_modified -> Int8,
        missing_loaded -> Int8,
        missing_consensus -> Int8,
        digest_mismatches -> Int8,
    }
}

diesel::table! {
    run_stats (task, checkpoint) {
        task -> Text,
        epoch -> Int8,
        checkpoint -> Int8,
        checked -> Int8,
        executed -> Int8,
        divergences -> Int8,
        reconstruction_errors -> Int8,
        coin_reservation_skipped -> Int8,
        execute_skipped -> Int8,
        gas_from_balance -> Int8,
        cancellation_excluded -> Int8,
    }
}

diesel::allow_tables_to_appear_in_same_query!(divergence, run_stats);
