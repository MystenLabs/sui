// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::check_table;
use crate::data::{Db, DbConnection, QueryExecutor};
use crate::error::Error;
use diesel::{sql_query, QueryDsl};
use diesel::{RunQueryDsl, SelectableHelper};
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

pub(crate) async fn check_all_tables(db: &Db) -> Result<bool, Error> {
    let result: bool = db
        .execute(|conn| {
            let mut all_ok = true;

            // Allocate 60 seconds for the compatibility check
            sql_query(format!("SET statement_timeout = {}", 60000)).execute(conn.conn())?;

            all_ok &= check_table!(conn, checkpoints::dsl::checkpoints, StoredCheckpoint);
            all_ok &= check_table!(conn, display::dsl::display, StoredDisplay);
            all_ok &= check_table!(conn, epochs::dsl::epochs, QueryableEpochInfo);
            all_ok &= check_table!(conn, events::dsl::events, StoredEvent);
            all_ok &= check_table!(
                conn,
                objects_history::dsl::objects_history,
                StoredHistoryObject
            );
            all_ok &= check_table!(
                conn,
                objects_snapshot::dsl::objects_snapshot,
                StoredObjectSnapshot
            );
            all_ok &= check_table!(conn, packages::dsl::packages, StoredPackage);
            all_ok &= check_table!(conn, transactions::dsl::transactions, StoredTransaction);
            all_ok &= check_table!(conn, tx_calls::dsl::tx_calls, StoredTxCalls);
            all_ok &= check_table!(
                conn,
                tx_changed_objects::dsl::tx_changed_objects,
                StoredTxChangedObject
            );
            all_ok &= check_table!(conn, tx_digests::dsl::tx_digests, StoredTxDigest);
            all_ok &= check_table!(
                conn,
                tx_input_objects::dsl::tx_input_objects,
                StoredTxInputObject
            );
            all_ok &= check_table!(conn, tx_recipients::dsl::tx_recipients, StoredTxRecipients);
            all_ok &= check_table!(conn, tx_senders::dsl::tx_senders, StoredTxSenders);

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

#[macro_export]
macro_rules! check_table {
    ($conn:expr, $table:path, $type:ty) => {{
        let result: Result<$type, _> = $conn.first(move || $table.select(<$type>::as_select()));
        match result {
            Ok(_) => true,
            Err(_) => false,
        }
    }};
}
