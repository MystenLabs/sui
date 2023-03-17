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
    checkpoints (sequence_number) {
        sequence_number -> Int8,
        checkpoint_digest -> Varchar,
        epoch -> Int8,
        transactions -> Array<Nullable<Text>>,
        previous_checkpoint_digest -> Nullable<Varchar>,
        next_epoch_committee -> Nullable<Text>,
        next_epoch_protocol_version -> Nullable<Int8>,
        end_of_epoch_data -> Nullable<Text>,
        total_gas_cost -> Int8,
        total_computation_cost -> Int8,
        total_storage_cost -> Int8,
        total_storage_rebate -> Int8,
        total_transactions -> Int8,
        total_transactions_current_epoch -> Int8,
        total_transactions_from_genesis -> Int8,
        timestamp_ms -> Int8,
        timestamp_ms_str -> Timestamp,
        checkpoint_tps -> Float4,
    }
}

diesel::table! {
    error_logs (id) {
        id -> Int8,
        error_type -> Varchar,
        error -> Text,
        error_time -> Timestamp,
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
        parsed_json -> Jsonb,
        event_bcs -> Bytea,
    }
}

diesel::table! {
    move_calls (id) {
        id -> Int8,
        transaction_digest -> Varchar,
        checkpoint_sequence_number -> Int8,
        epoch -> Int8,
        sender -> Text,
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

    objects_history (epoch, object_id, version) {
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

    owner (object_id) {
        epoch -> Int8,
        checkpoint -> Int8,
        object_id -> Varchar,
        version -> Int8,
        object_digest -> Varchar,
        owner_type -> OwnerType,
        owner_address -> Nullable<Varchar>,
        object_status -> ObjectStatus,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::OwnerType;
    use super::sql_types::ObjectStatus;

    owner_history (epoch, object_id, version) {
        epoch -> Int8,
        checkpoint -> Int8,
        object_id -> Varchar,
        version -> Int8,
        object_digest -> Varchar,
        owner_type -> Nullable<OwnerType>,
        owner_address -> Nullable<Varchar>,
        old_owner_type -> Nullable<OwnerType>,
        old_owner_address -> Nullable<Varchar>,
        object_status -> ObjectStatus,
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
        recipient -> Varchar,
    }
}

diesel::table! {
    transactions (id) {
        id -> Int8,
        transaction_digest -> Varchar,
        sender -> Varchar,
        recipients -> Array<Nullable<Text>>,
        checkpoint_sequence_number -> Int8,
        timestamp_ms -> Int8,
        transaction_kind -> Text,
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
        gas_price -> Int8,
        transaction_content -> Text,
        transaction_effects_content -> Text,
        confirmed_local_execution -> Nullable<Bool>,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    addresses,
    checkpoints,
    error_logs,
    events,
    move_calls,
    objects,
    objects_history,
    owner,
    owner_history,
    packages,
    recipients,
    transactions,
);
