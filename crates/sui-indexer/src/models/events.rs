// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::events;
use crate::schema::events::dsl::{events as events_table, id};
use crate::utils::log_errors_to_pg;
use crate::PgPoolConnection;

use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::result::Error;
use sui_json_rpc_types::SuiEvent;
use sui_types::base_types::TransactionDigest;

// NOTE: EVENT_BATCH_SIZE * number of columns in events table
// should less than 65535, which is the max "parameters" Postgres
// can take in one query.
const EVENT_BATCH_SIZE: usize = 1000;

#[derive(Queryable, Debug)]
pub struct Event {
    pub id: i64,
    pub transaction_digest: String,
    pub event_sequence: i64,
    pub event_time: Option<NaiveDateTime>,
    pub event_type: String,
    pub event_content: String,
    pub next_cursor_transaction_digest: Option<String>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = events)]
pub struct NewEvent {
    pub transaction_digest: String,
    pub event_sequence: i64,
    pub event_time: Option<NaiveDateTime>,
    pub event_type: String,
    pub event_content: String,
    pub next_cursor_transaction_digest: Option<String>,
}

#[derive(Clone, Debug)]
pub struct IndexerEventEnvelope {
    pub transaction_digest: TransactionDigest,
    pub timestamp: Option<u64>,
    pub events: Vec<SuiEvent>,
    pub next_cursor: Option<TransactionDigest>,
}

pub fn read_events(
    pg_pool_conn: &mut PgPoolConnection,
    last_processed_id: i64,
    limit: usize,
) -> Result<Vec<Event>, IndexerError> {
    let event_read_result: Result<Vec<Event>, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| {
            events_table
                .filter(id.gt(last_processed_id))
                .limit(limit as i64)
                .load::<Event>(conn)
        });

    event_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading events with last_processed_id {} and error: {:?}",
            last_processed_id, e
        ))
    })
}

pub fn read_last_event(pg_pool_conn: &mut PgPoolConnection) -> Result<Option<Event>, IndexerError> {
    let event_read_result: Result<Option<Event>, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| {
            events_table
                .order(id.desc())
                .limit(1)
                .load::<Event>(conn)
                .map(|mut events| events.pop())
        });

    event_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!("Failed reading last event with error: {:?}", e))
    })
}

pub fn commit_events(
    pg_pool_conn: &mut PgPoolConnection,
    mut event_envelopes: Vec<IndexerEventEnvelope>,
    txn_page_next_cursor: Option<TransactionDigest>,
) -> Result<Option<(usize, TransactionDigest)>, IndexerError> {
    // No op when there is no more than 1 event envelope,
    // which could have  been left as next cursor from last iteration.
    if event_envelopes.len() <= 1 {
        return Ok(None);
    }
    let next_cursor: TransactionDigest;
    if let Some(next_cursor_val) = txn_page_next_cursor {
        // unwrap is safe because we already checked the length of new_events
        let mut last_event_envelope = event_envelopes.pop().unwrap();
        last_event_envelope.next_cursor = Some(next_cursor_val);
        event_envelopes.push(last_event_envelope);
        next_cursor = next_cursor_val;
    } else {
        // unwrap here are safe because we already checked the length of new_events
        let next_cursor_event_envelope = event_envelopes.pop().unwrap();
        let mut last_event_envelope = event_envelopes.pop().unwrap();
        last_event_envelope.next_cursor = Some(next_cursor_event_envelope.transaction_digest);
        event_envelopes.push(last_event_envelope);
        next_cursor = next_cursor_event_envelope.transaction_digest;
    }
    let new_events: Vec<NewEvent> = event_envelopes
        .into_iter()
        .flat_map(|e| event_envelope_to_events(pg_pool_conn, e))
        .collect();

    // NOTE: Postgres can take at most 65535 parameters in a single query, one single txn on private testnet
    // can have thousands of events, thus we will need to batch write here.
    let event_batch_commit_result = commit_events_impl(pg_pool_conn, new_events);
    Ok(Some((event_batch_commit_result, next_cursor)))
}

