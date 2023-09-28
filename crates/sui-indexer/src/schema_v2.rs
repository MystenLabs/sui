// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// @generated automatically by Diesel CLI.

diesel::table! {
    checkpoints (sequence_number) {
        sequence_number -> Bigint,
        #[max_length = 255]
        checkpoint_digest -> Varchar,
        epoch -> Bigint,
        network_total_transactions -> Bigint,
        #[max_length = 255]
        previous_checkpoint_digest -> Nullable<Varchar>,
        end_of_epoch -> Bool,
        tx_digests -> Json,
        timestamp_ms -> Bigint,
        total_gas_cost -> Bigint,
        computation_cost -> Bigint,
        storage_cost -> Bigint,
        storage_rebate -> Bigint,
        non_refundable_storage_fee -> Bigint,
        checkpoint_commitments -> Blob,
        validator_signature -> Blob,
        end_of_epoch_data -> Nullable<Blob>,
    }
}

diesel::table! {
    epochs (epoch) {
        epoch -> Bigint,
        validators -> Json,
        first_checkpoint_id -> Bigint,
        epoch_start_timestamp -> Bigint,
        reference_gas_price -> Bigint,
        protocol_version -> Bigint,
        epoch_total_transactions -> Nullable<Bigint>,
        last_checkpoint_id -> Nullable<Bigint>,
        epoch_end_timestamp -> Nullable<Bigint>,
        storage_fund_reinvestment -> Nullable<Bigint>,
        storage_charge -> Nullable<Bigint>,
        storage_rebate -> Nullable<Bigint>,
        storage_fund_balance -> Nullable<Bigint>,
        stake_subsidy_amount -> Nullable<Bigint>,
        total_gas_fees -> Nullable<Bigint>,
        total_stake_rewards_distributed -> Nullable<Bigint>,
        leftover_storage_fund_inflow -> Nullable<Bigint>,
        new_total_stake -> Nullable<Bigint>,
        epoch_commitments -> Nullable<Blob>,
        next_epoch_reference_gas_price -> Nullable<Bigint>,
        next_epoch_protocol_version -> Nullable<Bigint>,
    }
}

diesel::table! {
    events (tx_sequence_number, event_sequence_number) {
        tx_sequence_number -> Bigint,
        event_sequence_number -> Bigint,
        transaction_digest -> Blob,
        checkpoint_sequence_number -> Bigint,
        senders -> Json,
        #[max_length = 255]
        package -> Varchar,
        #[max_length = 127]
        module -> Varchar,
        #[max_length = 255]
        event_type -> Varchar,
        timestamp_ms -> Bigint,
        bcs -> Blob,
    }
}

diesel::table! {
    objects (object_id) {
        #[max_length = 255]
        object_id -> Varchar,
        object_version -> Bigint,
        #[max_length = 255]
        object_digest -> Varchar,
        checkpoint_sequence_number -> Bigint,
        owner_type -> Smallint,
        #[max_length = 255]
        owner_id -> Nullable<Varchar>,
        serialized_object -> Blob,
        #[max_length = 255]
        coin_type -> Nullable<Varchar>,
        coin_balance -> Nullable<Bigint>,
        df_kind -> Nullable<Smallint>,
        df_name -> Nullable<Blob>,
        df_object_type -> Nullable<Text>,
        df_object_id -> Nullable<Blob>,
    }
}

diesel::table! {
    packages (package_id) {
        #[max_length = 255]
        package_id -> Varchar,
        move_package -> Longblob,
    }
}

diesel::table! {
    transactions (tx_sequence_number) {
        tx_sequence_number -> Bigint,
        #[max_length = 255]
        transaction_digest -> Varchar,
        raw_transaction -> Blob,
        raw_effects -> Blob,
        checkpoint_sequence_number -> Bigint,
        timestamp_ms -> Bigint,
        object_changes -> Json,
        balance_changes -> Json,
        events -> Json,
        transaction_kind -> Smallint,
    }
}

diesel::table! {
    tx_indices (tx_sequence_number) {
        tx_sequence_number -> Bigint,
        checkpoint_sequence_number -> Bigint,
        #[max_length = 255]
        transaction_digest -> Varchar,
        input_objects -> Json,
        changed_objects -> Json,
        senders -> Json,
        payers -> Json,
        recipients -> Json,
        packages -> Json,
        package_modules -> Json,
        package_module_functions -> Json,
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
