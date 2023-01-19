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
use sui_json_rpc_types::{EventPage, SuiEvent, SuiEventEnvelope};
use sui_types::event::EventID;

#[derive(Queryable, Debug)]
pub struct Event {
    pub id: i64,
    pub transaction_digest: String,
    pub event_sequence: i64,
    pub event_time: Option<NaiveDateTime>,
    pub event_type: String,
    pub event_content: String,
    pub next_cursor_transaction_digest: Option<String>,
    pub next_cursor_event_sequence: Option<i64>,
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
    pub next_cursor_event_sequence: Option<i64>,
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

// NOTE: no need to retry here b/c errors here are not transient,
// instead we write them to PG tables for debugging purposes.
pub fn event_to_new_event(e: SuiEventEnvelope) -> Result<NewEvent, IndexerError> {
    let event_json = serde_json::to_string(&e.event).map_err(|err| {
        IndexerError::InsertableParsingError(format!(
            "Failed converting event to JSON with error: {:?}",
            err
        ))
    })?;
    let timestamp = NaiveDateTime::from_timestamp_millis(e.timestamp as i64).ok_or_else(|| {
        IndexerError::DateTimeParsingError(format!(
            "Cannot convert timestamp {:?} to NaiveDateTime",
            e.timestamp
        ))
    })?;

    Ok(NewEvent {
        transaction_digest: format!("{:?}", e.tx_digest),
        event_sequence: e.id.event_seq,
        event_time: Some(timestamp),
        event_type: e.event.get_event_type(),
        event_content: event_json,
        next_cursor_transaction_digest: None,
        next_cursor_event_sequence: None,
    })
}

pub fn commit_events(
    pg_pool_conn: &mut PgPoolConnection,
    event_page: EventPage,
) -> Result<Option<(usize, EventID)>, IndexerError> {
    let events = event_page.data;
    let mut errors = vec![];
    let mut new_events: Vec<NewEvent> = events
        .into_iter()
        .map(event_to_new_event)
        .filter_map(|r| r.map_err(|e| errors.push(e)).ok())
        .collect();
    log_errors_to_pg(pg_pool_conn, errors);

    // No op when there is no more than 1 event, which has been left
    // as next cursor from last iteration.
    if new_events.len() <= 1 {
        return Ok(None);
    }
    let next_cursor: EventID;
    if let Some(next_cursor_val) = event_page.next_cursor {
        next_cursor = next_cursor_val.clone();
        // unwrap is safe because we already checked the length of new_events
        let mut last_event = new_events.pop().unwrap();
        last_event.next_cursor_transaction_digest = Some(next_cursor_val.tx_digest.base58_encode());
        last_event.next_cursor_event_sequence = Some(next_cursor_val.event_seq);
        new_events.push(last_event);
    } else {
        // unwrap here are safe because we already checked the length of new_events
        let next_cursor_event = new_events.pop().unwrap();
        let mut last_event = new_events.pop().unwrap();
        last_event.next_cursor_transaction_digest =
            Some(next_cursor_event.transaction_digest.clone());
        last_event.next_cursor_event_sequence = Some(next_cursor_event.event_sequence);
        new_events.push(last_event);

        let tx_digest = next_cursor_event.transaction_digest.parse().map_err(|e| {
            IndexerError::TransactionDigestParsingError(format!(
                "Failed parsing transaction digest {:?} with error: {:?}",
                next_cursor_event.transaction_digest, e
            ))
        })?;
        next_cursor = EventID {
            tx_digest,
            event_seq: next_cursor_event.event_sequence,
        };
    }

    let event_commit_result: Result<usize, Error> = pg_pool_conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|conn| {
        diesel::insert_into(events::table)
            .values(&new_events)
            .execute(conn)
    });
    event_commit_result
        .map(|commit_row_count| Some((commit_row_count, next_cursor)))
        .map_err(|e| {
            IndexerError::PostgresWriteError(format!(
                "Failed writing events to PostgresDB with events {:?} and error: {:?}",
                new_events, e
            ))
        })
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
