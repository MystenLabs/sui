// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::metrics::IndexerObjectProcessorMetrics;

use crate::store::IndexerStore;
use prometheus::Registry;
use tracing::info;

//const OBJECT_EVENT_BATCH_SIZE: usize = 100;

pub struct ObjectProcessor<S> {
    pub store: S,
    pub object_processor_metrics: IndexerObjectProcessorMetrics,
}

impl<S> ObjectProcessor<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    pub fn new(store: S, prometheus_registry: &Registry) -> ObjectProcessor<S> {
        let object_processor_metrics = IndexerObjectProcessorMetrics::new(prometheus_registry);
        Self {
            store,
            object_processor_metrics,
        }
    }

    pub async fn start(&self) -> Result<(), IndexerError> {
        info!("Indexer object processor started...");

        /* let object_log = read_object_log(&mut pg_pool_conn)?;
        let mut last_processed_id = object_log.last_processed_id;

        loop {
            let events_to_process = read_events(
                &mut pg_pool_conn,
                last_processed_id,
                OBJECT_EVENT_BATCH_SIZE,
            )?;
            let event_count = events_to_process.len();
            let sui_events_to_process = events_to_sui_events(&mut pg_pool_conn, events_to_process);
            commit_objects_from_events(&mut pg_pool_conn, sui_events_to_process)?;

            last_processed_id += event_count as i64;
            commit_object_log(&mut pg_pool_conn, last_processed_id)?;
            self.object_processor_metrics
                .total_object_batch_processed
                .inc();
            if event_count < OBJECT_EVENT_BATCH_SIZE {
                sleep(Duration::from_secs_f32(0.1)).await;
            }
        }*/
        Ok(())
    }
}