fn sui_event_to_new_event(
    sui_event: SuiEvent,
    sequence_number: i64,
    transaction_digest: TransactionDigest,
    timestamp_opt: Option<u64>,
    next_cursor: Option<TransactionDigest>,
) -> Result<NewEvent, IndexerError> {
    let event_json = serde_json::to_string(&sui_event).map_err(|err| {
        IndexerError::InsertableParsingError(format!(
            "Failed converting event to JSON with error: {:?}",
            err
        ))
    })?;
    let mut naive_date_time_opt = None;
    if let Some(timestamp) = timestamp_opt {
        let naive_date_time =
            NaiveDateTime::from_timestamp_millis(timestamp as i64).ok_or_else(|| {
                IndexerError::DateTimeParsingError(format!(
                    "Cannot convert timestamp {:?} to NaiveDateTime",
                    timestamp
                ))
            })?;
        naive_date_time_opt = Some(naive_date_time);
    }
    Ok(NewEvent {
        transaction_digest: transaction_digest.base58_encode(),
        event_sequence: sequence_number,
        event_time: naive_date_time_opt,
        event_type: sui_event.get_event_type(),
        event_content: event_json,
        next_cursor_transaction_digest: next_cursor.map(|d| d.base58_encode()),
    })
}

fn event_envelope_to_events(
    pg_pool_conn: &mut PgPoolConnection,
    event_envelope: IndexerEventEnvelope,
) -> Vec<NewEvent> {
    let mut errors = vec![];
    let new_events: Vec<NewEvent> = event_envelope
        .events
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            sui_event_to_new_event(
                e,
                i as i64,
                event_envelope.transaction_digest,
                event_envelope.timestamp,
                event_envelope.next_cursor,
            )
        })
        .filter_map(|r| r.map_err(|e| errors.push(e)).ok())
        .collect();
    log_errors_to_pg(pg_pool_conn, errors);
    new_events
}

fn commit_events_impl(pg_pool_conn: &mut PgPoolConnection, new_events: Vec<NewEvent>) -> usize {
    let mut errors = vec![];
    let mut new_events_to_process = new_events;
    let mut total_event_committed = 0;
    while !new_events_to_process.is_empty() {
        let new_events_to_process_batch = new_events_to_process
            .drain(..std::cmp::min(new_events_to_process.len(), EVENT_BATCH_SIZE))
            .collect::<Vec<_>>();
        let new_events_to_process_batch_len = new_events_to_process_batch.len();

        let event_batch_commit_result: Result<usize, Error> = pg_pool_conn
            .build_transaction()
            .read_write()
            .run::<_, Error, _>(|conn| {
                diesel::insert_into(events::table)
                    .values(&new_events_to_process_batch)
                    .execute(conn)
            });
        match event_batch_commit_result {
            Ok(inserted_count) => {
                total_event_committed += inserted_count;
                if inserted_count != new_events_to_process_batch_len {
                    errors.push(IndexerError::PostgresWriteError(format!(
                        "Inserted {} events, but expected to insert {}",
                        inserted_count, new_events_to_process_batch_len
                    )));
                }
            }
            Err(e) => {
                errors.push(IndexerError::PostgresWriteError(format!(
                    "Failed inserting events with error: {:?}",
                    e
                )));
            }
        }
    }
    log_errors_to_pg(pg_pool_conn, errors);
    total_event_committed
}

pub fn events_to_sui_events(
    pg_pool_conn: &mut PgPoolConnection,
    events: Vec<Event>,
) -> Vec<SuiEvent> {
    let mut errors = vec![];
    let sui_events_to_process: Vec<SuiEvent> = events
        .into_iter()
        .filter_map(|event| {
            let sui_event_str = event.event_content.as_str();
            let sui_event: Result<SuiEvent, IndexerError> = serde_json::from_str(sui_event_str)
                .map_err(|e| {
                    IndexerError::EventDeserializationError(format!(
                        "Failed deserializing event {:?} with error: {:?}",
                        event.event_content, e
                    ))
                });
            sui_event
                .map_err(|e| {
                    errors.push(e.clone());
                    e
                })
                .ok()
        })
        .collect();

    log_errors_to_pg(pg_pool_conn, errors);
    sui_events_to_process
}
