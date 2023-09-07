// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// @generated automatically by Diesel CLI.

diesel::table! {
    active_addresses (account_address) {
        #[max_length = 66]
        account_address -> Varchar,
        #[max_length = 44]
        first_appearance_tx -> Varchar,
        first_appearance_time -> Bigint,
        #[max_length = 44]
        last_appearance_tx -> Varchar,
        last_appearance_time -> Bigint,
    }
}

diesel::table! {
    address_stats (checkpoint) {
        checkpoint -> Bigint,
        epoch -> Bigint,
        timestamp_ms -> Bigint,
        cumulative_addresses -> Bigint,
        cumulative_active_addresses -> Bigint,
        daily_active_addresses -> Bigint,
    }
}

diesel::table! {
    addresses (account_address) {
        #[max_length = 66]
        account_address -> Varchar,
        #[max_length = 44]
        first_appearance_tx -> Varchar,
        first_appearance_time -> Bigint,
        #[max_length = 44]
        last_appearance_tx -> Varchar,
        last_appearance_time -> Bigint,
    }
}

diesel::table! {
    at_risk_validators (epoch, address) {
        epoch -> Bigint,
        #[max_length = 66]
        address -> Varchar,
        epoch_count -> Bigint,
        reported_by -> Json,
    }
}

diesel::table! {
    changed_objects (id) {
        id -> Bigint,
        #[max_length = 44]
        transaction_digest -> Varchar,
        checkpoint_sequence_number -> Bigint,
        epoch -> Bigint,
        #[max_length = 66]
        object_id -> Varchar,
        object_change_type -> Text,
        object_version -> Bigint,
    }
}

diesel::table! {
    checkpoints (sequence_number) {
        sequence_number -> Bigint,
        #[max_length = 255]
        checkpoint_digest -> Varchar,
        epoch -> Bigint,
        transactions -> Json,
        #[max_length = 255]
        previous_checkpoint_digest -> Nullable<Varchar>,
        end_of_epoch -> Bool,
        total_gas_cost -> Bigint,
        total_computation_cost -> Bigint,
        total_storage_cost -> Bigint,
        total_storage_rebate -> Bigint,
        total_transaction_blocks -> Bigint,
        total_transactions -> Bigint,
        total_successful_transaction_blocks -> Bigint,
        total_successful_transactions -> Bigint,
        network_total_transactions -> Bigint,
        timestamp_ms -> Bigint,
        validator_signature -> Text,
    }
}

diesel::table! {
    epoch_move_call_metrics (id) {
        id -> Bigint,
        epoch -> Bigint,
        day -> Bigint,
        move_package -> Text,
        move_module -> Text,
        move_function -> Text,
        count -> Bigint,
    }
}

diesel::table! {
    epochs (epoch) {
        epoch -> Bigint,
        first_checkpoint_id -> Bigint,
        last_checkpoint_id -> Nullable<Bigint>,
        epoch_start_timestamp -> Bigint,
        epoch_end_timestamp -> Nullable<Bigint>,
        epoch_total_transactions -> Bigint,
        next_epoch_version -> Nullable<Bigint>,
        next_epoch_committee -> Json,
        next_epoch_committee_stake -> Json,
        epoch_commitments -> Json,
        protocol_version -> Nullable<Bigint>,
        reference_gas_price -> Nullable<Bigint>,
        total_stake -> Nullable<Bigint>,
        storage_fund_reinvestment -> Nullable<Bigint>,
        storage_charge -> Nullable<Bigint>,
        storage_rebate -> Nullable<Bigint>,
        storage_fund_balance -> Nullable<Bigint>,
        stake_subsidy_amount -> Nullable<Bigint>,
        total_gas_fees -> Nullable<Bigint>,
        total_stake_rewards_distributed -> Nullable<Bigint>,
        leftover_storage_fund_inflow -> Nullable<Bigint>,
    }
}

diesel::table! {
    events (id) {
        id -> Bigint,
        #[max_length = 44]
        transaction_digest -> Varchar,
        event_sequence -> Bigint,
        #[max_length = 66]
        sender -> Varchar,
        #[max_length = 66]
        package -> Varchar,
        module -> Text,
        event_type -> Text,
        event_time_ms -> Nullable<Bigint>,
        event_bcs -> Blob,
    }
}

diesel::table! {
    input_objects (id) {
        id -> Bigint,
        #[max_length = 44]
        transaction_digest -> Varchar,
        checkpoint_sequence_number -> Bigint,
        epoch -> Bigint,
        #[max_length = 66]
        object_id -> Varchar,
        object_version -> Nullable<Bigint>,
    }
}

diesel::table! {
    move_calls (id) {
        id -> Bigint,
        #[max_length = 44]
        transaction_digest -> Varchar,
        checkpoint_sequence_number -> Bigint,
        epoch -> Bigint,
        #[max_length = 66]
        sender -> Varchar,
        move_package -> Text,
        move_module -> Text,
        move_function -> Text,
    }
}

diesel::table! {
    objects (object_id) {
        epoch -> Bigint,
        checkpoint -> Bigint,
        #[max_length = 66]
        object_id -> Varchar,
        version -> Bigint,
        #[max_length = 44]
        object_digest -> Varchar,
        #[max_length = 31]
        owner_type -> Varchar,
        #[max_length = 66]
        owner_address -> Nullable<Varchar>,
        initial_shared_version -> Nullable<Bigint>,
        #[max_length = 44]
        previous_transaction -> Varchar,
        object_type -> Text,
        #[max_length = 31]
        object_status -> Varchar,
        has_public_transfer -> Bool,
        storage_rebate -> Bigint,
        bcs -> Json,
    }
}

