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
    pub use crate::schema::pg::checkpoints;
    pub use crate::schema::pg::display;
    pub use crate::schema::pg::epochs;
    pub use crate::schema::pg::events;
    pub use crate::schema::pg::objects;
    pub use crate::schema::pg::objects_history;
    pub use crate::schema::pg::objects_snapshot;
    pub use crate::schema::pg::packages;
    pub use crate::schema::pg::pruner_cp_watermark;
    pub use crate::schema::pg::transactions;
    pub use crate::schema::pg::tx_calls;
    pub use crate::schema::pg::tx_changed_objects;
    pub use crate::schema::pg::tx_digests;
    pub use crate::schema::pg::tx_input_objects;
    pub use crate::schema::pg::tx_recipients;
    pub use crate::schema::pg::tx_senders;
}

#[cfg(feature = "mysql-feature")]
#[cfg(not(feature = "postgres-feature"))]
mod inner {
    pub use crate::schema::mysql::checkpoints;
    pub use crate::schema::mysql::display;
    pub use crate::schema::mysql::epochs;
    pub use crate::schema::mysql::events;
    pub use crate::schema::mysql::objects;
    pub use crate::schema::mysql::objects_history;
    pub use crate::schema::mysql::objects_snapshot;
    pub use crate::schema::mysql::packages;
    pub use crate::schema::mysql::pruner_cp_watermark;
    pub use crate::schema::mysql::transactions;
    pub use crate::schema::mysql::tx_calls;
    pub use crate::schema::mysql::tx_changed_objects;
    pub use crate::schema::mysql::tx_digests;
    pub use crate::schema::mysql::tx_input_objects;
    pub use crate::schema::mysql::tx_recipients;
    pub use crate::schema::mysql::tx_senders;
}

pub use inner::checkpoints;
pub use inner::display;
pub use inner::epochs;
pub use inner::events;
pub use inner::objects;
pub use inner::objects_history;
pub use inner::objects_snapshot;
pub use inner::packages;
pub use inner::pruner_cp_watermark;
pub use inner::transactions;
pub use inner::tx_calls;
pub use inner::tx_changed_objects;
pub use inner::tx_digests;
pub use inner::tx_input_objects;
pub use inner::tx_recipients;
pub use inner::tx_senders;

// Postgres only tables
#[cfg(feature = "postgres-feature")]
pub use crate::schema::pg::events_partition_0;
#[cfg(feature = "postgres-feature")]
pub use crate::schema::pg::objects_history_partition_0;
#[cfg(feature = "postgres-feature")]
pub use crate::schema::pg::transactions_partition_0;
