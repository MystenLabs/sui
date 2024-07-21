// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// @generated automatically by Diesel CLI.

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
    pruner_cp_watermark (checkpoint_sequence_number) {
        checkpoint_sequence_number -> Int8,
        min_tx_sequence_number -> Int8,
        max_tx_sequence_number -> Int8,
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
        first_checkpoint_id -> Int8,
        epoch_start_timestamp -> Int8,
        reference_gas_price -> Int8,
        protocol_version -> Int8,
        total_stake -> Int8,
        storage_fund_balance -> Int8,
        system_state -> Bytea,
        epoch_total_transactions -> Nullable<Int8>,
        last_checkpoint_id -> Nullable<Int8>,
        epoch_end_timestamp -> Nullable<Int8>,
        storage_fund_reinvestment -> Nullable<Int8>,
        storage_charge -> Nullable<Int8>,
        storage_rebate -> Nullable<Int8>,
        stake_subsidy_amount -> Nullable<Int8>,
        total_gas_fees -> Nullable<Int8>,
        total_stake_rewards_distributed -> Nullable<Int8>,
        leftover_storage_fund_inflow -> Nullable<Int8>,
        epoch_commitments -> Nullable<Bytea>,
    }
}

diesel::table! {
    events (tx_sequence_number, event_sequence_number, checkpoint_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        checkpoint_sequence_number -> Int8,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        event_type_package -> Bytea,
        event_type_module -> Text,
        event_type_name -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
    }
}

diesel::table! {
    events_partition_0 (tx_sequence_number, event_sequence_number, checkpoint_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        checkpoint_sequence_number -> Int8,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        event_type_package -> Bytea,
        event_type_module -> Text,
        event_type_name -> Text,
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
        object_type -> Nullable<Text>,
        object_type_package -> Nullable<Bytea>,
        object_type_module -> Nullable<Text>,
        object_type_name -> Nullable<Text>,
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
    objects_history (checkpoint_sequence_number, object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        object_status -> Int2,
        object_digest -> Nullable<Bytea>,
        checkpoint_sequence_number -> Int8,
        owner_type -> Nullable<Int2>,
        owner_id -> Nullable<Bytea>,
        object_type -> Nullable<Text>,
        object_type_package -> Nullable<Bytea>,
        object_type_module -> Nullable<Text>,
        object_type_name -> Nullable<Text>,
        serialized_object -> Nullable<Bytea>,
        coin_type -> Nullable<Text>,
        coin_balance -> Nullable<Int8>,
        df_kind -> Nullable<Int2>,
        df_name -> Nullable<Bytea>,
        df_object_type -> Nullable<Text>,
        df_object_id -> Nullable<Bytea>,
    }
}

diesel::table! {
    objects_history_partition_0 (checkpoint_sequence_number, object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        object_status -> Int2,
        object_digest -> Nullable<Bytea>,
        checkpoint_sequence_number -> Int8,
        owner_type -> Nullable<Int2>,
        owner_id -> Nullable<Bytea>,
        object_type -> Nullable<Text>,
        object_type_package -> Nullable<Bytea>,
        object_type_module -> Nullable<Text>,
        object_type_name -> Nullable<Text>,
        serialized_object -> Nullable<Bytea>,
        coin_type -> Nullable<Text>,
        coin_balance -> Nullable<Int8>,
        df_kind -> Nullable<Int2>,
        df_name -> Nullable<Bytea>,
        df_object_type -> Nullable<Text>,
        df_object_id -> Nullable<Bytea>,
    }
}

diesel::table! {
    objects_snapshot (object_id) {
        object_id -> Bytea,
        object_version -> Int8,
        object_status -> Int2,
        object_digest -> Nullable<Bytea>,
        checkpoint_sequence_number -> Int8,
        owner_type -> Nullable<Int2>,
        owner_id -> Nullable<Bytea>,
        object_type -> Nullable<Text>,
        object_type_package -> Nullable<Bytea>,
        object_type_module -> Nullable<Text>,
        object_type_name -> Nullable<Text>,
        serialized_object -> Nullable<Bytea>,
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
    transactions (tx_sequence_number, checkpoint_sequence_number) {
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
        success_command_count -> Int2,
    }
}

diesel::table! {
    transactions_partition_0 (tx_sequence_number, checkpoint_sequence_number) {
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
        success_command_count -> Int2,
    }
}

diesel::table! {
    tx_calls (package, tx_sequence_number, cp_sequence_number) {
        cp_sequence_number -> Int8,
        tx_sequence_number -> Int8,
        package -> Bytea,
        module -> Text,
        func -> Text,
    }
}

diesel::table! {
    tx_changed_objects (object_id, tx_sequence_number, cp_sequence_number) {
        cp_sequence_number -> Int8,
        tx_sequence_number -> Int8,
        object_id -> Bytea,
    }
}

diesel::table! {
    tx_digests (tx_digest) {
        tx_digest -> Bytea,
        cp_sequence_number -> Int8,
        tx_sequence_number -> Int8,
    }
}

diesel::table! {
    tx_input_objects (object_id, tx_sequence_number, cp_sequence_number) {
        cp_sequence_number -> Int8,
        tx_sequence_number -> Int8,
        object_id -> Bytea,
    }
}

diesel::table! {
    tx_recipients (recipient, tx_sequence_number, cp_sequence_number) {
        cp_sequence_number -> Int8,
        tx_sequence_number -> Int8,
        recipient -> Bytea,
    }
}

diesel::table! {
    tx_senders (sender, tx_sequence_number, cp_sequence_number) {
        cp_sequence_number -> Int8,
        tx_sequence_number -> Int8,
        sender -> Bytea,
    }
}

#[macro_export]
macro_rules! for_all_tables {
    ($action:path) => {
        $action!(
            checkpoints,
            pruner_cp_watermark,
            display,
            epochs,
            events,
            events_partition_0,
            objects,
            objects_history,
            objects_history_partition_0,
            objects_snapshot,
            packages,
            transactions,
            transactions_partition_0,
            tx_calls,
            tx_changed_objects,
            tx_digests,
            tx_input_objects,
            tx_recipients,
            tx_senders
        );
    };
}
pub use for_all_tables;

for_all_tables!(diesel::allow_tables_to_appear_in_same_query);