diesel::table! {
    objects_history (object_id, version, checkpoint) {
        epoch -> Bigint,
        checkpoint -> Bigint,
        #[max_length = 66]
        object_id -> Varchar,
        version -> Bigint,
        #[max_length = 44]
        object_digest -> Varchar,
        #[max_length = 31]
        owner_type -> Varchar,
        #[max_length = 66]
        owner_address -> Nullable<Varchar>,
        #[max_length = 31]
        old_owner_type -> Nullable<Varchar>,
        #[max_length = 66]
        old_owner_address -> Nullable<Varchar>,
        initial_shared_version -> Nullable<Bigint>,
        #[max_length = 44]
        previous_transaction -> Varchar,
        object_type -> Text,
        #[max_length = 31]
        object_status -> Varchar,
        has_public_transfer -> Bool,
        storage_rebate -> Bigint,
        bcs -> Json,
    }
}

diesel::table! {
    packages (package_id, version) {
        #[max_length = 66]
        package_id -> Varchar,
        version -> Bigint,
        #[max_length = 66]
        author -> Varchar,
        data -> Json,
    }
}

diesel::table! {
    recipients (id) {
        id -> Bigint,
        #[max_length = 44]
        transaction_digest -> Varchar,
        checkpoint_sequence_number -> Bigint,
        epoch -> Bigint,
        #[max_length = 66]
        sender -> Varchar,
        #[max_length = 66]
        recipient -> Varchar,
    }
}

diesel::table! {
    system_states (epoch) {
        epoch -> Bigint,
        protocol_version -> Bigint,
        system_state_version -> Bigint,
        storage_fund -> Bigint,
        reference_gas_price -> Bigint,
        safe_mode -> Bool,
        epoch_start_timestamp_ms -> Bigint,
        epoch_duration_ms -> Bigint,
        stake_subsidy_start_epoch -> Bigint,
        stake_subsidy_epoch_counter -> Bigint,
        stake_subsidy_balance -> Bigint,
        stake_subsidy_current_epoch_amount -> Bigint,
        total_stake -> Bigint,
        pending_active_validators_id -> Text,
        pending_active_validators_size -> Bigint,
        pending_removals -> Json,
        staking_pool_mappings_id -> Text,
        staking_pool_mappings_size -> Bigint,
        inactive_pools_id -> Text,
        inactive_pools_size -> Bigint,
        validator_candidates_id -> Text,
        validator_candidates_size -> Bigint,
    }
}

diesel::table! {
    transactions (id) {
        id -> Bigint,
        #[max_length = 44]
        transaction_digest -> Varchar,
        #[max_length = 255]
        sender -> Varchar,
        recipients -> Json,
        checkpoint_sequence_number -> Nullable<Bigint>,
        timestamp_ms -> Nullable<Bigint>,
        transaction_kind -> Text,
        transaction_count -> Bigint,
        execution_success -> Bool,
        created -> Json,
        mutated -> Json,
        deleted -> Json,
        unwrapped -> Json,
        wrapped -> Json,
        move_calls -> Json,
        #[max_length = 66]
        gas_object_id -> Varchar,
        gas_object_sequence -> Bigint,
        #[max_length = 66]
        gas_object_digest -> Varchar,
        gas_budget -> Bigint,
        total_gas_cost -> Bigint,
        computation_cost -> Bigint,
        storage_cost -> Bigint,
        storage_rebate -> Bigint,
        non_refundable_storage_fee -> Bigint,
        gas_price -> Bigint,
        raw_transaction -> Blob,
        transaction_content -> Text,
        transaction_effects_content -> Text,
        confirmed_local_execution -> Nullable<Bool>,
    }
}

diesel::table! {
    validators (epoch, sui_address) {
        epoch -> Bigint,
        #[max_length = 66]
        sui_address -> Varchar,
        protocol_pubkey_bytes -> Blob,
        network_pubkey_bytes -> Blob,
        worker_pubkey_bytes -> Blob,
        proof_of_possession_bytes -> Blob,
        name -> Text,
        description -> Text,
        image_url -> Text,
        project_url -> Text,
        net_address -> Text,
        p2p_address -> Text,
        primary_address -> Text,
        worker_address -> Text,
        next_epoch_protocol_pubkey_bytes -> Nullable<Blob>,
        next_epoch_proof_of_possession -> Nullable<Blob>,
        next_epoch_network_pubkey_bytes -> Nullable<Blob>,
        next_epoch_worker_pubkey_bytes -> Nullable<Blob>,
        next_epoch_net_address -> Nullable<Text>,
        next_epoch_p2p_address -> Nullable<Text>,
        next_epoch_primary_address -> Nullable<Text>,
        next_epoch_worker_address -> Nullable<Text>,
        voting_power -> Bigint,
        operation_cap_id -> Text,
        gas_price -> Bigint,
        commission_rate -> Bigint,
        next_epoch_stake -> Bigint,
        next_epoch_gas_price -> Bigint,
        next_epoch_commission_rate -> Bigint,
        staking_pool_id -> Text,
        staking_pool_activation_epoch -> Nullable<Bigint>,
        staking_pool_deactivation_epoch -> Nullable<Bigint>,
        staking_pool_sui_balance -> Bigint,
        rewards_pool -> Bigint,
        pool_token_balance -> Bigint,
        pending_stake -> Bigint,
        pending_total_sui_withdraw -> Bigint,
        pending_pool_token_withdraw -> Bigint,
        exchange_rates_id -> Text,
        exchange_rates_size -> Bigint,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    active_addresses,
    address_stats,
    addresses,
    at_risk_validators,
    changed_objects,
    checkpoints,
    epoch_move_call_metrics,
    epochs,
    events,
    input_objects,
    move_calls,
    objects,
    objects_history,
    packages,
    recipients,
    system_states,
    transactions,
    validators,
);
