// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::check_table;
use crate::data::{Db, DbConnection, QueryExecutor};
use crate::error::Error;
use diesel::{sql_query, OptionalExtension, QueryDsl, RunQueryDsl, SelectableHelper};
use sui_indexer::models::checkpoints::StoredCheckpoint;
use sui_indexer::models::display::StoredDisplay;
use sui_indexer::models::epoch::QueryableEpochInfo;
use sui_indexer::models::events::StoredEvent;
use sui_indexer::models::objects::{StoredHistoryObject, StoredObjectSnapshot};
use sui_indexer::models::packages::StoredPackage;
use sui_indexer::models::transactions::StoredTransaction;
use sui_indexer::models::tx_indices::{
    StoredTxCalls, StoredTxChangedObject, StoredTxDigest, StoredTxInputObject, StoredTxRecipients,
    StoredTxSenders,
};
use sui_indexer::schema::tx_digests;
use sui_indexer::schema::{
    checkpoints, display, epochs, events, objects_history, objects_snapshot, packages,
    transactions, tx_calls, tx_changed_objects, tx_input_objects, tx_recipients, tx_senders,
};

#[macro_export]
macro_rules! check_table {
    ($conn:expr, $table:path, $type:ty) => {{
        let result: Result<Option<$type>, _> = $conn
            .first(move || $table.select(<$type>::as_select()))
            .optional();
        match result {
            Ok(_) => true,
            Err(_) => false,
        }
    }};
}

#[macro_export]
macro_rules! generate_check_all_tables {
    ($(($table:ident, $type:ty)),* $(,)?) => {
        pub(crate) async fn check_all_tables(db: &Db) -> Result<bool, Error> {
            let result: bool = db
                .execute(|conn| {
                    let mut all_ok = true;

                    // Allocate 60 seconds for the compatibility check
                    sql_query("SET statement_timeout = 60000").execute(conn.conn())?;

                    $(
                        all_ok &= check_table!(conn, $table::dsl::$table, $type);
                    )*

                    Ok::<_, diesel::result::Error>(all_ok)
                })
                .await?;

            if result {
                Ok(true)
            } else {
                Err(Error::Internal(
                    "One or more tables are missing expected columns".into(),
                ))
            }
        }
    };
}

generate_check_all_tables!(
    (checkpoints, StoredCheckpoint),
    (display, StoredDisplay),
    (epochs, QueryableEpochInfo),
    (events, StoredEvent),
    (objects_history, StoredHistoryObject),
    (objects_snapshot, StoredObjectSnapshot),
    (packages, StoredPackage),
    (transactions, StoredTransaction),
    (tx_calls, StoredTxCalls),
    (tx_changed_objects, StoredTxChangedObject),
    (tx_digests, StoredTxDigest),
    (tx_input_objects, StoredTxInputObject),
    (tx_recipients, StoredTxRecipients),
    (tx_senders, StoredTxSenders),
);
