// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// @generated automatically by Diesel CLI.

pub mod sql_types {
    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "bcs_bytes"))]
    pub struct BcsBytes;

    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "object_status"))]
    pub struct ObjectStatus;

    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "owner_type"))]
    pub struct OwnerType;
}

diesel::table! {
    addresses (account_address) {
        account_address -> Varchar,
        first_appearance_tx -> Varchar,
        first_appearance_time -> Int8,
    }
}

diesel::table! {
    at_risk_validators (epoch, address) {
        epoch -> Int8,
        address -> Text,
        epoch_count -> Int8,
        reported_by -> Array<Nullable<Text>>,
    }
}

diesel::table! {
    checkpoints (sequence_number) {
        sequence_number -> Int8,
        checkpoint_digest -> Varchar,
        epoch -> Int8,
        transactions -> Array<Nullable<Text>>,
        previous_checkpoint_digest -> Nullable<Varchar>,
        end_of_epoch -> Bool,
        total_gas_cost -> Int8,
        total_computation_cost -> Int8,
        total_storage_cost -> Int8,
        total_storage_rebate -> Int8,
        total_transaction_blocks -> Int8,
        total_transactions -> Int8,
        network_total_transactions -> Int8,
        timestamp_ms -> Int8,
        validator_signature -> Text,
    }
}

diesel::table! {
    epochs (epoch) {
        epoch -> Int8,
        first_checkpoint_id -> Int8,
        last_checkpoint_id -> Nullable<Int8>,
        epoch_start_timestamp -> Int8,
        epoch_end_timestamp -> Nullable<Int8>,
        epoch_total_transactions -> Int8,
        next_epoch_version -> Nullable<Int8>,
        next_epoch_committee -> Array<Nullable<Bytea>>,
        next_epoch_committee_stake -> Array<Nullable<Int8>>,
        epoch_commitments -> Array<Nullable<Bytea>>,
        protocol_version -> Nullable<Int8>,
        reference_gas_price -> Nullable<Int8>,
        total_stake -> Nullable<Int8>,
        storage_fund_reinvestment -> Nullable<Int8>,
        storage_charge -> Nullable<Int8>,
        storage_rebate -> Nullable<Int8>,
        storage_fund_balance -> Nullable<Int8>,
        stake_subsidy_amount -> Nullable<Int8>,
        total_gas_fees -> Nullable<Int8>,
        total_stake_rewards_distributed -> Nullable<Int8>,
        leftover_storage_fund_inflow -> Nullable<Int8>,
    }
}

diesel::table! {
    events (id) {
        id -> Int8,
        transaction_digest -> Varchar,
        event_sequence -> Int8,
        sender -> Varchar,
        package -> Varchar,
        module -> Text,
        event_type -> Text,
        event_time_ms -> Nullable<Int8>,
        event_bcs -> Bytea,
    }
}

diesel::table! {
    input_objects (id) {
        id -> Int8,
        transaction_digest -> Varchar,
        checkpoint_sequence_number -> Int8,
        epoch -> Int8,
        object_id -> Varchar,
        object_version -> Nullable<Int8>,
    }
}

