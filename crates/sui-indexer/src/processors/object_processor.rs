// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer::errors::IndexerError;
use sui_indexer::establish_connection;
use sui_indexer::models::events::{events_to_sui_events, read_events};
use sui_indexer::models::object_logs::{commit_object_log, read_object_log};
use sui_indexer::models::objects::commit_objects_from_events;

use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

const OBJECT_EVENT_BATCH_SIZE: usize = 100;

pub struct ObjectProcessor {
    db_url: String,
}

impl ObjectProcessor {
    pub fn new(db_url: String) -> ObjectProcessor {
        Self { db_url }
    }

    pub async fn start(&self) -> Result<(), IndexerError> {
        info!("Indexer object processor started...");
        let mut pg_conn = establish_connection(self.db_url.clone());
        let object_log = read_object_log(&mut pg_conn)?;
        let mut last_processed_id = object_log.last_processed_id;

        loop {
            let events_to_process =
                read_events(&mut pg_conn, last_processed_id, OBJECT_EVENT_BATCH_SIZE)?;
            let event_count = events_to_process.len();
            let sui_events_to_process = events_to_sui_events(&mut pg_conn, events_to_process);
            commit_objects_from_events(&mut pg_conn, sui_events_to_process)?;

            last_processed_id += event_count as i64;
            commit_object_log(&mut pg_conn, last_processed_id)?;
            if event_count < OBJECT_EVENT_BATCH_SIZE {
                sleep(Duration::from_secs_f32(0.1)).await;
            }
        }
    }
}
