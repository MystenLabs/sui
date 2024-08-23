// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// @generated automatically by Diesel CLI.

diesel::table! {
    chain_identifier (checkpoint_digest) {
        checkpoint_digest -> Blob,
    }
}

diesel::table! {
    checkpoints (sequence_number) {
        sequence_number -> Bigint,
        checkpoint_digest -> Blob,
        epoch -> Bigint,
        network_total_transactions -> Bigint,
        previous_checkpoint_digest -> Nullable<Blob>,
        end_of_epoch -> Bool,
        tx_digests -> Json,
        timestamp_ms -> Bigint,
        total_gas_cost -> Bigint,
        computation_cost -> Bigint,
        storage_cost -> Bigint,
        storage_rebate -> Bigint,
        non_refundable_storage_fee -> Bigint,
        checkpoint_commitments -> Mediumblob,
        validator_signature -> Blob,
        end_of_epoch_data -> Nullable<Blob>,
        min_tx_sequence_number -> Nullable<Bigint>,
        max_tx_sequence_number -> Nullable<Bigint>
    }
}

diesel::table! {
    display (object_type) {
        object_type -> Text,
        id -> Blob,
        version -> Smallint,
        bcs -> Mediumblob,
    }
}

diesel::table! {
    epochs (epoch) {
        epoch -> Bigint,
        first_checkpoint_id -> Bigint,
        epoch_start_timestamp -> Bigint,
        reference_gas_price -> Bigint,
        protocol_version -> Bigint,
        total_stake -> Bigint,
        storage_fund_balance -> Bigint,
        system_state -> Mediumblob,
        epoch_total_transactions -> Nullable<Bigint>,
        last_checkpoint_id -> Nullable<Bigint>,
        epoch_end_timestamp -> Nullable<Bigint>,
        storage_fund_reinvestment -> Nullable<Bigint>,
        storage_charge -> Nullable<Bigint>,
        storage_rebate -> Nullable<Bigint>,
        stake_subsidy_amount -> Nullable<Bigint>,
        total_gas_fees -> Nullable<Bigint>,
        total_stake_rewards_distributed -> Nullable<Bigint>,
        leftover_storage_fund_inflow -> Nullable<Bigint>,
        epoch_commitments -> Nullable<Blob>,
    }
}

diesel::table! {
    events (tx_sequence_number, event_sequence_number, checkpoint_sequence_number) {
        tx_sequence_number -> Bigint,
        event_sequence_number -> Bigint,
        transaction_digest -> Blob,
        checkpoint_sequence_number -> Bigint,
        senders -> Json,
        package -> Blob,
        module -> Text,
        event_type -> Text,
        event_type_package -> Blob,
        event_type_module -> Text,
        event_type_name -> Text,
        timestamp_ms -> Bigint,
        bcs -> Mediumblob,
    }
}

diesel::table! {
    event_emit_module (package, module, tx_sequence_number, event_sequence_number) {
        package -> Blob,
        module -> Text,
        tx_sequence_number -> Bigint,
        event_sequence_number -> Bigint,
        sender -> Blob,
    }
}

diesel::table! {
    event_emit_package (package, tx_sequence_number, event_sequence_number) {
        package -> Blob,
        tx_sequence_number -> Bigint,
        event_sequence_number -> Bigint,
        sender -> Blob,
    }
}

diesel::table! {
    event_senders (sender, tx_sequence_number, event_sequence_number) {
        sender -> Blob,
        tx_sequence_number -> Bigint,
        event_sequence_number -> Bigint,
    }
}

diesel::table! {
    event_struct_instantiation (package, module, type_instantiation, tx_sequence_number, event_sequence_number) {
        package -> Blob,
        module -> Text,
        type_instantiation -> Text,
        tx_sequence_number -> Bigint,
        event_sequence_number -> Bigint,
        sender -> Blob,
    }
}

diesel::table! {
    event_struct_module (package, module, tx_sequence_number, event_sequence_number) {
        package -> Blob,
        module -> Text,
        tx_sequence_number -> Bigint,
        event_sequence_number -> Bigint,
        sender -> Blob,
    }
}

diesel::table! {
    event_struct_name (package, module, type_name, tx_sequence_number, event_sequence_number) {
        package -> Blob,
        module -> Text,
        type_name -> Text,
        tx_sequence_number -> Bigint,
        event_sequence_number -> Bigint,
        sender -> Blob,
    }
}

diesel::table! {
    event_struct_package (package, tx_sequence_number, event_sequence_number) {
        package -> Blob,
        tx_sequence_number -> Bigint,
        event_sequence_number -> Bigint,
        sender -> Blob,
    }
}

