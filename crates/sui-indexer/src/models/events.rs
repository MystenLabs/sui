// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::events;
use crate::schema::events::dsl::{events as events_table, id};
use crate::schema::events::{event_sequence, transaction_sequence};
use crate::utils::log_errors_to_pg;

use chrono::NaiveDateTime;
use diesel::prelude::*;
use sui_json_rpc_types::{EventPage, SuiEvent, SuiEventEnvelope};

#[derive(Queryable, Debug)]
pub struct Event {
    pub id: i64,
    pub transaction_digest: Option<String>,
    pub transaction_sequence: i64,
    pub event_sequence: i64,
    pub event_time: Option<NaiveDateTime>,
    pub event_type: String,
    pub event_content: String,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = events)]
pub struct NewEvent {
    pub transaction_digest: Option<String>,
    pub transaction_sequence: i64,
    pub event_sequence: i64,
    pub event_time: Option<NaiveDateTime>,
    pub event_type: String,
    pub event_content: String,
}

pub fn read_events(
    conn: &mut PgConnection,
    last_processed_id: i64,
    limit: usize,
) -> Result<Vec<Event>, IndexerError> {
    events_table
        .filter(id.gt(last_processed_id))
        .limit(limit as i64)
        .load::<Event>(conn)
        .map_err(|e| {
            IndexerError::PostgresReadError(format!(
                "Failed reading events with last_processed_id {} and error: {:?}",
                last_processed_id, e
            ))
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
        transaction_digest: e.tx_digest.map(|digest| digest.to_string()),
        transaction_sequence: e.id.tx_seq,
        event_sequence: e.id.event_seq,
        event_time: Some(timestamp),
        event_type: e.event.get_event_type(),
        event_content: event_json,
    })
}

pub fn commit_events(
    conn: &mut PgConnection,
    event_page: EventPage,
) -> Result<usize, IndexerError> {
    let events = event_page.data;
    let mut errors = vec![];
    let new_events: Vec<NewEvent> = events
        .into_iter()
        .map(event_to_new_event)
        .filter_map(|r| r.map_err(|e| errors.push(e)).ok())
        .collect();

    log_errors_to_pg(conn, errors);
    diesel::insert_into(events::table)
        .values(&new_events)
        .on_conflict((transaction_sequence, event_sequence))
        .do_nothing()
        .execute(conn)
        .map_err(|e| {
            IndexerError::PostgresWriteError(format!(
                "Failed writing events to PostgresDB with events {:?} and error: {:?}",
                new_events, e
            ))
        })
}

pub fn events_to_sui_events(conn: &mut PgConnection, events: Vec<Event>) -> Vec<SuiEvent> {
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
    log_errors_to_pg(conn, errors);
    sui_events_to_process
}
