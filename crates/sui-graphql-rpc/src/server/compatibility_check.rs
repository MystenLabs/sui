// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::check_table;
use crate::data::{Db, DbConnection, QueryExecutor};
use crate::error::Error;
use diesel::{OptionalExtension, QueryDsl, SelectableHelper};
use sui_indexer::models::checkpoints::StoredCheckpoint;
use sui_indexer::models::display::StoredDisplay;
use sui_indexer::models::epoch::QueryableEpochInfo;
use sui_indexer::models::events::StoredEvent;
use sui_indexer::models::objects::{StoredHistoryObject, StoredObjectSnapshot};
use sui_indexer::models::packages::StoredPackage;
use sui_indexer::models::transactions::StoredTransaction;
use sui_indexer::models::tx_indices::{
    StoredTxChangedObject, StoredTxDigest, StoredTxFun, StoredTxInputObject, StoredTxMod,
    StoredTxPkg, StoredTxRecipients, StoredTxSenders,
};
use sui_indexer::schema::tx_digests;
use sui_indexer::schema::{
    checkpoints, display, epochs, events, objects_history, objects_snapshot, packages,
    transactions, tx_calls_fun, tx_calls_mod, tx_calls_pkg, tx_changed_objects, tx_input_objects,
    tx_recipients, tx_senders,
};

#[macro_export]
macro_rules! check_table {
    ($conn:expr, $table:path, $type:ty) => {{
        let result: Result<Option<$type>, _> = $conn
            .first(move || $table.select(<$type>::as_select()))
            .optional();
        result.is_ok()
    }};
}

#[macro_export]
macro_rules! generate_check_all_tables {
    ($(($table:ident, $type:ty)),* $(,)?) => {
        pub(crate) async fn check_all_tables(db: &Db) -> Result<bool, Error> {
            use futures::future::join_all;

            let futures = vec![
                $(
                    db.execute(|conn| {
                        Ok::<_, diesel::result::Error>(check_table!(conn, $table::dsl::$table, $type))
                    })
                ),*
            ];

            let results = join_all(futures).await;
            if results.into_iter().all(|res| res.unwrap_or(false)) {
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
    (tx_calls_pkg, StoredTxPkg),
    (tx_calls_mod, StoredTxMod),
    (tx_calls_fun, StoredTxFun),
    (tx_changed_objects, StoredTxChangedObject),
    (tx_digests, StoredTxDigest),
    (tx_input_objects, StoredTxInputObject),
    (tx_recipients, StoredTxRecipients),
    (tx_senders, StoredTxSenders),
);