diesel::table! {
    move_calls (id) {
        id -> Int8,
        transaction_digest -> Varchar,
        checkpoint_sequence_number -> Int8,
        epoch -> Int8,
        sender -> Varchar,
        move_package -> Text,
        move_module -> Text,
        move_function -> Text,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::OwnerType;
    use super::sql_types::ObjectStatus;
    use super::sql_types::BcsBytes;

    objects (object_id) {
        epoch -> Int8,
        checkpoint -> Int8,
        object_id -> Varchar,
        version -> Int8,
        object_digest -> Varchar,
        owner_type -> OwnerType,
        owner_address -> Nullable<Varchar>,
        initial_shared_version -> Nullable<Int8>,
        previous_transaction -> Varchar,
        object_type -> Varchar,
        object_status -> ObjectStatus,
        has_public_transfer -> Bool,
        storage_rebate -> Int8,
        bcs -> Array<Nullable<BcsBytes>>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::OwnerType;
    use super::sql_types::ObjectStatus;
    use super::sql_types::BcsBytes;

    objects_history (object_id, version, checkpoint) {
        epoch -> Int8,
        checkpoint -> Int8,
        object_id -> Varchar,
        version -> Int8,
        object_digest -> Varchar,
        owner_type -> OwnerType,
        owner_address -> Nullable<Varchar>,
        old_owner_type -> Nullable<OwnerType>,
        old_owner_address -> Nullable<Varchar>,
        initial_shared_version -> Nullable<Int8>,
        previous_transaction -> Varchar,
        object_type -> Varchar,
        object_status -> ObjectStatus,
        has_public_transfer -> Bool,
        storage_rebate -> Int8,
        bcs -> Array<Nullable<BcsBytes>>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::BcsBytes;

    packages (package_id, version) {
        package_id -> Varchar,
        version -> Int8,
        author -> Varchar,
        data -> Array<Nullable<BcsBytes>>,
    }
}

diesel::table! {
    recipients (id) {
        id -> Int8,
        transaction_digest -> Varchar,
        checkpoint_sequence_number -> Int8,
        epoch -> Int8,
        sender -> Varchar,
        recipient -> Varchar,
    }
}

diesel::table! {
    system_states (epoch) {
        epoch -> Int8,
        protocol_version -> Int8,
        system_state_version -> Int8,
        storage_fund -> Int8,
        reference_gas_price -> Int8,
        safe_mode -> Bool,
        epoch_start_timestamp_ms -> Int8,
        epoch_duration_ms -> Int8,
        stake_subsidy_start_epoch -> Int8,
        stake_subsidy_epoch_counter -> Int8,
        stake_subsidy_balance -> Int8,
        stake_subsidy_current_epoch_amount -> Int8,
        total_stake -> Int8,
        pending_active_validators_id -> Text,
        pending_active_validators_size -> Int8,
        pending_removals -> Array<Nullable<Int8>>,
        staking_pool_mappings_id -> Text,
        staking_pool_mappings_size -> Int8,
        inactive_pools_id -> Text,
        inactive_pools_size -> Int8,
        validator_candidates_id -> Text,
        validator_candidates_size -> Int8,
    }
}

diesel::table! {
    transactions (id) {
        id -> Int8,
        transaction_digest -> Varchar,
        sender -> Varchar,
        recipients -> Array<Nullable<Text>>,
        checkpoint_sequence_number -> Nullable<Int8>,
        timestamp_ms -> Nullable<Int8>,
        transaction_kind -> Text,
        transaction_count -> Int8,
        created -> Array<Nullable<Text>>,
        mutated -> Array<Nullable<Text>>,
        deleted -> Array<Nullable<Text>>,
        unwrapped -> Array<Nullable<Text>>,
        wrapped -> Array<Nullable<Text>>,
        move_calls -> Array<Nullable<Text>>,
        gas_object_id -> Varchar,
        gas_object_sequence -> Int8,
        gas_object_digest -> Varchar,
        gas_budget -> Int8,
        total_gas_cost -> Int8,
        computation_cost -> Int8,
        storage_cost -> Int8,
        storage_rebate -> Int8,
        non_refundable_storage_fee -> Int8,
        gas_price -> Int8,
        raw_transaction -> Bytea,
        transaction_content -> Text,
        transaction_effects_content -> Text,
        confirmed_local_execution -> Nullable<Bool>,
    }
}

diesel::table! {
    validators (epoch, sui_address) {
        epoch -> Int8,
        sui_address -> Text,
        protocol_pubkey_bytes -> Bytea,
        network_pubkey_bytes -> Bytea,
        worker_pubkey_bytes -> Bytea,
        proof_of_possession_bytes -> Bytea,
        name -> Text,
        description -> Text,
        image_url -> Text,
        project_url -> Text,
        net_address -> Text,
        p2p_address -> Text,
        primary_address -> Text,
        worker_address -> Text,
        next_epoch_protocol_pubkey_bytes -> Nullable<Bytea>,
        next_epoch_proof_of_possession -> Nullable<Bytea>,
        next_epoch_network_pubkey_bytes -> Nullable<Bytea>,
        next_epoch_worker_pubkey_bytes -> Nullable<Bytea>,
        next_epoch_net_address -> Nullable<Text>,
        next_epoch_p2p_address -> Nullable<Text>,
        next_epoch_primary_address -> Nullable<Text>,
        next_epoch_worker_address -> Nullable<Text>,
        voting_power -> Int8,
        operation_cap_id -> Text,
        gas_price -> Int8,
        commission_rate -> Int8,
        next_epoch_stake -> Int8,
        next_epoch_gas_price -> Int8,
        next_epoch_commission_rate -> Int8,
        staking_pool_id -> Text,
        staking_pool_activation_epoch -> Nullable<Int8>,
        staking_pool_deactivation_epoch -> Nullable<Int8>,
        staking_pool_sui_balance -> Int8,
        rewards_pool -> Int8,
        pool_token_balance -> Int8,
        pending_stake -> Int8,
        pending_total_sui_withdraw -> Int8,
        pending_pool_token_withdraw -> Int8,
        exchange_rates_id -> Text,
        exchange_rates_size -> Int8,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    addresses,
    at_risk_validators,
    checkpoints,
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
