// @generated automatically by Diesel CLI.

diesel::table! {
    chain_identifier (checkpoint_digest) {
        checkpoint_digest -> Bytea,
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
        min_tx_sequence_number -> Nullable<Int8>,
        max_tx_sequence_number -> Nullable<Int8>,
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
        system_state -> Nullable<Bytea>,
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
        system_state_summary_json -> Nullable<Jsonb>,
        first_tx_sequence_number -> Nullable<Int8>,
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
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_0 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_1 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_10 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_100 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_101 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_102 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_103 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_104 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_105 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_106 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_107 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_108 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_109 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_11 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_110 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_111 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_112 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_113 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_114 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_115 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_116 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_117 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_118 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_119 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_12 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_120 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_121 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_122 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_123 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_124 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_125 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_126 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_127 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_128 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_129 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_13 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_130 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_131 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_132 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_133 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_134 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_135 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_136 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_137 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_138 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_139 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_14 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_140 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_141 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_142 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_143 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_144 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_145 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_146 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_147 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_148 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_149 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_15 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_150 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_151 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_152 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_153 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_154 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_155 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_156 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_157 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_158 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_159 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_16 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_160 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_161 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_162 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_163 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_164 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_165 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_166 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_167 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_168 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_169 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_17 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_170 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_171 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_172 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_173 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_174 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_175 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_176 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_177 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_178 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_179 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_18 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_180 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_181 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_182 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_183 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_184 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_185 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_186 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_187 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_188 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_189 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_19 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_190 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_191 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_192 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_193 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_194 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_195 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_196 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_197 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_198 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_199 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_2 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_20 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_200 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_201 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_202 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_203 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_204 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_205 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_206 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_207 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_208 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_209 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_21 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_210 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_211 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_212 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_213 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_214 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_215 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_216 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_217 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_218 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_219 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_22 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_220 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_221 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_222 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_223 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_224 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_225 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_226 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_227 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_228 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_229 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_23 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_230 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_231 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_232 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_233 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_234 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_235 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_236 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_237 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_238 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_239 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_24 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_240 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_241 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_242 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_243 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_244 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_245 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_246 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_247 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_248 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_249 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_25 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_250 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_251 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_252 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_253 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_254 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_255 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_256 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_257 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_258 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_259 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_26 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_260 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_261 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_262 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_263 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_264 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_265 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_266 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_267 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_268 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_269 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_27 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_270 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_271 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_272 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_273 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_274 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_275 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_276 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_277 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_278 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_279 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_28 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_280 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_281 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_282 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_283 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_284 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_285 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_286 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_287 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_288 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_289 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_29 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_290 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_291 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_292 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_293 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_294 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_295 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_296 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_297 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_298 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_299 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_3 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_30 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_300 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_301 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_302 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_303 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_304 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_305 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_306 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_307 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_308 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_309 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_31 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_310 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_311 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_312 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_313 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_314 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_315 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_316 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_317 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_318 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_319 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_32 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_320 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_321 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_322 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_323 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_324 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_325 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_326 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_327 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_328 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_329 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_33 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_330 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_331 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_332 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_333 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_334 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_335 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_336 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_337 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_338 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_339 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_34 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_340 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_341 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_342 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_343 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_344 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_345 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_346 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_347 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_348 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_349 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_35 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_350 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_351 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_352 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_353 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_354 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_355 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_356 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_357 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_358 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_359 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_36 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_360 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_361 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_362 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_363 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_364 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_365 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_366 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_367 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_368 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_369 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_37 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_370 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_371 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_372 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_373 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_374 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_375 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_376 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_377 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_378 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_379 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_38 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_380 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_381 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_382 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_383 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_384 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_385 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_386 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_387 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_388 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_389 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_39 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_390 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_391 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_392 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_393 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_394 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_395 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_396 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_397 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_398 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_399 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_4 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_40 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_400 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_401 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_402 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_403 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_404 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_405 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_406 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_407 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_408 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_409 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_41 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_410 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_411 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_412 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_413 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_414 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_415 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_416 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_417 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_418 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_419 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_42 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_420 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_421 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_422 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_423 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_424 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_425 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_426 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_427 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_428 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_429 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_43 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_430 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_431 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_432 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_433 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_434 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_435 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_436 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_437 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_438 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_439 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_44 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_440 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_441 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_442 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_443 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_444 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_445 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_446 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_447 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_448 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_449 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_45 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_450 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_451 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_452 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_453 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_454 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_455 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_456 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_457 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_458 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_459 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_46 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_460 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_461 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_462 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_463 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_464 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_465 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_466 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_467 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_468 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_469 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_47 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_470 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_471 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_472 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_473 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_474 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_475 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_476 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_477 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_478 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_479 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_48 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_480 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_481 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_482 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_483 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_484 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_485 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_486 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_487 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_488 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_489 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_49 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_490 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_491 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_492 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_493 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_494 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_495 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_496 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_497 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_498 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_499 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_5 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_50 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_500 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_501 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_502 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_503 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_504 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_505 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_506 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_507 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_508 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_509 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_51 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_510 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_511 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_512 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_513 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_514 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_515 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_516 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_517 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_518 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_519 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_52 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_520 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_521 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_522 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_523 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_524 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_525 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_526 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_527 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_528 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_529 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_53 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_530 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_531 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_532 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_533 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_534 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_535 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_536 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_537 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_538 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_539 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_54 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_540 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_541 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_542 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_543 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_544 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_545 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_546 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_547 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_548 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_549 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_55 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_550 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_551 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_552 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_553 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_554 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_555 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_556 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_557 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_56 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_57 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_58 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_59 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_6 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_60 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_61 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_62 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_63 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_64 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_65 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_66 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_67 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_68 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_69 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_7 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_70 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_71 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_72 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_73 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_74 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_75 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_76 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_77 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_78 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_79 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_8 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_80 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_81 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_82 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_83 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_84 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_85 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_86 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_87 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_88 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_89 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_9 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_90 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_91 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_92 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_93 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_94 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_95 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_96 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_97 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_98 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    events_partition_99 (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Int8,
        event_sequence_number -> Int8,
        transaction_digest -> Bytea,
        senders -> Array<Nullable<Bytea>>,
        package -> Bytea,
        module -> Text,
        event_type -> Text,
        timestamp_ms -> Int8,
        bcs -> Bytea,
        sender -> Nullable<Bytea>,
    }
}

diesel::table! {
    feature_flags (protocol_version, flag_name) {
        protocol_version -> Int8,
        flag_name -> Text,
        flag_value -> Bool,
    }
}

diesel::table! {
    full_objects_history (object_id, object_version) {
        object_id -> Bytea,
        object_version -> Int8,
        serialized_object -> Nullable<Bytea>,
    }
}

diesel::table! {
    objects (object_id) {
        object_id -> Bytea,
        object_version -> Int8,
        object_digest -> Bytea,
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
    }
}

diesel::table! {
    objects_history_partition_551 (checkpoint_sequence_number, object_id, object_version) {
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
    }
}

diesel::table! {
    objects_history_partition_552 (checkpoint_sequence_number, object_id, object_version) {
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
    }
}

diesel::table! {
    objects_history_partition_553 (checkpoint_sequence_number, object_id, object_version) {
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
    }
}

diesel::table! {
    objects_history_partition_554 (checkpoint_sequence_number, object_id, object_version) {
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
    }
}

diesel::table! {
    objects_history_partition_555 (checkpoint_sequence_number, object_id, object_version) {
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
    }
}

diesel::table! {
    objects_history_partition_556 (checkpoint_sequence_number, object_id, object_version) {
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
    }
}

diesel::table! {
    objects_history_partition_557 (checkpoint_sequence_number, object_id, object_version) {
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
    protocol_configs (protocol_version, config_name) {
        protocol_version -> Int8,
        config_name -> Text,
        config_value -> Nullable<Text>,
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
    raw_checkpoints (sequence_number) {
        sequence_number -> Int8,
        certified_checkpoint -> Bytea,
        checkpoint_contents -> Bytea,
    }
}

diesel::table! {
    test_deletion_table (tx_sequence_number) {
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
    transactions_partition_1 (tx_sequence_number) {
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
    transactions_partition_10 (tx_sequence_number) {
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
    transactions_partition_100 (tx_sequence_number) {
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
    transactions_partition_101 (tx_sequence_number) {
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
    transactions_partition_102 (tx_sequence_number) {
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
    transactions_partition_103 (tx_sequence_number) {
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
    transactions_partition_104 (tx_sequence_number) {
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
    transactions_partition_105 (tx_sequence_number) {
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
    transactions_partition_106 (tx_sequence_number) {
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
    transactions_partition_107 (tx_sequence_number) {
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
    transactions_partition_108 (tx_sequence_number) {
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
    transactions_partition_109 (tx_sequence_number) {
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
    transactions_partition_11 (tx_sequence_number) {
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
    transactions_partition_110 (tx_sequence_number) {
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
    transactions_partition_111 (tx_sequence_number) {
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
    transactions_partition_112 (tx_sequence_number) {
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
    transactions_partition_113 (tx_sequence_number) {
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
    transactions_partition_114 (tx_sequence_number) {
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
    transactions_partition_115 (tx_sequence_number) {
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
    transactions_partition_116 (tx_sequence_number) {
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
    transactions_partition_117 (tx_sequence_number) {
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
    transactions_partition_118 (tx_sequence_number) {
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
    transactions_partition_119 (tx_sequence_number) {
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
    transactions_partition_12 (tx_sequence_number) {
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
    transactions_partition_120 (tx_sequence_number) {
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
    transactions_partition_121 (tx_sequence_number) {
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
    transactions_partition_122 (tx_sequence_number) {
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
    transactions_partition_123 (tx_sequence_number) {
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
    transactions_partition_124 (tx_sequence_number) {
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
    transactions_partition_125 (tx_sequence_number) {
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
    transactions_partition_126 (tx_sequence_number) {
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
    transactions_partition_127 (tx_sequence_number) {
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
    transactions_partition_128 (tx_sequence_number) {
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
    transactions_partition_129 (tx_sequence_number) {
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
    transactions_partition_13 (tx_sequence_number) {
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
    transactions_partition_130 (tx_sequence_number) {
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
    transactions_partition_131 (tx_sequence_number) {
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
    transactions_partition_132 (tx_sequence_number) {
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
    transactions_partition_133 (tx_sequence_number) {
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
    transactions_partition_134 (tx_sequence_number) {
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
    transactions_partition_135 (tx_sequence_number) {
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
    transactions_partition_136 (tx_sequence_number) {
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
    transactions_partition_137 (tx_sequence_number) {
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
    transactions_partition_138 (tx_sequence_number) {
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
    transactions_partition_139 (tx_sequence_number) {
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
    transactions_partition_14 (tx_sequence_number) {
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
    transactions_partition_140 (tx_sequence_number) {
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
    transactions_partition_141 (tx_sequence_number) {
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
    transactions_partition_142 (tx_sequence_number) {
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
    transactions_partition_143 (tx_sequence_number) {
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
    transactions_partition_144 (tx_sequence_number) {
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
    transactions_partition_145 (tx_sequence_number) {
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
    transactions_partition_146 (tx_sequence_number) {
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
    transactions_partition_147 (tx_sequence_number) {
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
    transactions_partition_148 (tx_sequence_number) {
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
    transactions_partition_149 (tx_sequence_number) {
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
    transactions_partition_15 (tx_sequence_number) {
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
    transactions_partition_150 (tx_sequence_number) {
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
    transactions_partition_151 (tx_sequence_number) {
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
    transactions_partition_152 (tx_sequence_number) {
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
    transactions_partition_153 (tx_sequence_number) {
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
    transactions_partition_154 (tx_sequence_number) {
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
    transactions_partition_155 (tx_sequence_number) {
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
    transactions_partition_156 (tx_sequence_number) {
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
    transactions_partition_157 (tx_sequence_number) {
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
    transactions_partition_158 (tx_sequence_number) {
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
    transactions_partition_159 (tx_sequence_number) {
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
    transactions_partition_16 (tx_sequence_number) {
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
    transactions_partition_160 (tx_sequence_number) {
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
    transactions_partition_161 (tx_sequence_number) {
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
    transactions_partition_162 (tx_sequence_number) {
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
    transactions_partition_163 (tx_sequence_number) {
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
    transactions_partition_164 (tx_sequence_number) {
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
    transactions_partition_165 (tx_sequence_number) {
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
    transactions_partition_166 (tx_sequence_number) {
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
    transactions_partition_167 (tx_sequence_number) {
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
    transactions_partition_168 (tx_sequence_number) {
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
    transactions_partition_169 (tx_sequence_number) {
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
    transactions_partition_17 (tx_sequence_number) {
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
    transactions_partition_170 (tx_sequence_number) {
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
    transactions_partition_171 (tx_sequence_number) {
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
    transactions_partition_172 (tx_sequence_number) {
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
    transactions_partition_173 (tx_sequence_number) {
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
    transactions_partition_174 (tx_sequence_number) {
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
    transactions_partition_175 (tx_sequence_number) {
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
    transactions_partition_176 (tx_sequence_number) {
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
    transactions_partition_177 (tx_sequence_number) {
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
    transactions_partition_178 (tx_sequence_number) {
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
    transactions_partition_179 (tx_sequence_number) {
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
    transactions_partition_18 (tx_sequence_number) {
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
    transactions_partition_180 (tx_sequence_number) {
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
    transactions_partition_181 (tx_sequence_number) {
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
    transactions_partition_182 (tx_sequence_number) {
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
    transactions_partition_183 (tx_sequence_number) {
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
    transactions_partition_184 (tx_sequence_number) {
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
    transactions_partition_185 (tx_sequence_number) {
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
    transactions_partition_186 (tx_sequence_number) {
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
    transactions_partition_187 (tx_sequence_number) {
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
    transactions_partition_188 (tx_sequence_number) {
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
    transactions_partition_189 (tx_sequence_number) {
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
    transactions_partition_19 (tx_sequence_number) {
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
    transactions_partition_190 (tx_sequence_number) {
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
    transactions_partition_191 (tx_sequence_number) {
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
    transactions_partition_192 (tx_sequence_number) {
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
    transactions_partition_193 (tx_sequence_number) {
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
    transactions_partition_194 (tx_sequence_number) {
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
    transactions_partition_195 (tx_sequence_number) {
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
    transactions_partition_196 (tx_sequence_number) {
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
    transactions_partition_197 (tx_sequence_number) {
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
    transactions_partition_198 (tx_sequence_number) {
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
    transactions_partition_199 (tx_sequence_number) {
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
    transactions_partition_2 (tx_sequence_number) {
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
    transactions_partition_20 (tx_sequence_number) {
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
    transactions_partition_200 (tx_sequence_number) {
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
    transactions_partition_201 (tx_sequence_number) {
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
    transactions_partition_202 (tx_sequence_number) {
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
    transactions_partition_203 (tx_sequence_number) {
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
    transactions_partition_204 (tx_sequence_number) {
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
    transactions_partition_205 (tx_sequence_number) {
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
    transactions_partition_206 (tx_sequence_number) {
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
    transactions_partition_207 (tx_sequence_number) {
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
    transactions_partition_208 (tx_sequence_number) {
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
    transactions_partition_209 (tx_sequence_number) {
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
    transactions_partition_21 (tx_sequence_number) {
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
    transactions_partition_210 (tx_sequence_number) {
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
    transactions_partition_211 (tx_sequence_number) {
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
    transactions_partition_212 (tx_sequence_number) {
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
    transactions_partition_213 (tx_sequence_number) {
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
    transactions_partition_214 (tx_sequence_number) {
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
    transactions_partition_215 (tx_sequence_number) {
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
    transactions_partition_216 (tx_sequence_number) {
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
    transactions_partition_217 (tx_sequence_number) {
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
    transactions_partition_218 (tx_sequence_number) {
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
    transactions_partition_219 (tx_sequence_number) {
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
    transactions_partition_22 (tx_sequence_number) {
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
    transactions_partition_220 (tx_sequence_number) {
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
    transactions_partition_221 (tx_sequence_number) {
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
    transactions_partition_222 (tx_sequence_number) {
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
    transactions_partition_223 (tx_sequence_number) {
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
    transactions_partition_224 (tx_sequence_number) {
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
    transactions_partition_225 (tx_sequence_number) {
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
    transactions_partition_226 (tx_sequence_number) {
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
    transactions_partition_227 (tx_sequence_number) {
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
    transactions_partition_228 (tx_sequence_number) {
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
    transactions_partition_229 (tx_sequence_number) {
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
    transactions_partition_23 (tx_sequence_number) {
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
    transactions_partition_230 (tx_sequence_number) {
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
    transactions_partition_231 (tx_sequence_number) {
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
    transactions_partition_232 (tx_sequence_number) {
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
    transactions_partition_233 (tx_sequence_number) {
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
    transactions_partition_234 (tx_sequence_number) {
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
    transactions_partition_235 (tx_sequence_number) {
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
    transactions_partition_236 (tx_sequence_number) {
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
    transactions_partition_237 (tx_sequence_number) {
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
    transactions_partition_238 (tx_sequence_number) {
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
    transactions_partition_239 (tx_sequence_number) {
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
    transactions_partition_24 (tx_sequence_number) {
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
    transactions_partition_240 (tx_sequence_number) {
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
    transactions_partition_241 (tx_sequence_number) {
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
    transactions_partition_242 (tx_sequence_number) {
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
    transactions_partition_243 (tx_sequence_number) {
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
    transactions_partition_244 (tx_sequence_number) {
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
    transactions_partition_245 (tx_sequence_number) {
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
    transactions_partition_246 (tx_sequence_number) {
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
    transactions_partition_247 (tx_sequence_number) {
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
    transactions_partition_248 (tx_sequence_number) {
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
    transactions_partition_249 (tx_sequence_number) {
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
    transactions_partition_25 (tx_sequence_number) {
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
    transactions_partition_250 (tx_sequence_number) {
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
    transactions_partition_251 (tx_sequence_number) {
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
    transactions_partition_252 (tx_sequence_number) {
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
    transactions_partition_253 (tx_sequence_number) {
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
    transactions_partition_254 (tx_sequence_number) {
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
    transactions_partition_255 (tx_sequence_number) {
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
    transactions_partition_256 (tx_sequence_number) {
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
    transactions_partition_257 (tx_sequence_number) {
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
    transactions_partition_258 (tx_sequence_number) {
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
    transactions_partition_259 (tx_sequence_number) {
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
    transactions_partition_26 (tx_sequence_number) {
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
    transactions_partition_260 (tx_sequence_number) {
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
    transactions_partition_261 (tx_sequence_number) {
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
    transactions_partition_262 (tx_sequence_number) {
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
    transactions_partition_263 (tx_sequence_number) {
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
    transactions_partition_264 (tx_sequence_number) {
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
    transactions_partition_265 (tx_sequence_number) {
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
    transactions_partition_266 (tx_sequence_number) {
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
    transactions_partition_267 (tx_sequence_number) {
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
    transactions_partition_268 (tx_sequence_number) {
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
    transactions_partition_269 (tx_sequence_number) {
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
    transactions_partition_27 (tx_sequence_number) {
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
    transactions_partition_270 (tx_sequence_number) {
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
    transactions_partition_271 (tx_sequence_number) {
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
    transactions_partition_272 (tx_sequence_number) {
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
    transactions_partition_273 (tx_sequence_number) {
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
    transactions_partition_274 (tx_sequence_number) {
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
    transactions_partition_275 (tx_sequence_number) {
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
    transactions_partition_276 (tx_sequence_number) {
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
    transactions_partition_277 (tx_sequence_number) {
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
    transactions_partition_278 (tx_sequence_number) {
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
    transactions_partition_279 (tx_sequence_number) {
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
    transactions_partition_28 (tx_sequence_number) {
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
    transactions_partition_280 (tx_sequence_number) {
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
    transactions_partition_281 (tx_sequence_number) {
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
    transactions_partition_282 (tx_sequence_number) {
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
    transactions_partition_283 (tx_sequence_number) {
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
    transactions_partition_284 (tx_sequence_number) {
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
    transactions_partition_285 (tx_sequence_number) {
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
    transactions_partition_286 (tx_sequence_number) {
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
    transactions_partition_287 (tx_sequence_number) {
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
    transactions_partition_288 (tx_sequence_number) {
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
    transactions_partition_289 (tx_sequence_number) {
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
    transactions_partition_29 (tx_sequence_number) {
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
    transactions_partition_290 (tx_sequence_number) {
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
    transactions_partition_291 (tx_sequence_number) {
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
    transactions_partition_292 (tx_sequence_number) {
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
    transactions_partition_293 (tx_sequence_number) {
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
    transactions_partition_294 (tx_sequence_number) {
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
    transactions_partition_295 (tx_sequence_number) {
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
    transactions_partition_296 (tx_sequence_number) {
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
    transactions_partition_297 (tx_sequence_number) {
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
    transactions_partition_298 (tx_sequence_number) {
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
    transactions_partition_299 (tx_sequence_number) {
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
    transactions_partition_3 (tx_sequence_number) {
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
    transactions_partition_30 (tx_sequence_number) {
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
    transactions_partition_300 (tx_sequence_number) {
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
    transactions_partition_301 (tx_sequence_number) {
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
    transactions_partition_302 (tx_sequence_number) {
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
    transactions_partition_303 (tx_sequence_number) {
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
    transactions_partition_304 (tx_sequence_number) {
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
    transactions_partition_305 (tx_sequence_number) {
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
    transactions_partition_306 (tx_sequence_number) {
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
    transactions_partition_307 (tx_sequence_number) {
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
    transactions_partition_308 (tx_sequence_number) {
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
    transactions_partition_309 (tx_sequence_number) {
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
    transactions_partition_31 (tx_sequence_number) {
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
    transactions_partition_310 (tx_sequence_number) {
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
    transactions_partition_311 (tx_sequence_number) {
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
    transactions_partition_312 (tx_sequence_number) {
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
    transactions_partition_313 (tx_sequence_number) {
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
    transactions_partition_314 (tx_sequence_number) {
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
    transactions_partition_315 (tx_sequence_number) {
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
    transactions_partition_316 (tx_sequence_number) {
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
    transactions_partition_317 (tx_sequence_number) {
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
    transactions_partition_318 (tx_sequence_number) {
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
    transactions_partition_319 (tx_sequence_number) {
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
    transactions_partition_32 (tx_sequence_number) {
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
    transactions_partition_320 (tx_sequence_number) {
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
    transactions_partition_321 (tx_sequence_number) {
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
    transactions_partition_322 (tx_sequence_number) {
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
    transactions_partition_323 (tx_sequence_number) {
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
    transactions_partition_324 (tx_sequence_number) {
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
    transactions_partition_325 (tx_sequence_number) {
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
    transactions_partition_326 (tx_sequence_number) {
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
    transactions_partition_327 (tx_sequence_number) {
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
    transactions_partition_328 (tx_sequence_number) {
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
    transactions_partition_329 (tx_sequence_number) {
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
    transactions_partition_33 (tx_sequence_number) {
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
    transactions_partition_330 (tx_sequence_number) {
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
    transactions_partition_331 (tx_sequence_number) {
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
    transactions_partition_332 (tx_sequence_number) {
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
    transactions_partition_333 (tx_sequence_number) {
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
    transactions_partition_334 (tx_sequence_number) {
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
    transactions_partition_335 (tx_sequence_number) {
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
    transactions_partition_336 (tx_sequence_number) {
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
    transactions_partition_337 (tx_sequence_number) {
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
    transactions_partition_338 (tx_sequence_number) {
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
    transactions_partition_339 (tx_sequence_number) {
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
    transactions_partition_34 (tx_sequence_number) {
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
    transactions_partition_340 (tx_sequence_number) {
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
    transactions_partition_341 (tx_sequence_number) {
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
    transactions_partition_342 (tx_sequence_number) {
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
    transactions_partition_343 (tx_sequence_number) {
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
    transactions_partition_344 (tx_sequence_number) {
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
    transactions_partition_345 (tx_sequence_number) {
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
    transactions_partition_346 (tx_sequence_number) {
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
    transactions_partition_347 (tx_sequence_number) {
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
    transactions_partition_348 (tx_sequence_number) {
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
    transactions_partition_349 (tx_sequence_number) {
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
    transactions_partition_35 (tx_sequence_number) {
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
    transactions_partition_350 (tx_sequence_number) {
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
    transactions_partition_351 (tx_sequence_number) {
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
    transactions_partition_352 (tx_sequence_number) {
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
    transactions_partition_353 (tx_sequence_number) {
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
    transactions_partition_354 (tx_sequence_number) {
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
    transactions_partition_355 (tx_sequence_number) {
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
    transactions_partition_356 (tx_sequence_number) {
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
    transactions_partition_357 (tx_sequence_number) {
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
    transactions_partition_358 (tx_sequence_number) {
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
    transactions_partition_359 (tx_sequence_number) {
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
    transactions_partition_36 (tx_sequence_number) {
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
    transactions_partition_360 (tx_sequence_number) {
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
    transactions_partition_361 (tx_sequence_number) {
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
    transactions_partition_362 (tx_sequence_number) {
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
    transactions_partition_363 (tx_sequence_number) {
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
    transactions_partition_364 (tx_sequence_number) {
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
    transactions_partition_365 (tx_sequence_number) {
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
    transactions_partition_366 (tx_sequence_number) {
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
    transactions_partition_367 (tx_sequence_number) {
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
    transactions_partition_368 (tx_sequence_number) {
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
    transactions_partition_369 (tx_sequence_number) {
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
    transactions_partition_37 (tx_sequence_number) {
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
    transactions_partition_370 (tx_sequence_number) {
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
    transactions_partition_371 (tx_sequence_number) {
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
    transactions_partition_372 (tx_sequence_number) {
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
    transactions_partition_373 (tx_sequence_number) {
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
    transactions_partition_374 (tx_sequence_number) {
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
    transactions_partition_375 (tx_sequence_number) {
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
    transactions_partition_376 (tx_sequence_number) {
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
    transactions_partition_377 (tx_sequence_number) {
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
    transactions_partition_378 (tx_sequence_number) {
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
    transactions_partition_379 (tx_sequence_number) {
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
    transactions_partition_38 (tx_sequence_number) {
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
    transactions_partition_380 (tx_sequence_number) {
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
    transactions_partition_381 (tx_sequence_number) {
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
    transactions_partition_382 (tx_sequence_number) {
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
    transactions_partition_383 (tx_sequence_number) {
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
    transactions_partition_384 (tx_sequence_number) {
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
    transactions_partition_385 (tx_sequence_number) {
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
    transactions_partition_386 (tx_sequence_number) {
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
    transactions_partition_387 (tx_sequence_number) {
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
    transactions_partition_388 (tx_sequence_number) {
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
    transactions_partition_389 (tx_sequence_number) {
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
    transactions_partition_39 (tx_sequence_number) {
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
    transactions_partition_390 (tx_sequence_number) {
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
    transactions_partition_391 (tx_sequence_number) {
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
    transactions_partition_392 (tx_sequence_number) {
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
    transactions_partition_393 (tx_sequence_number) {
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
    transactions_partition_394 (tx_sequence_number) {
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
    transactions_partition_395 (tx_sequence_number) {
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
    transactions_partition_396 (tx_sequence_number) {
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
    transactions_partition_397 (tx_sequence_number) {
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
    transactions_partition_398 (tx_sequence_number) {
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
    transactions_partition_399 (tx_sequence_number) {
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
    transactions_partition_4 (tx_sequence_number) {
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
    transactions_partition_40 (tx_sequence_number) {
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
    transactions_partition_400 (tx_sequence_number) {
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
    transactions_partition_401 (tx_sequence_number) {
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
    transactions_partition_402 (tx_sequence_number) {
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
    transactions_partition_403 (tx_sequence_number) {
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
    transactions_partition_404 (tx_sequence_number) {
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
    transactions_partition_405 (tx_sequence_number) {
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
    transactions_partition_406 (tx_sequence_number) {
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
    transactions_partition_407 (tx_sequence_number) {
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
    transactions_partition_408 (tx_sequence_number) {
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
    transactions_partition_409 (tx_sequence_number) {
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
    transactions_partition_41 (tx_sequence_number) {
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
    transactions_partition_410 (tx_sequence_number) {
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
    transactions_partition_411 (tx_sequence_number) {
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
    transactions_partition_412 (tx_sequence_number) {
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
    transactions_partition_413 (tx_sequence_number) {
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
    transactions_partition_414 (tx_sequence_number) {
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
    transactions_partition_415 (tx_sequence_number) {
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
    transactions_partition_416 (tx_sequence_number) {
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
    transactions_partition_417 (tx_sequence_number) {
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
    transactions_partition_418 (tx_sequence_number) {
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
    transactions_partition_419 (tx_sequence_number) {
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
    transactions_partition_42 (tx_sequence_number) {
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
    transactions_partition_420 (tx_sequence_number) {
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
    transactions_partition_421 (tx_sequence_number) {
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
    transactions_partition_422 (tx_sequence_number) {
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
    transactions_partition_423 (tx_sequence_number) {
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
    transactions_partition_424 (tx_sequence_number) {
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
    transactions_partition_425 (tx_sequence_number) {
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
    transactions_partition_426 (tx_sequence_number) {
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
    transactions_partition_427 (tx_sequence_number) {
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
    transactions_partition_428 (tx_sequence_number) {
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
    transactions_partition_429 (tx_sequence_number) {
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
    transactions_partition_43 (tx_sequence_number) {
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
    transactions_partition_430 (tx_sequence_number) {
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
    transactions_partition_431 (tx_sequence_number) {
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
    transactions_partition_432 (tx_sequence_number) {
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
    transactions_partition_433 (tx_sequence_number) {
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
    transactions_partition_434 (tx_sequence_number) {
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
    transactions_partition_435 (tx_sequence_number) {
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
    transactions_partition_436 (tx_sequence_number) {
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
    transactions_partition_437 (tx_sequence_number) {
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
    transactions_partition_438 (tx_sequence_number) {
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
    transactions_partition_439 (tx_sequence_number) {
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
    transactions_partition_44 (tx_sequence_number) {
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
    transactions_partition_440 (tx_sequence_number) {
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
    transactions_partition_441 (tx_sequence_number) {
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
    transactions_partition_442 (tx_sequence_number) {
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
    transactions_partition_443 (tx_sequence_number) {
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
    transactions_partition_444 (tx_sequence_number) {
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
    transactions_partition_445 (tx_sequence_number) {
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
    transactions_partition_446 (tx_sequence_number) {
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
    transactions_partition_447 (tx_sequence_number) {
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
    transactions_partition_448 (tx_sequence_number) {
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
    transactions_partition_449 (tx_sequence_number) {
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
    transactions_partition_45 (tx_sequence_number) {
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
    transactions_partition_450 (tx_sequence_number) {
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
    transactions_partition_451 (tx_sequence_number) {
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
    transactions_partition_452 (tx_sequence_number) {
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
    transactions_partition_453 (tx_sequence_number) {
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
    transactions_partition_454 (tx_sequence_number) {
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
    transactions_partition_455 (tx_sequence_number) {
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
    transactions_partition_456 (tx_sequence_number) {
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
    transactions_partition_457 (tx_sequence_number) {
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
    transactions_partition_458 (tx_sequence_number) {
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
    transactions_partition_459 (tx_sequence_number) {
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
    transactions_partition_46 (tx_sequence_number) {
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
    transactions_partition_460 (tx_sequence_number) {
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
    transactions_partition_461 (tx_sequence_number) {
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
    transactions_partition_462 (tx_sequence_number) {
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
    transactions_partition_463 (tx_sequence_number) {
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
    transactions_partition_464 (tx_sequence_number) {
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
    transactions_partition_465 (tx_sequence_number) {
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
    transactions_partition_466 (tx_sequence_number) {
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
    transactions_partition_467 (tx_sequence_number) {
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
    transactions_partition_468 (tx_sequence_number) {
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
    transactions_partition_469 (tx_sequence_number) {
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
    transactions_partition_47 (tx_sequence_number) {
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
    transactions_partition_470 (tx_sequence_number) {
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
    transactions_partition_471 (tx_sequence_number) {
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
    transactions_partition_472 (tx_sequence_number) {
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
    transactions_partition_473 (tx_sequence_number) {
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
    transactions_partition_474 (tx_sequence_number) {
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
    transactions_partition_475 (tx_sequence_number) {
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
    transactions_partition_476 (tx_sequence_number) {
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
    transactions_partition_477 (tx_sequence_number) {
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
    transactions_partition_478 (tx_sequence_number) {
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
    transactions_partition_479 (tx_sequence_number) {
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
    transactions_partition_48 (tx_sequence_number) {
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
    transactions_partition_480 (tx_sequence_number) {
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
    transactions_partition_481 (tx_sequence_number) {
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
    transactions_partition_482 (tx_sequence_number) {
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
    transactions_partition_483 (tx_sequence_number) {
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
    transactions_partition_484 (tx_sequence_number) {
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
    transactions_partition_485 (tx_sequence_number) {
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
    transactions_partition_486 (tx_sequence_number) {
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
    transactions_partition_487 (tx_sequence_number) {
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
    transactions_partition_488 (tx_sequence_number) {
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
    transactions_partition_489 (tx_sequence_number) {
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
    transactions_partition_49 (tx_sequence_number) {
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
    transactions_partition_490 (tx_sequence_number) {
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
    transactions_partition_491 (tx_sequence_number) {
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
    transactions_partition_492 (tx_sequence_number) {
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
    transactions_partition_493 (tx_sequence_number) {
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
    transactions_partition_494 (tx_sequence_number) {
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
    transactions_partition_495 (tx_sequence_number) {
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
    transactions_partition_496 (tx_sequence_number) {
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
    transactions_partition_497 (tx_sequence_number) {
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
    transactions_partition_498 (tx_sequence_number) {
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
    transactions_partition_499 (tx_sequence_number) {
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
    transactions_partition_5 (tx_sequence_number) {
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
    transactions_partition_50 (tx_sequence_number) {
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
    transactions_partition_500 (tx_sequence_number) {
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
    transactions_partition_501 (tx_sequence_number) {
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
    transactions_partition_502 (tx_sequence_number) {
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
    transactions_partition_503 (tx_sequence_number) {
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
    transactions_partition_504 (tx_sequence_number) {
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
    transactions_partition_505 (tx_sequence_number) {
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
    transactions_partition_506 (tx_sequence_number) {
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
    transactions_partition_507 (tx_sequence_number) {
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
    transactions_partition_508 (tx_sequence_number) {
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
    transactions_partition_509 (tx_sequence_number) {
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
    transactions_partition_51 (tx_sequence_number) {
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
    transactions_partition_510 (tx_sequence_number) {
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
    transactions_partition_511 (tx_sequence_number) {
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
    transactions_partition_512 (tx_sequence_number) {
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
    transactions_partition_513 (tx_sequence_number) {
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
    transactions_partition_514 (tx_sequence_number) {
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
    transactions_partition_515 (tx_sequence_number) {
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
    transactions_partition_516 (tx_sequence_number) {
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
    transactions_partition_517 (tx_sequence_number) {
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
    transactions_partition_518 (tx_sequence_number) {
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
    transactions_partition_519 (tx_sequence_number) {
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
    transactions_partition_52 (tx_sequence_number) {
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
    transactions_partition_520 (tx_sequence_number) {
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
    transactions_partition_521 (tx_sequence_number) {
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
    transactions_partition_522 (tx_sequence_number) {
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
    transactions_partition_523 (tx_sequence_number) {
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
    transactions_partition_524 (tx_sequence_number) {
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
    transactions_partition_525 (tx_sequence_number) {
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
    transactions_partition_526 (tx_sequence_number) {
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
    transactions_partition_527 (tx_sequence_number) {
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
    transactions_partition_528 (tx_sequence_number) {
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
    transactions_partition_529 (tx_sequence_number) {
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
    transactions_partition_53 (tx_sequence_number) {
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
    transactions_partition_530 (tx_sequence_number) {
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
    transactions_partition_531 (tx_sequence_number) {
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
    transactions_partition_532 (tx_sequence_number) {
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
    transactions_partition_533 (tx_sequence_number) {
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
    transactions_partition_534 (tx_sequence_number) {
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
    transactions_partition_535 (tx_sequence_number) {
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
    transactions_partition_536 (tx_sequence_number) {
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
    transactions_partition_537 (tx_sequence_number) {
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
    transactions_partition_538 (tx_sequence_number) {
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
    transactions_partition_539 (tx_sequence_number) {
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
    transactions_partition_54 (tx_sequence_number) {
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
    transactions_partition_540 (tx_sequence_number) {
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
    transactions_partition_541 (tx_sequence_number) {
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
    transactions_partition_542 (tx_sequence_number) {
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
    transactions_partition_543 (tx_sequence_number) {
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
    transactions_partition_544 (tx_sequence_number) {
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
    transactions_partition_545 (tx_sequence_number) {
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
    transactions_partition_546 (tx_sequence_number) {
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
    transactions_partition_547 (tx_sequence_number) {
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
    transactions_partition_548 (tx_sequence_number) {
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
    transactions_partition_549 (tx_sequence_number) {
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
    transactions_partition_55 (tx_sequence_number) {
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
    transactions_partition_550 (tx_sequence_number) {
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
    transactions_partition_551 (tx_sequence_number) {
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
    transactions_partition_552 (tx_sequence_number) {
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
    transactions_partition_553 (tx_sequence_number) {
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
    transactions_partition_554 (tx_sequence_number) {
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
    transactions_partition_555 (tx_sequence_number) {
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
    transactions_partition_556 (tx_sequence_number) {
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
    transactions_partition_557 (tx_sequence_number) {
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
    transactions_partition_56 (tx_sequence_number) {
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
    transactions_partition_57 (tx_sequence_number) {
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
    transactions_partition_58 (tx_sequence_number) {
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
    transactions_partition_59 (tx_sequence_number) {
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
    transactions_partition_6 (tx_sequence_number) {
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
    transactions_partition_60 (tx_sequence_number) {
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
    transactions_partition_61 (tx_sequence_number) {
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
    transactions_partition_62 (tx_sequence_number) {
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
    transactions_partition_63 (tx_sequence_number) {
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
    transactions_partition_64 (tx_sequence_number) {
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
    transactions_partition_65 (tx_sequence_number) {
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
    transactions_partition_66 (tx_sequence_number) {
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
    transactions_partition_67 (tx_sequence_number) {
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
    transactions_partition_68 (tx_sequence_number) {
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
    transactions_partition_69 (tx_sequence_number) {
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
    transactions_partition_7 (tx_sequence_number) {
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
    transactions_partition_70 (tx_sequence_number) {
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
    transactions_partition_71 (tx_sequence_number) {
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
    transactions_partition_72 (tx_sequence_number) {
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
    transactions_partition_73 (tx_sequence_number) {
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
    transactions_partition_74 (tx_sequence_number) {
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
    transactions_partition_75 (tx_sequence_number) {
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
    transactions_partition_76 (tx_sequence_number) {
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
    transactions_partition_77 (tx_sequence_number) {
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
    transactions_partition_78 (tx_sequence_number) {
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
    transactions_partition_79 (tx_sequence_number) {
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
    transactions_partition_8 (tx_sequence_number) {
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
    transactions_partition_80 (tx_sequence_number) {
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
    transactions_partition_81 (tx_sequence_number) {
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
    transactions_partition_82 (tx_sequence_number) {
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
    transactions_partition_83 (tx_sequence_number) {
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
    transactions_partition_84 (tx_sequence_number) {
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
    transactions_partition_85 (tx_sequence_number) {
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
    transactions_partition_86 (tx_sequence_number) {
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
    transactions_partition_87 (tx_sequence_number) {
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
    transactions_partition_88 (tx_sequence_number) {
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
    transactions_partition_89 (tx_sequence_number) {
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
    transactions_partition_9 (tx_sequence_number) {
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
    transactions_partition_90 (tx_sequence_number) {
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
    transactions_partition_91 (tx_sequence_number) {
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
    transactions_partition_92 (tx_sequence_number) {
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
    transactions_partition_93 (tx_sequence_number) {
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
    transactions_partition_94 (tx_sequence_number) {
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
    transactions_partition_95 (tx_sequence_number) {
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
    transactions_partition_96 (tx_sequence_number) {
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
    transactions_partition_97 (tx_sequence_number) {
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
    transactions_partition_98 (tx_sequence_number) {
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
    transactions_partition_99 (tx_sequence_number) {
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
    tx_affected_addresses (affected, tx_sequence_number) {
        tx_sequence_number -> Int8,
        affected -> Bytea,
        sender -> Bytea,
    }
}

diesel::table! {
    tx_affected_objects (affected, tx_sequence_number) {
        tx_sequence_number -> Int8,
        affected -> Bytea,
        sender -> Bytea,
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

diesel::table! {
    watermarks (pipeline) {
        pipeline -> Text,
        epoch_hi_inclusive -> Int8,
        checkpoint_hi_inclusive -> Int8,
        tx_hi -> Int8,
        epoch_lo -> Int8,
        reader_lo -> Int8,
        timestamp_ms -> Int8,
        pruner_hi -> Int8,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    chain_identifier,
    checkpoints,
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
    events_partition_1,
    events_partition_10,
    events_partition_100,
    events_partition_101,
    events_partition_102,
    events_partition_103,
    events_partition_104,
    events_partition_105,
    events_partition_106,
    events_partition_107,
    events_partition_108,
    events_partition_109,
    events_partition_11,
    events_partition_110,
    events_partition_111,
    events_partition_112,
    events_partition_113,
    events_partition_114,
    events_partition_115,
    events_partition_116,
    events_partition_117,
    events_partition_118,
    events_partition_119,
    events_partition_12,
    events_partition_120,
    events_partition_121,
    events_partition_122,
    events_partition_123,
    events_partition_124,
    events_partition_125,
    events_partition_126,
    events_partition_127,
    events_partition_128,
    events_partition_129,
    events_partition_13,
    events_partition_130,
    events_partition_131,
    events_partition_132,
    events_partition_133,
    events_partition_134,
    events_partition_135,
    events_partition_136,
    events_partition_137,
    events_partition_138,
    events_partition_139,
    events_partition_14,
    events_partition_140,
    events_partition_141,
    events_partition_142,
    events_partition_143,
    events_partition_144,
    events_partition_145,
    events_partition_146,
    events_partition_147,
    events_partition_148,
    events_partition_149,
    events_partition_15,
    events_partition_150,
    events_partition_151,
    events_partition_152,
    events_partition_153,
    events_partition_154,
    events_partition_155,
    events_partition_156,
    events_partition_157,
    events_partition_158,
    events_partition_159,
    events_partition_16,
    events_partition_160,
    events_partition_161,
    events_partition_162,
    events_partition_163,
    events_partition_164,
    events_partition_165,
    events_partition_166,
    events_partition_167,
    events_partition_168,
    events_partition_169,
    events_partition_17,
    events_partition_170,
    events_partition_171,
    events_partition_172,
    events_partition_173,
    events_partition_174,
    events_partition_175,
    events_partition_176,
    events_partition_177,
    events_partition_178,
    events_partition_179,
    events_partition_18,
    events_partition_180,
    events_partition_181,
    events_partition_182,
    events_partition_183,
    events_partition_184,
    events_partition_185,
    events_partition_186,
    events_partition_187,
    events_partition_188,
    events_partition_189,
    events_partition_19,
    events_partition_190,
    events_partition_191,
    events_partition_192,
    events_partition_193,
    events_partition_194,
    events_partition_195,
    events_partition_196,
    events_partition_197,
    events_partition_198,
    events_partition_199,
    events_partition_2,
    events_partition_20,
    events_partition_200,
    events_partition_201,
    events_partition_202,
    events_partition_203,
    events_partition_204,
    events_partition_205,
    events_partition_206,
    events_partition_207,
    events_partition_208,
    events_partition_209,
    events_partition_21,
    events_partition_210,
    events_partition_211,
    events_partition_212,
    events_partition_213,
    events_partition_214,
    events_partition_215,
    events_partition_216,
    events_partition_217,
    events_partition_218,
    events_partition_219,
    events_partition_22,
    events_partition_220,
    events_partition_221,
    events_partition_222,
    events_partition_223,
    events_partition_224,
    events_partition_225,
    events_partition_226,
    events_partition_227,
    events_partition_228,
    events_partition_229,
    events_partition_23,
    events_partition_230,
    events_partition_231,
    events_partition_232,
    events_partition_233,
    events_partition_234,
    events_partition_235,
    events_partition_236,
    events_partition_237,
    events_partition_238,
    events_partition_239,
    events_partition_24,
    events_partition_240,
    events_partition_241,
    events_partition_242,
    events_partition_243,
    events_partition_244,
    events_partition_245,
    events_partition_246,
    events_partition_247,
    events_partition_248,
    events_partition_249,
    events_partition_25,
    events_partition_250,
    events_partition_251,
    events_partition_252,
    events_partition_253,
    events_partition_254,
    events_partition_255,
    events_partition_256,
    events_partition_257,
    events_partition_258,
    events_partition_259,
    events_partition_26,
    events_partition_260,
    events_partition_261,
    events_partition_262,
    events_partition_263,
    events_partition_264,
    events_partition_265,
    events_partition_266,
    events_partition_267,
    events_partition_268,
    events_partition_269,
    events_partition_27,
    events_partition_270,
    events_partition_271,
    events_partition_272,
    events_partition_273,
    events_partition_274,
    events_partition_275,
    events_partition_276,
    events_partition_277,
    events_partition_278,
    events_partition_279,
    events_partition_28,
    events_partition_280,
    events_partition_281,
    events_partition_282,
    events_partition_283,
    events_partition_284,
    events_partition_285,
    events_partition_286,
    events_partition_287,
    events_partition_288,
    events_partition_289,
    events_partition_29,
    events_partition_290,
    events_partition_291,
    events_partition_292,
    events_partition_293,
    events_partition_294,
    events_partition_295,
    events_partition_296,
    events_partition_297,
    events_partition_298,
    events_partition_299,
    events_partition_3,
    events_partition_30,
    events_partition_300,
    events_partition_301,
    events_partition_302,
    events_partition_303,
    events_partition_304,
    events_partition_305,
    events_partition_306,
    events_partition_307,
    events_partition_308,
    events_partition_309,
    events_partition_31,
    events_partition_310,
    events_partition_311,
    events_partition_312,
    events_partition_313,
    events_partition_314,
    events_partition_315,
    events_partition_316,
    events_partition_317,
    events_partition_318,
    events_partition_319,
    events_partition_32,
    events_partition_320,
    events_partition_321,
    events_partition_322,
    events_partition_323,
    events_partition_324,
    events_partition_325,
    events_partition_326,
    events_partition_327,
    events_partition_328,
    events_partition_329,
    events_partition_33,
    events_partition_330,
    events_partition_331,
    events_partition_332,
    events_partition_333,
    events_partition_334,
    events_partition_335,
    events_partition_336,
    events_partition_337,
    events_partition_338,
    events_partition_339,
    events_partition_34,
    events_partition_340,
    events_partition_341,
    events_partition_342,
    events_partition_343,
    events_partition_344,
    events_partition_345,
    events_partition_346,
    events_partition_347,
    events_partition_348,
    events_partition_349,
    events_partition_35,
    events_partition_350,
    events_partition_351,
    events_partition_352,
    events_partition_353,
    events_partition_354,
    events_partition_355,
    events_partition_356,
    events_partition_357,
    events_partition_358,
    events_partition_359,
    events_partition_36,
    events_partition_360,
    events_partition_361,
    events_partition_362,
    events_partition_363,
    events_partition_364,
    events_partition_365,
    events_partition_366,
    events_partition_367,
    events_partition_368,
    events_partition_369,
    events_partition_37,
    events_partition_370,
    events_partition_371,
    events_partition_372,
    events_partition_373,
    events_partition_374,
    events_partition_375,
    events_partition_376,
    events_partition_377,
    events_partition_378,
    events_partition_379,
    events_partition_38,
    events_partition_380,
    events_partition_381,
    events_partition_382,
    events_partition_383,
    events_partition_384,
    events_partition_385,
    events_partition_386,
    events_partition_387,
    events_partition_388,
    events_partition_389,
    events_partition_39,
    events_partition_390,
    events_partition_391,
    events_partition_392,
    events_partition_393,
    events_partition_394,
    events_partition_395,
    events_partition_396,
    events_partition_397,
    events_partition_398,
    events_partition_399,
    events_partition_4,
    events_partition_40,
    events_partition_400,
    events_partition_401,
    events_partition_402,
    events_partition_403,
    events_partition_404,
    events_partition_405,
    events_partition_406,
    events_partition_407,
    events_partition_408,
    events_partition_409,
    events_partition_41,
    events_partition_410,
    events_partition_411,
    events_partition_412,
    events_partition_413,
    events_partition_414,
    events_partition_415,
    events_partition_416,
    events_partition_417,
    events_partition_418,
    events_partition_419,
    events_partition_42,
    events_partition_420,
    events_partition_421,
    events_partition_422,
    events_partition_423,
    events_partition_424,
    events_partition_425,
    events_partition_426,
    events_partition_427,
    events_partition_428,
    events_partition_429,
    events_partition_43,
    events_partition_430,
    events_partition_431,
    events_partition_432,
    events_partition_433,
    events_partition_434,
    events_partition_435,
    events_partition_436,
    events_partition_437,
    events_partition_438,
    events_partition_439,
    events_partition_44,
    events_partition_440,
    events_partition_441,
    events_partition_442,
    events_partition_443,
    events_partition_444,
    events_partition_445,
    events_partition_446,
    events_partition_447,
    events_partition_448,
    events_partition_449,
    events_partition_45,
    events_partition_450,
    events_partition_451,
    events_partition_452,
    events_partition_453,
    events_partition_454,
    events_partition_455,
    events_partition_456,
    events_partition_457,
    events_partition_458,
    events_partition_459,
    events_partition_46,
    events_partition_460,
    events_partition_461,
    events_partition_462,
    events_partition_463,
    events_partition_464,
    events_partition_465,
    events_partition_466,
    events_partition_467,
    events_partition_468,
    events_partition_469,
    events_partition_47,
    events_partition_470,
    events_partition_471,
    events_partition_472,
    events_partition_473,
    events_partition_474,
    events_partition_475,
    events_partition_476,
    events_partition_477,
    events_partition_478,
    events_partition_479,
    events_partition_48,
    events_partition_480,
    events_partition_481,
    events_partition_482,
    events_partition_483,
    events_partition_484,
    events_partition_485,
    events_partition_486,
    events_partition_487,
    events_partition_488,
    events_partition_489,
    events_partition_49,
    events_partition_490,
    events_partition_491,
    events_partition_492,
    events_partition_493,
    events_partition_494,
    events_partition_495,
    events_partition_496,
    events_partition_497,
    events_partition_498,
    events_partition_499,
    events_partition_5,
    events_partition_50,
    events_partition_500,
    events_partition_501,
    events_partition_502,
    events_partition_503,
    events_partition_504,
    events_partition_505,
    events_partition_506,
    events_partition_507,
    events_partition_508,
    events_partition_509,
    events_partition_51,
    events_partition_510,
    events_partition_511,
    events_partition_512,
    events_partition_513,
    events_partition_514,
    events_partition_515,
    events_partition_516,
    events_partition_517,
    events_partition_518,
    events_partition_519,
    events_partition_52,
    events_partition_520,
    events_partition_521,
    events_partition_522,
    events_partition_523,
    events_partition_524,
    events_partition_525,
    events_partition_526,
    events_partition_527,
    events_partition_528,
    events_partition_529,
    events_partition_53,
    events_partition_530,
    events_partition_531,
    events_partition_532,
    events_partition_533,
    events_partition_534,
    events_partition_535,
    events_partition_536,
    events_partition_537,
    events_partition_538,
    events_partition_539,
    events_partition_54,
    events_partition_540,
    events_partition_541,
    events_partition_542,
    events_partition_543,
    events_partition_544,
    events_partition_545,
    events_partition_546,
    events_partition_547,
    events_partition_548,
    events_partition_549,
    events_partition_55,
    events_partition_550,
    events_partition_551,
    events_partition_552,
    events_partition_553,
    events_partition_554,
    events_partition_555,
    events_partition_556,
    events_partition_557,
    events_partition_56,
    events_partition_57,
    events_partition_58,
    events_partition_59,
    events_partition_6,
    events_partition_60,
    events_partition_61,
    events_partition_62,
    events_partition_63,
    events_partition_64,
    events_partition_65,
    events_partition_66,
    events_partition_67,
    events_partition_68,
    events_partition_69,
    events_partition_7,
    events_partition_70,
    events_partition_71,
    events_partition_72,
    events_partition_73,
    events_partition_74,
    events_partition_75,
    events_partition_76,
    events_partition_77,
    events_partition_78,
    events_partition_79,
    events_partition_8,
    events_partition_80,
    events_partition_81,
    events_partition_82,
    events_partition_83,
    events_partition_84,
    events_partition_85,
    events_partition_86,
    events_partition_87,
    events_partition_88,
    events_partition_89,
    events_partition_9,
    events_partition_90,
    events_partition_91,
    events_partition_92,
    events_partition_93,
    events_partition_94,
    events_partition_95,
    events_partition_96,
    events_partition_97,
    events_partition_98,
    events_partition_99,
    feature_flags,
    full_objects_history,
    objects,
    objects_history,
    objects_history_partition_551,
    objects_history_partition_552,
    objects_history_partition_553,
    objects_history_partition_554,
    objects_history_partition_555,
    objects_history_partition_556,
    objects_history_partition_557,
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
    protocol_configs,
    pruner_cp_watermark,
    raw_checkpoints,
    test_deletion_table,
    transactions,
    transactions_partition_0,
    transactions_partition_1,
    transactions_partition_10,
    transactions_partition_100,
    transactions_partition_101,
    transactions_partition_102,
    transactions_partition_103,
    transactions_partition_104,
    transactions_partition_105,
    transactions_partition_106,
    transactions_partition_107,
    transactions_partition_108,
    transactions_partition_109,
    transactions_partition_11,
    transactions_partition_110,
    transactions_partition_111,
    transactions_partition_112,
    transactions_partition_113,
    transactions_partition_114,
    transactions_partition_115,
    transactions_partition_116,
    transactions_partition_117,
    transactions_partition_118,
    transactions_partition_119,
    transactions_partition_12,
    transactions_partition_120,
    transactions_partition_121,
    transactions_partition_122,
    transactions_partition_123,
    transactions_partition_124,
    transactions_partition_125,
    transactions_partition_126,
    transactions_partition_127,
    transactions_partition_128,
    transactions_partition_129,
    transactions_partition_13,
    transactions_partition_130,
    transactions_partition_131,
    transactions_partition_132,
    transactions_partition_133,
    transactions_partition_134,
    transactions_partition_135,
    transactions_partition_136,
    transactions_partition_137,
    transactions_partition_138,
    transactions_partition_139,
    transactions_partition_14,
    transactions_partition_140,
    transactions_partition_141,
    transactions_partition_142,
    transactions_partition_143,
    transactions_partition_144,
    transactions_partition_145,
    transactions_partition_146,
    transactions_partition_147,
    transactions_partition_148,
    transactions_partition_149,
    transactions_partition_15,
    transactions_partition_150,
    transactions_partition_151,
    transactions_partition_152,
    transactions_partition_153,
    transactions_partition_154,
    transactions_partition_155,
    transactions_partition_156,
    transactions_partition_157,
    transactions_partition_158,
    transactions_partition_159,
    transactions_partition_16,
    transactions_partition_160,
    transactions_partition_161,
    transactions_partition_162,
    transactions_partition_163,
    transactions_partition_164,
    transactions_partition_165,
    transactions_partition_166,
    transactions_partition_167,
    transactions_partition_168,
    transactions_partition_169,
    transactions_partition_17,
    transactions_partition_170,
    transactions_partition_171,
    transactions_partition_172,
    transactions_partition_173,
    transactions_partition_174,
    transactions_partition_175,
    transactions_partition_176,
    transactions_partition_177,
    transactions_partition_178,
    transactions_partition_179,
    transactions_partition_18,
    transactions_partition_180,
    transactions_partition_181,
    transactions_partition_182,
    transactions_partition_183,
    transactions_partition_184,
    transactions_partition_185,
    transactions_partition_186,
    transactions_partition_187,
    transactions_partition_188,
    transactions_partition_189,
    transactions_partition_19,
    transactions_partition_190,
    transactions_partition_191,
    transactions_partition_192,
    transactions_partition_193,
    transactions_partition_194,
    transactions_partition_195,
    transactions_partition_196,
    transactions_partition_197,
    transactions_partition_198,
    transactions_partition_199,
    transactions_partition_2,
    transactions_partition_20,
    transactions_partition_200,
    transactions_partition_201,
    transactions_partition_202,
    transactions_partition_203,
    transactions_partition_204,
    transactions_partition_205,
    transactions_partition_206,
    transactions_partition_207,
    transactions_partition_208,
    transactions_partition_209,
    transactions_partition_21,
    transactions_partition_210,
    transactions_partition_211,
    transactions_partition_212,
    transactions_partition_213,
    transactions_partition_214,
    transactions_partition_215,
    transactions_partition_216,
    transactions_partition_217,
    transactions_partition_218,
    transactions_partition_219,
    transactions_partition_22,
    transactions_partition_220,
    transactions_partition_221,
    transactions_partition_222,
    transactions_partition_223,
    transactions_partition_224,
    transactions_partition_225,
    transactions_partition_226,
    transactions_partition_227,
    transactions_partition_228,
    transactions_partition_229,
    transactions_partition_23,
    transactions_partition_230,
    transactions_partition_231,
    transactions_partition_232,
    transactions_partition_233,
    transactions_partition_234,
    transactions_partition_235,
    transactions_partition_236,
    transactions_partition_237,
    transactions_partition_238,
    transactions_partition_239,
    transactions_partition_24,
    transactions_partition_240,
    transactions_partition_241,
    transactions_partition_242,
    transactions_partition_243,
    transactions_partition_244,
    transactions_partition_245,
    transactions_partition_246,
    transactions_partition_247,
    transactions_partition_248,
    transactions_partition_249,
    transactions_partition_25,
    transactions_partition_250,
    transactions_partition_251,
    transactions_partition_252,
    transactions_partition_253,
    transactions_partition_254,
    transactions_partition_255,
    transactions_partition_256,
    transactions_partition_257,
    transactions_partition_258,
    transactions_partition_259,
    transactions_partition_26,
    transactions_partition_260,
    transactions_partition_261,
    transactions_partition_262,
    transactions_partition_263,
    transactions_partition_264,
    transactions_partition_265,
    transactions_partition_266,
    transactions_partition_267,
    transactions_partition_268,
    transactions_partition_269,
    transactions_partition_27,
    transactions_partition_270,
    transactions_partition_271,
    transactions_partition_272,
    transactions_partition_273,
    transactions_partition_274,
    transactions_partition_275,
    transactions_partition_276,
    transactions_partition_277,
    transactions_partition_278,
    transactions_partition_279,
    transactions_partition_28,
    transactions_partition_280,
    transactions_partition_281,
    transactions_partition_282,
    transactions_partition_283,
    transactions_partition_284,
    transactions_partition_285,
    transactions_partition_286,
    transactions_partition_287,
    transactions_partition_288,
    transactions_partition_289,
    transactions_partition_29,
    transactions_partition_290,
    transactions_partition_291,
    transactions_partition_292,
    transactions_partition_293,
    transactions_partition_294,
    transactions_partition_295,
    transactions_partition_296,
    transactions_partition_297,
    transactions_partition_298,
    transactions_partition_299,
    transactions_partition_3,
    transactions_partition_30,
    transactions_partition_300,
    transactions_partition_301,
    transactions_partition_302,
    transactions_partition_303,
    transactions_partition_304,
    transactions_partition_305,
    transactions_partition_306,
    transactions_partition_307,
    transactions_partition_308,
    transactions_partition_309,
    transactions_partition_31,
    transactions_partition_310,
    transactions_partition_311,
    transactions_partition_312,
    transactions_partition_313,
    transactions_partition_314,
    transactions_partition_315,
    transactions_partition_316,
    transactions_partition_317,
    transactions_partition_318,
    transactions_partition_319,
    transactions_partition_32,
    transactions_partition_320,
    transactions_partition_321,
    transactions_partition_322,
    transactions_partition_323,
    transactions_partition_324,
    transactions_partition_325,
    transactions_partition_326,
    transactions_partition_327,
    transactions_partition_328,
    transactions_partition_329,
    transactions_partition_33,
    transactions_partition_330,
    transactions_partition_331,
    transactions_partition_332,
    transactions_partition_333,
    transactions_partition_334,
    transactions_partition_335,
    transactions_partition_336,
    transactions_partition_337,
    transactions_partition_338,
    transactions_partition_339,
    transactions_partition_34,
    transactions_partition_340,
    transactions_partition_341,
    transactions_partition_342,
    transactions_partition_343,
    transactions_partition_344,
    transactions_partition_345,
    transactions_partition_346,
    transactions_partition_347,
    transactions_partition_348,
    transactions_partition_349,
    transactions_partition_35,
    transactions_partition_350,
    transactions_partition_351,
    transactions_partition_352,
    transactions_partition_353,
    transactions_partition_354,
    transactions_partition_355,
    transactions_partition_356,
    transactions_partition_357,
    transactions_partition_358,
    transactions_partition_359,
    transactions_partition_36,
    transactions_partition_360,
    transactions_partition_361,
    transactions_partition_362,
    transactions_partition_363,
    transactions_partition_364,
    transactions_partition_365,
    transactions_partition_366,
    transactions_partition_367,
    transactions_partition_368,
    transactions_partition_369,
    transactions_partition_37,
    transactions_partition_370,
    transactions_partition_371,
    transactions_partition_372,
    transactions_partition_373,
    transactions_partition_374,
    transactions_partition_375,
    transactions_partition_376,
    transactions_partition_377,
    transactions_partition_378,
    transactions_partition_379,
    transactions_partition_38,
    transactions_partition_380,
    transactions_partition_381,
    transactions_partition_382,
    transactions_partition_383,
    transactions_partition_384,
    transactions_partition_385,
    transactions_partition_386,
    transactions_partition_387,
    transactions_partition_388,
    transactions_partition_389,
    transactions_partition_39,
    transactions_partition_390,
    transactions_partition_391,
    transactions_partition_392,
    transactions_partition_393,
    transactions_partition_394,
    transactions_partition_395,
    transactions_partition_396,
    transactions_partition_397,
    transactions_partition_398,
    transactions_partition_399,
    transactions_partition_4,
    transactions_partition_40,
    transactions_partition_400,
    transactions_partition_401,
    transactions_partition_402,
    transactions_partition_403,
    transactions_partition_404,
    transactions_partition_405,
    transactions_partition_406,
    transactions_partition_407,
    transactions_partition_408,
    transactions_partition_409,
    transactions_partition_41,
    transactions_partition_410,
    transactions_partition_411,
    transactions_partition_412,
    transactions_partition_413,
    transactions_partition_414,
    transactions_partition_415,
    transactions_partition_416,
    transactions_partition_417,
    transactions_partition_418,
    transactions_partition_419,
    transactions_partition_42,
    transactions_partition_420,
    transactions_partition_421,
    transactions_partition_422,
    transactions_partition_423,
    transactions_partition_424,
    transactions_partition_425,
    transactions_partition_426,
    transactions_partition_427,
    transactions_partition_428,
    transactions_partition_429,
    transactions_partition_43,
    transactions_partition_430,
    transactions_partition_431,
    transactions_partition_432,
    transactions_partition_433,
    transactions_partition_434,
    transactions_partition_435,
    transactions_partition_436,
    transactions_partition_437,
    transactions_partition_438,
    transactions_partition_439,
    transactions_partition_44,
    transactions_partition_440,
    transactions_partition_441,
    transactions_partition_442,
    transactions_partition_443,
    transactions_partition_444,
    transactions_partition_445,
    transactions_partition_446,
    transactions_partition_447,
    transactions_partition_448,
    transactions_partition_449,
    transactions_partition_45,
    transactions_partition_450,
    transactions_partition_451,
    transactions_partition_452,
    transactions_partition_453,
    transactions_partition_454,
    transactions_partition_455,
    transactions_partition_456,
    transactions_partition_457,
    transactions_partition_458,
    transactions_partition_459,
    transactions_partition_46,
    transactions_partition_460,
    transactions_partition_461,
    transactions_partition_462,
    transactions_partition_463,
    transactions_partition_464,
    transactions_partition_465,
    transactions_partition_466,
    transactions_partition_467,
    transactions_partition_468,
    transactions_partition_469,
    transactions_partition_47,
    transactions_partition_470,
    transactions_partition_471,
    transactions_partition_472,
    transactions_partition_473,
    transactions_partition_474,
    transactions_partition_475,
    transactions_partition_476,
    transactions_partition_477,
    transactions_partition_478,
    transactions_partition_479,
    transactions_partition_48,
    transactions_partition_480,
    transactions_partition_481,
    transactions_partition_482,
    transactions_partition_483,
    transactions_partition_484,
    transactions_partition_485,
    transactions_partition_486,
    transactions_partition_487,
    transactions_partition_488,
    transactions_partition_489,
    transactions_partition_49,
    transactions_partition_490,
    transactions_partition_491,
    transactions_partition_492,
    transactions_partition_493,
    transactions_partition_494,
    transactions_partition_495,
    transactions_partition_496,
    transactions_partition_497,
    transactions_partition_498,
    transactions_partition_499,
    transactions_partition_5,
    transactions_partition_50,
    transactions_partition_500,
    transactions_partition_501,
    transactions_partition_502,
    transactions_partition_503,
    transactions_partition_504,
    transactions_partition_505,
    transactions_partition_506,
    transactions_partition_507,
    transactions_partition_508,
    transactions_partition_509,
    transactions_partition_51,
    transactions_partition_510,
    transactions_partition_511,
    transactions_partition_512,
    transactions_partition_513,
    transactions_partition_514,
    transactions_partition_515,
    transactions_partition_516,
    transactions_partition_517,
    transactions_partition_518,
    transactions_partition_519,
    transactions_partition_52,
    transactions_partition_520,
    transactions_partition_521,
    transactions_partition_522,
    transactions_partition_523,
    transactions_partition_524,
    transactions_partition_525,
    transactions_partition_526,
    transactions_partition_527,
    transactions_partition_528,
    transactions_partition_529,
    transactions_partition_53,
    transactions_partition_530,
    transactions_partition_531,
    transactions_partition_532,
    transactions_partition_533,
    transactions_partition_534,
    transactions_partition_535,
    transactions_partition_536,
    transactions_partition_537,
    transactions_partition_538,
    transactions_partition_539,
    transactions_partition_54,
    transactions_partition_540,
    transactions_partition_541,
    transactions_partition_542,
    transactions_partition_543,
    transactions_partition_544,
    transactions_partition_545,
    transactions_partition_546,
    transactions_partition_547,
    transactions_partition_548,
    transactions_partition_549,
    transactions_partition_55,
    transactions_partition_550,
    transactions_partition_551,
    transactions_partition_552,
    transactions_partition_553,
    transactions_partition_554,
    transactions_partition_555,
    transactions_partition_556,
    transactions_partition_557,
    transactions_partition_56,
    transactions_partition_57,
    transactions_partition_58,
    transactions_partition_59,
    transactions_partition_6,
    transactions_partition_60,
    transactions_partition_61,
    transactions_partition_62,
    transactions_partition_63,
    transactions_partition_64,
    transactions_partition_65,
    transactions_partition_66,
    transactions_partition_67,
    transactions_partition_68,
    transactions_partition_69,
    transactions_partition_7,
    transactions_partition_70,
    transactions_partition_71,
    transactions_partition_72,
    transactions_partition_73,
    transactions_partition_74,
    transactions_partition_75,
    transactions_partition_76,
    transactions_partition_77,
    transactions_partition_78,
    transactions_partition_79,
    transactions_partition_8,
    transactions_partition_80,
    transactions_partition_81,
    transactions_partition_82,
    transactions_partition_83,
    transactions_partition_84,
    transactions_partition_85,
    transactions_partition_86,
    transactions_partition_87,
    transactions_partition_88,
    transactions_partition_89,
    transactions_partition_9,
    transactions_partition_90,
    transactions_partition_91,
    transactions_partition_92,
    transactions_partition_93,
    transactions_partition_94,
    transactions_partition_95,
    transactions_partition_96,
    transactions_partition_97,
    transactions_partition_98,
    transactions_partition_99,
    tx_affected_addresses,
    tx_affected_objects,
    tx_calls_fun,
    tx_calls_mod,
    tx_calls_pkg,
    tx_changed_objects,
    tx_digests,
    tx_input_objects,
    tx_kinds,
    tx_recipients,
    tx_senders,
    watermarks,
);
