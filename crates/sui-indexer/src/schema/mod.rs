// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::all)]

#[cfg(feature = "mysql-feature")]
#[cfg(not(feature = "postgres-feature"))]
mod mysql;

#[cfg(feature = "postgres-feature")]
mod pg;

#[cfg(feature = "postgres-feature")]
mod inner {
    pub use crate::schema::pg::chain_identifier;
    pub use crate::schema::pg::checkpoints;
    pub use crate::schema::pg::display;
    pub use crate::schema::pg::epochs;
    pub use crate::schema::pg::event_emit_module;
    pub use crate::schema::pg::event_emit_package;
    pub use crate::schema::pg::event_senders;
    pub use crate::schema::pg::event_struct_instantiation;
    pub use crate::schema::pg::event_struct_module;
    pub use crate::schema::pg::event_struct_name;
    pub use crate::schema::pg::event_struct_package;
    pub use crate::schema::pg::events;
    pub use crate::schema::pg::feature_flags;
    pub use crate::schema::pg::objects;
    pub use crate::schema::pg::objects_history;
    pub use crate::schema::pg::objects_snapshot;
    pub use crate::schema::pg::objects_version;
    pub use crate::schema::pg::packages;
    pub use crate::schema::pg::protocol_configs;
    pub use crate::schema::pg::pruner_cp_watermark;
    pub use crate::schema::pg::transactions;
    pub use crate::schema::pg::tx_calls_fun;
    pub use crate::schema::pg::tx_calls_mod;
    pub use crate::schema::pg::tx_calls_pkg;
    pub use crate::schema::pg::tx_changed_objects;
    pub use crate::schema::pg::tx_digests;
    pub use crate::schema::pg::tx_input_objects;
    pub use crate::schema::pg::tx_kinds;
    pub use crate::schema::pg::tx_recipients;
    pub use crate::schema::pg::tx_senders;
}

#[cfg(feature = "mysql-feature")]
#[cfg(not(feature = "postgres-feature"))]
mod inner {
    pub use crate::schema::mysql::chain_identifier;
    pub use crate::schema::mysql::checkpoints;
    pub use crate::schema::mysql::display;
    pub use crate::schema::mysql::epochs;
    pub use crate::schema::mysql::event_emit_module;
    pub use crate::schema::mysql::event_emit_package;
    pub use crate::schema::mysql::event_senders;
    pub use crate::schema::mysql::event_struct_instantiation;
    pub use crate::schema::mysql::event_struct_module;
    pub use crate::schema::mysql::event_struct_name;
    pub use crate::schema::mysql::event_struct_package;
    pub use crate::schema::mysql::events;
    pub use crate::schema::mysql::feature_flags;
    pub use crate::schema::mysql::objects;
    pub use crate::schema::mysql::objects_history;
    pub use crate::schema::mysql::objects_snapshot;
    pub use crate::schema::mysql::objects_version;
    pub use crate::schema::mysql::packages;
    pub use crate::schema::mysql::protocol_configs;
    pub use crate::schema::mysql::pruner_cp_watermark;
    pub use crate::schema::mysql::transactions;
    pub use crate::schema::mysql::tx_calls_fun;
    pub use crate::schema::mysql::tx_calls_mod;
    pub use crate::schema::mysql::tx_calls_pkg;
    pub use crate::schema::mysql::tx_changed_objects;
    pub use crate::schema::mysql::tx_digests;
    pub use crate::schema::mysql::tx_input_objects;
    pub use crate::schema::mysql::tx_kinds;
    pub use crate::schema::mysql::tx_recipients;
    pub use crate::schema::mysql::tx_senders;
}

pub use inner::chain_identifier;
pub use inner::checkpoints;
pub use inner::display;
pub use inner::epochs;
pub use inner::event_emit_module;
pub use inner::event_emit_package;
pub use inner::event_senders;
pub use inner::event_struct_instantiation;
pub use inner::event_struct_module;
pub use inner::event_struct_name;
pub use inner::event_struct_package;
pub use inner::events;
pub use inner::feature_flags;
pub use inner::objects;
pub use inner::objects_history;
pub use inner::objects_snapshot;
pub use inner::objects_version;
pub use inner::packages;
pub use inner::protocol_configs;
pub use inner::pruner_cp_watermark;
pub use inner::transactions;
pub use inner::tx_calls_fun;
pub use inner::tx_calls_mod;
pub use inner::tx_calls_pkg;
pub use inner::tx_changed_objects;
pub use inner::tx_digests;
pub use inner::tx_input_objects;
pub use inner::tx_kinds;
pub use inner::tx_recipients;
pub use inner::tx_senders;

// Postgres only tables
#[cfg(feature = "postgres-feature")]
pub use crate::schema::pg::events_partition_0;
#[cfg(feature = "postgres-feature")]
pub use crate::schema::pg::objects_history_partition_0;
#[cfg(feature = "postgres-feature")]
pub use crate::schema::pg::transactions_partition_0;