diesel::table! {
    feature_flags (protocol_version, flag_name) {
        protocol_version -> Bigint,
        flag_name -> Text,
        flag_value -> Bool,
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
        object_type_package -> Nullable<Blob>,
        object_type_module -> Nullable<Text>,
        object_type_name -> Nullable<Text>,
        serialized_object -> Mediumblob,
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
        object_type_package -> Nullable<Blob>,
        object_type_module -> Nullable<Text>,
        object_type_name -> Nullable<Text>,
        serialized_object -> Nullable<Mediumblob>,
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
        object_type_package -> Nullable<Blob>,
        object_type_module -> Nullable<Text>,
        object_type_name -> Nullable<Text>,
        serialized_object -> Nullable<Mediumblob>,
        coin_type -> Nullable<Text>,
        coin_balance -> Nullable<Bigint>,
        df_kind -> Nullable<Smallint>,
        df_name -> Nullable<Blob>,
        df_object_type -> Nullable<Text>,
        df_object_id -> Nullable<Blob>,
    }
}

diesel::table! {
    objects_version (object_id, object_version) {
        object_id -> Blob,
        object_version -> Bigint,
        cp_sequence_number -> Bigint,
    }
}

diesel::table! {
    packages (package_id) {
        package_id -> Blob,
        original_id -> Blob,
        package_version -> Bigint,
        move_package -> Mediumblob,
        checkpoint_sequence_number -> Bigint,
    }
}

diesel::table! {
    protocol_configs (protocol_version, config_name) {
        protocol_version -> Bigint,
        config_name -> Text,
        config_value -> Nullable<Text>,
    }
}

diesel::table! {
    pruner_cp_watermark (checkpoint_sequence_number) {
        checkpoint_sequence_number -> Bigint,
        min_tx_sequence_number -> Bigint,
        max_tx_sequence_number -> Bigint,
    }
}

diesel::table! {
    transactions (tx_sequence_number, checkpoint_sequence_number) {
        tx_sequence_number -> Bigint,
        transaction_digest -> Blob,
        raw_transaction -> Mediumblob,
        raw_effects -> Mediumblob,
        checkpoint_sequence_number -> Bigint,
        timestamp_ms -> Bigint,
        object_changes -> Json,
        balance_changes -> Json,
        events -> Json,
        transaction_kind -> Smallint,
        success_command_count -> Smallint,
    }
}

diesel::table! {
    tx_calls_fun (package, module, func, tx_sequence_number) {
        tx_sequence_number -> Bigint,
        package -> Blob,
        module -> Text,
        func -> Text,
        sender -> Blob,
    }
}

diesel::table! {
    tx_calls_mod (package, module, tx_sequence_number) {
        tx_sequence_number -> Bigint,
        package -> Blob,
        module -> Text,
        sender -> Blob,
    }
}

diesel::table! {
    tx_calls_pkg (package, tx_sequence_number) {
        tx_sequence_number -> Bigint,
        package -> Blob,
        sender -> Blob,
    }
}

diesel::table! {
    tx_changed_objects (object_id, tx_sequence_number) {
        cp_sequence_number -> Bigint,
        tx_sequence_number -> Bigint,
        object_id -> Blob,
        sender -> Blob,
    }
}

diesel::table! {
    tx_digests (tx_digest) {
        tx_digest -> Blob,
        cp_sequence_number -> Bigint,
        tx_sequence_number -> Bigint,
    }
}

diesel::table! {
    tx_input_objects (object_id, tx_sequence_number, cp_sequence_number) {
        cp_sequence_number -> Bigint,
        tx_sequence_number -> Bigint,
        object_id -> Blob,
        sender -> Blob,
    }
}

diesel::table! {
    tx_kinds (tx_kind, tx_sequence_number) {
        tx_sequence_number -> Bigint,
        tx_kind -> Smallint,
    }
}

diesel::table! {
    tx_recipients (recipient, tx_sequence_number) {
        cp_sequence_number -> Bigint,
        tx_sequence_number -> Bigint,
        recipient -> Blob,
        sender -> Blob,
    }
}

diesel::table! {
    tx_senders (sender, tx_sequence_number, cp_sequence_number) {
        cp_sequence_number -> Bigint,
        tx_sequence_number -> Bigint,
        sender -> Blob,
    }
}

#[macro_export]
macro_rules! for_all_tables {
    ($action:path) => {
        $action!(
            chain_identifier,
            checkpoints,
            pruner_cp_watermark,
            display,
            epochs,
            event_emit_module,
            event_emit_package,
            event_senders,
            event_struct_instantiation,
            event_struct_module,
            event_struct_name,
            event_struct_package,
            events,
            objects,
            objects_history,
            objects_snapshot,
            objects_version,
            packages,
            transactions,
            tx_calls_fun,
            tx_calls_mod,
            tx_calls_pkg,
            tx_changed_objects,
            tx_digests,
            tx_input_objects,
            tx_kinds,
            tx_recipients,
            tx_senders
        );
    };
}
pub use for_all_tables;

for_all_tables!(diesel::allow_tables_to_appear_in_same_query);
