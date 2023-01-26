// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::publish_events;
use crate::schema::publish_events::{event_sequence, transaction_digest};
use crate::utils::log_errors_to_pg;
use crate::PgPoolConnection;

use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::result::Error;
use sui_json_rpc_types::{EventPage, SuiEventEnvelope};

#[derive(Queryable, Debug)]
pub struct PublishEvent {
    pub id: i64,
    pub transaction_digest: Option<String>,
    pub event_sequence: i64,
    pub event_time: Option<NaiveDateTime>,
    pub event_type: String,
    pub event_content: String,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = publish_events)]
pub struct NewPublishEvent {
    pub transaction_digest: String,
    pub event_sequence: i64,
    pub event_time: Option<NaiveDateTime>,
    pub event_type: String,
    pub event_content: String,
}

fn event_to_new_publish_event(e: SuiEventEnvelope) -> Result<NewPublishEvent, IndexerError> {
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

    Ok(NewPublishEvent {
        transaction_digest: e.tx_digest.base58_encode(),
        event_sequence: e.id.event_seq,
        event_time: Some(timestamp),
        event_type: e.event.get_event_type(),
        event_content: event_json,
    })
}

pub fn commit_events(
    pg_pool_conn: &mut PgPoolConnection,
    event_page: EventPage,
) -> Result<usize, IndexerError> {
    let events = event_page.data;
    let mut errors = vec![];
    let new_events: Vec<NewPublishEvent> = events
        .into_iter()
        .map(event_to_new_publish_event)
        .filter_map(|r| r.map_err(|e| errors.push(e)).ok())
        .collect();
    log_errors_to_pg(pg_pool_conn, errors);

    let event_commit_result: Result<usize, Error> = pg_pool_conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|conn| {
        diesel::insert_into(publish_events::table)
            .values(&new_events)
            .on_conflict((transaction_digest, event_sequence))
            .do_nothing()
            .execute(conn)
    });

    event_commit_result.map_err(|e| {
        IndexerError::PostgresWriteError(format!(
            "Failed writing publish events to PostgresDB with publish events {:?} and error: {:?}",
            new_events, e
        ))
    })
}
