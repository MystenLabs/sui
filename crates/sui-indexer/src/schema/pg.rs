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
        min_tx_sequence_number -> Nullable<Int8>,
        max_tx_sequence_number -> Nullable<Int8>,
    }
}

diesel::table! {
    cp_tx (checkpoint_sequence_number) {
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
    event_emit_module (package, module, tx_sequence_number, event_sequence_number) {
        package -> Bytea,
        module -> Text,
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        sender -> Bytea,
    }
}

diesel::table! {
    event_emit_package (package, tx_sequence_number, event_sequence_number) {
        package -> Bytea,
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        sender -> Bytea,
    }
}

diesel::table! {
    event_senders (sender, tx_sequence_number, event_sequence_number) {
        sender -> Bytea,
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
    }
}

diesel::table! {
    event_struct_instantiation (package, module, type_instantiation, tx_sequence_number, event_sequence_number) {
        package -> Bytea,
        module -> Text,
        type_instantiation -> Text,
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        sender -> Bytea,
    }
}

diesel::table! {
    event_struct_module (package, module, tx_sequence_number, event_sequence_number) {
        package -> Bytea,
        module -> Text,
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        sender -> Bytea,
    }
}

diesel::table! {
    event_struct_name (package, module, type_name, tx_sequence_number, event_sequence_number) {
        package -> Bytea,
        module -> Text,
        type_name -> Text,
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        sender -> Bytea,
    }
}

diesel::table! {
    event_struct_package (package, tx_sequence_number, event_sequence_number) {
        package -> Bytea,
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        sender -> Bytea,
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
        event_type_package -> Bytea,
        event_type_module -> Text,
        event_type_name -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
    }
}

diesel::table! {
    events_partition_0 (tx_sequence_number, event_sequence_number) {
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
    objects_version (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_00 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_01 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_02 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_03 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_04 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_05 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_06 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_07 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_08 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_09 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_0a (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_0b (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_0c (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_0d (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_0e (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_0f (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_10 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_11 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_12 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_13 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_14 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_15 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_16 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_17 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_18 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_19 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_1a (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_1b (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_1c (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_1d (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_1e (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_1f (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_20 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_21 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_22 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_23 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_24 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_25 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_26 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_27 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_28 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_29 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_2a (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_2b (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_2c (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_2d (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_2e (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_2f (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_30 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_31 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_32 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_33 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_34 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_35 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_36 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_37 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_38 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_39 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_3a (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_3b (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_3c (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_3d (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_3e (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_3f (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_40 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_41 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_42 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_43 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_44 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_45 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_46 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_47 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_48 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_49 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_4a (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_4b (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_4c (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_4d (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_4e (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_4f (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_50 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_51 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_52 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_53 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_54 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_55 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_56 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_57 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_58 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_59 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_5a (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_5b (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_5c (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_5d (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_5e (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_5f (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_60 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_61 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_62 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_63 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_64 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_65 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_66 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_67 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_68 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_69 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_6a (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_6b (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_6c (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_6d (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_6e (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_6f (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_70 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_71 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_72 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_73 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_74 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_75 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_76 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_77 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_78 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_79 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_7a (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_7b (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_7c (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_7d (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_7e (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_7f (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_80 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_81 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_82 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_83 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_84 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_85 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_86 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_87 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_88 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_89 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_8a (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_8b (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_8c (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_8d (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_8e (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_8f (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_90 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_91 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_92 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_93 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_94 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_95 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_96 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_97 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_98 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_99 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_9a (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_9b (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_9c (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_9d (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_9e (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_9f (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_a0 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_a1 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_a2 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_a3 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_a4 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_a5 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_a6 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_a7 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_a8 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_a9 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_aa (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_ab (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_ac (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_ad (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_ae (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_af (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_b0 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_b1 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_b2 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_b3 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_b4 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_b5 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_b6 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_b7 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_b8 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_b9 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_ba (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_bb (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_bc (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_bd (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_be (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_bf (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_c0 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_c1 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_c2 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_c3 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_c4 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_c5 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_c6 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_c7 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_c8 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_c9 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_ca (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_cb (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_cc (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_cd (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_ce (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_cf (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_d0 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_d1 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_d2 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_d3 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_d4 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_d5 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_d6 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_d7 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_d8 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_d9 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_da (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_db (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_dc (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_dd (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_de (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_df (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_e0 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_e1 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_e2 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_e3 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_e4 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_e5 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_e6 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_e7 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_e8 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_e9 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_ea (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_eb (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_ec (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_ed (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_ee (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_ef (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_f0 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_f1 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_f2 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_f3 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_f4 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_f5 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_f6 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_f7 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_f8 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_f9 (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_fa (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_fb (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_fc (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_fd (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_fe (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    objects_version_ff (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        cp_sequence_number -> Int8,
    }
}

diesel::table! {
    packages (package_id, original_id, package_version) {
        package_id -> Bytea,
        original_id -> Bytea,
        package_version -> Int8,
        move_package -> Bytea,
        checkpoint_sequence_number -> Int8,
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
        success_command_count -> Int2,
    }
}

diesel::table! {
    transactions_partition_0 (tx_sequence_number) {
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
    tx_calls_fun (package, module, func, tx_sequence_number) {
        tx_sequence_number -> Int8,
        package -> Bytea,
        module -> Text,
        func -> Text,
        sender -> Bytea,
    }
}

diesel::table! {
    tx_calls_mod (package, module, tx_sequence_number) {
        tx_sequence_number -> Int8,
        package -> Bytea,
        module -> Text,
        sender -> Bytea,
    }
}

diesel::table! {
    tx_calls_pkg (package, tx_sequence_number) {
        tx_sequence_number -> Int8,
        package -> Bytea,
        sender -> Bytea,
    }
}

diesel::table! {
    tx_changed_objects (object_id, tx_sequence_number) {
        tx_sequence_number -> Int8,
        object_id -> Bytea,
        sender -> Bytea,
    }
}

diesel::table! {
    tx_digests (tx_digest) {
        tx_digest -> Bytea,
        tx_sequence_number -> Int8,
    }
}

diesel::table! {
    tx_input_objects (object_id, tx_sequence_number) {
        tx_sequence_number -> Int8,
        object_id -> Bytea,
        sender -> Bytea,
    }
}

diesel::table! {
    tx_kinds (tx_kind, tx_sequence_number) {
        tx_sequence_number -> Int8,
        tx_kind -> Int2,
    }
}

diesel::table! {
    tx_recipients (recipient, tx_sequence_number) {
        tx_sequence_number -> Int8,
        recipient -> Bytea,
        sender -> Bytea,
    }
}

diesel::table! {
    tx_senders (sender, tx_sequence_number) {
        tx_sequence_number -> Int8,
        sender -> Bytea,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    checkpoints,
    cp_tx,
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
    events_partition_0,
    objects,
    objects_history,
    objects_history_partition_0,
    objects_snapshot,
    objects_version,
    objects_version_00,
    objects_version_01,
    objects_version_02,
    objects_version_03,
    objects_version_04,
    objects_version_05,
    objects_version_06,
    objects_version_07,
    objects_version_08,
    objects_version_09,
    objects_version_0a,
    objects_version_0b,
    objects_version_0c,
    objects_version_0d,
    objects_version_0e,
    objects_version_0f,
    objects_version_10,
    objects_version_11,
    objects_version_12,
    objects_version_13,
    objects_version_14,
    objects_version_15,
    objects_version_16,
    objects_version_17,
    objects_version_18,
    objects_version_19,
    objects_version_1a,
    objects_version_1b,
    objects_version_1c,
    objects_version_1d,
    objects_version_1e,
    objects_version_1f,
    objects_version_20,
    objects_version_21,
    objects_version_22,
    objects_version_23,
    objects_version_24,
    objects_version_25,
    objects_version_26,
    objects_version_27,
    objects_version_28,
    objects_version_29,
    objects_version_2a,
    objects_version_2b,
    objects_version_2c,
    objects_version_2d,
    objects_version_2e,
    objects_version_2f,
    objects_version_30,
    objects_version_31,
    objects_version_32,
    objects_version_33,
    objects_version_34,
    objects_version_35,
    objects_version_36,
    objects_version_37,
    objects_version_38,
    objects_version_39,
    objects_version_3a,
    objects_version_3b,
    objects_version_3c,
    objects_version_3d,
    objects_version_3e,
    objects_version_3f,
    objects_version_40,
    objects_version_41,
    objects_version_42,
    objects_version_43,
    objects_version_44,
    objects_version_45,
    objects_version_46,
    objects_version_47,
    objects_version_48,
    objects_version_49,
    objects_version_4a,
    objects_version_4b,
    objects_version_4c,
    objects_version_4d,
    objects_version_4e,
    objects_version_4f,
    objects_version_50,
    objects_version_51,
    objects_version_52,
    objects_version_53,
    objects_version_54,
    objects_version_55,
    objects_version_56,
    objects_version_57,
    objects_version_58,
    objects_version_59,
    objects_version_5a,
    objects_version_5b,
    objects_version_5c,
    objects_version_5d,
    objects_version_5e,
    objects_version_5f,
    objects_version_60,
    objects_version_61,
    objects_version_62,
    objects_version_63,
    objects_version_64,
    objects_version_65,
    objects_version_66,
    objects_version_67,
    objects_version_68,
    objects_version_69,
    objects_version_6a,
    objects_version_6b,
    objects_version_6c,
    objects_version_6d,
    objects_version_6e,
    objects_version_6f,
    objects_version_70,
    objects_version_71,
    objects_version_72,
    objects_version_73,
    objects_version_74,
    objects_version_75,
    objects_version_76,
    objects_version_77,
    objects_version_78,
    objects_version_79,
    objects_version_7a,
    objects_version_7b,
    objects_version_7c,
    objects_version_7d,
    objects_version_7e,
    objects_version_7f,
    objects_version_80,
    objects_version_81,
    objects_version_82,
    objects_version_83,
    objects_version_84,
    objects_version_85,
    objects_version_86,
    objects_version_87,
    objects_version_88,
    objects_version_89,
    objects_version_8a,
    objects_version_8b,
    objects_version_8c,
    objects_version_8d,
    objects_version_8e,
    objects_version_8f,
    objects_version_90,
    objects_version_91,
    objects_version_92,
    objects_version_93,
    objects_version_94,
    objects_version_95,
    objects_version_96,
    objects_version_97,
    objects_version_98,
    objects_version_99,
    objects_version_9a,
    objects_version_9b,
    objects_version_9c,
    objects_version_9d,
    objects_version_9e,
    objects_version_9f,
    objects_version_a0,
    objects_version_a1,
    objects_version_a2,
    objects_version_a3,
    objects_version_a4,
    objects_version_a5,
    objects_version_a6,
    objects_version_a7,
    objects_version_a8,
    objects_version_a9,
    objects_version_aa,
    objects_version_ab,
    objects_version_ac,
    objects_version_ad,
    objects_version_ae,
    objects_version_af,
    objects_version_b0,
    objects_version_b1,
    objects_version_b2,
    objects_version_b3,
    objects_version_b4,
    objects_version_b5,
    objects_version_b6,
    objects_version_b7,
    objects_version_b8,
    objects_version_b9,
    objects_version_ba,
    objects_version_bb,
    objects_version_bc,
    objects_version_bd,
    objects_version_be,
    objects_version_bf,
    objects_version_c0,
    objects_version_c1,
    objects_version_c2,
    objects_version_c3,
    objects_version_c4,
    objects_version_c5,
    objects_version_c6,
    objects_version_c7,
    objects_version_c8,
    objects_version_c9,
    objects_version_ca,
    objects_version_cb,
    objects_version_cc,
    objects_version_cd,
    objects_version_ce,
    objects_version_cf,
    objects_version_d0,
    objects_version_d1,
    objects_version_d2,
    objects_version_d3,
    objects_version_d4,
    objects_version_d5,
    objects_version_d6,
    objects_version_d7,
    objects_version_d8,
    objects_version_d9,
    objects_version_da,
    objects_version_db,
    objects_version_dc,
    objects_version_dd,
    objects_version_de,
    objects_version_df,
    objects_version_e0,
    objects_version_e1,
    objects_version_e2,
    objects_version_e3,
    objects_version_e4,
    objects_version_e5,
    objects_version_e6,
    objects_version_e7,
    objects_version_e8,
    objects_version_e9,
    objects_version_ea,
    objects_version_eb,
    objects_version_ec,
    objects_version_ed,
    objects_version_ee,
    objects_version_ef,
    objects_version_f0,
    objects_version_f1,
    objects_version_f2,
    objects_version_f3,
    objects_version_f4,
    objects_version_f5,
    objects_version_f6,
    objects_version_f7,
    objects_version_f8,
    objects_version_f9,
    objects_version_fa,
    objects_version_fb,
    objects_version_fc,
    objects_version_fd,
    objects_version_fe,
    objects_version_ff,
    packages,
    transactions,
    transactions_partition_0,
    tx_calls_fun,
    tx_calls_mod,
    tx_calls_pkg,
    tx_changed_objects,
    tx_digests,
    tx_input_objects,
    tx_kinds,
    tx_recipients,
    tx_senders,
);
