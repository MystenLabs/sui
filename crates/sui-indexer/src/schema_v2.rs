// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// @generated automatically by Diesel CLI.

diesel::table! {
    active_addresses (address) {
        address -> Bytea,
        first_appearance_tx -> Int8,
        first_appearance_time -> Int8,
        last_appearance_tx -> Int8,
        last_appearance_time -> Int8,
    }
}

diesel::table! {
    address_metrics (checkpoint) {
        checkpoint -> Int8,
        epoch -> Int8,
        timestamp_ms -> Int8,
        cumulative_addresses -> Int8,
        cumulative_active_addresses -> Int8,
        daily_active_addresses -> Int8,
    }
}

diesel::table! {
    addresses (address) {
        address -> Bytea,
        first_appearance_tx -> Int8,
        first_appearance_time -> Int8,
        last_appearance_tx -> Int8,
        last_appearance_time -> Int8,
    }
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
    display (object_type) {
        object_type -> Text,
        id -> Bytea,
        version -> Int2,
        bcs -> Bytea,
    }
}

diesel::table! {
    epochs (epoch) {
        epoch -> Int8,
        validators -> Array<Nullable<Bytea>>,
        first_checkpoint_id -> Int8,
        epoch_start_timestamp -> Int8,
        reference_gas_price -> Int8,
        protocol_version -> Int8,
        epoch_total_transactions -> Nullable<Int8>,
        last_checkpoint_id -> Nullable<Int8>,
        epoch_end_timestamp -> Nullable<Int8>,
        storage_fund_reinvestment -> Nullable<Int8>,
        storage_charge -> Nullable<Int8>,
        storage_rebate -> Nullable<Int8>,
        storage_fund_balance -> Nullable<Int8>,
        stake_subsidy_amount -> Nullable<Int8>,
        total_gas_fees -> Nullable<Int8>,
        total_stake_rewards_distributed -> Nullable<Int8>,
        leftover_storage_fund_inflow -> Nullable<Int8>,
        new_total_stake -> Nullable<Int8>,
        epoch_commitments -> Nullable<Bytea>,
        next_epoch_reference_gas_price -> Nullable<Int8>,
        next_epoch_protocol_version -> Nullable<Int8>,
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
    move_call_metrics (id) {
        id -> Int8,
        checkpoint_sequence_number -> Int8,
        epoch -> Int8,
        day -> Int8,
        move_package -> Text,
        move_module -> Text,
        move_function -> Text,
        count -> Int8,
    }
}

diesel::table! {
    move_calls (id) {
        id -> Int8,
        transaction_sequence_number -> Int8,
        checkpoint_sequence_number -> Int8,
        epoch -> Int8,
        move_package -> Bytea,
        move_module -> Text,
        move_function -> Text,
    }
}

diesel::table! {
    network_metrics (checkpoint) {
        checkpoint -> Int8,
        epoch -> Int8,
        timestamp_ms -> Int8,
        real_time_tps -> Float8,
        peak_tps_30d -> Float8,
        total_addresses -> Int8,
        total_objects -> Int8,
        total_packages -> Int8,
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
        object_type -> Nullable<Text>,
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
    packages (package_id) {
        package_id -> Bytea,
        move_package -> Bytea,
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
    tx_calls (package, tx_sequence_number) {
        tx_sequence_number -> Int8,
        package -> Bytea,
        module -> Text,
        func -> Text,
    }
}

diesel::table! {
    tx_changed_objects (object_id, tx_sequence_number) {
        tx_sequence_number -> Int8,
        object_id -> Bytea,
    }
}

diesel::table! {
    tx_count_metrics (checkpoint_sequence_number) {
        checkpoint_sequence_number -> Int8,
        epoch -> Int8,
        timestamp_ms -> Int8,
        total_transaction_blocks -> Int8,
        total_successful_transaction_blocks -> Int8,
        total_successful_transactions -> Int8,
        network_total_transaction_blocks -> Int8,
        network_total_successful_transactions -> Int8,
        network_total_successful_transaction_blocks -> Int8,
    }
}

diesel::table! {
    tx_input_objects (object_id, tx_sequence_number) {
        tx_sequence_number -> Int8,
        object_id -> Bytea,
    }
}

diesel::table! {
    tx_recipients (recipient, tx_sequence_number) {
        tx_sequence_number -> Int8,
        recipient -> Bytea,
    }
}

diesel::table! {
    tx_senders (sender, tx_sequence_number) {
        tx_sequence_number -> Int8,
        sender -> Bytea,
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
        payers -> Array<Nullable<Bytea>>,
        recipients -> Array<Nullable<Bytea>>,
        packages -> Array<Nullable<Bytea>>,
        package_modules -> Array<Nullable<Text>>,
        package_module_functions -> Array<Nullable<Text>>,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    active_addresses,
    address_metrics,
    addresses,
    checkpoints,
    display,
    epochs,
    events,
    move_call_metrics,
    move_calls,
    network_metrics,
    objects,
    packages,
    transactions,
    tx_calls,
    tx_changed_objects,
    tx_count_metrics,
    tx_input_objects,
    tx_recipients,
    tx_senders,
    tx_indices,
);

use diesel::sql_types::Text;
diesel::sql_function! {fn query_cost(x : Text) ->Float8;}
