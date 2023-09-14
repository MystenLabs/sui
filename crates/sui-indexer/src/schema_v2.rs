// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// @generated automatically by Diesel CLI.

pub mod sql_types {
    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "bcs_bytes"))]
    pub struct BcsBytes;
}

diesel::table! {
    checkpoints (sequence_number) {
        sequence_number -> Int8,
        checkpoint_digest -> Bytea,
        epoch -> Int8,
        network_total_transactions -> Int8,
        previous_checkpoint_digest -> Nullable<Bytea>,
        end_of_epoch -> Bool,
        tx_digests -> Array<Nullable<Bytea>>,
        timestamp_ms -> Int8,
        total_gas_cost -> Int8,
        computation_cost -> Int8,
        storage_cost -> Int8,
        storage_rebate -> Int8,
        non_refundable_storage_fee -> Int8,
        checkpoint_commitments -> Bytea,
        validator_signature -> Bytea,
        end_of_epoch_data -> Nullable<Bytea>,
    }
}

diesel::table! {
    epochs (epoch) {
        epoch -> Int8,
        validators -> Array<Nullable<Bytea>>,
        epoch_total_transactions -> Int8,
        first_checkpoint_id -> Int8,
        epoch_start_timestamp -> Int8,
        reference_gas_price -> Int8,
        protocol_version -> Int8,
        last_checkpoint_id -> Nullable<Int8>,
        epoch_end_timestamp -> Nullable<Int8>,
        storage_fund_reinvestment -> Nullable<Int8>,
        storage_charge -> Nullable<Int8>,
        storage_rebate -> Nullable<Int8>,
        storage_fund_balance -> Nullable<Int8>,
        stake_subsidy_amount -> Nullable<Int8>,
        total_gas_fees -> Nullable<Int8>,
        total_stake_rewards_distributed -> Nullable<Int8>,
        leftover_storage_fund_inflow-> Nullable<Int8>,
        new_total_stake -> Nullable<Int8>,
        epoch_commitments -> Nullable<Bytea>,
    }
}

diesel::table! {
    events (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        checkpoint_sequence_number -> Int8,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
    }
}

diesel::table! {
    objects (object_id) {
        object_id -> Bytea,
        object_version -> Int8,
        object_digest -> Bytea,
        checkpoint_sequence_number -> Int8,
        owner_type -> Int2,
        owner_id -> Nullable<Bytea>,
        serialized_object -> Bytea,
        coin_type -> Nullable<Text>,
        coin_balance -> Nullable<Int8>,
        df_kind -> Nullable<Int2>,
        df_name -> Nullable<Bytea>,
        df_object_type -> Nullable<Text>,
        df_object_id -> Nullable<Bytea>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::BcsBytes;

    packages (package_id) {
        package_id -> Bytea,
        modules -> Array<Nullable<BcsBytes>>,
    }
}

diesel::table! {
    transactions (tx_sequence_number) {
        tx_sequence_number -> Int8,
        transaction_digest -> Bytea,
        raw_transaction -> Bytea,
        raw_effects -> Bytea,
        checkpoint_sequence_number -> Int8,
        timestamp_ms -> Int8,
        object_changes -> Array<Nullable<Bytea>>,
        balance_changes -> Array<Nullable<Bytea>>,
        events -> Array<Nullable<Bytea>>,
        transaction_kind -> Int2,
    }
}

diesel::table! {
    tx_indices (tx_sequence_number) {
        tx_sequence_number -> Int8,
        checkpoint_sequence_number -> Int8,
        transaction_digest -> Bytea,
        input_objects -> Array<Nullable<Bytea>>,
        changed_objects -> Array<Nullable<Bytea>>,
        senders -> Array<Nullable<Bytea>>,
        recipients -> Array<Nullable<Bytea>>,
        packages -> Array<Nullable<Bytea>>,
        package_modules -> Array<Nullable<Text>>,
        package_module_functions -> Array<Nullable<Text>>,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    checkpoints,
    epochs,
    events,
    objects,
    packages,
    transactions,
    tx_indices,
);
