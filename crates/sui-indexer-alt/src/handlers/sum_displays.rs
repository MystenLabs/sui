// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::{anyhow, Result};
use diesel::{upsert::excluded, ExpressionMethods};
use diesel_async::RunQueryDsl;
use futures::future::try_join_all;
use sui_field_count::FieldCount;
use sui_indexer_alt_framework::pipeline::{sequential::Handler, Processor};
use sui_indexer_alt_schema::{displays::StoredDisplay, schema::sum_displays};
use sui_pg_db as db;
use sui_types::{display::DisplayVersionUpdatedEvent, full_checkpoint_content::CheckpointData};

const MAX_INSERT_CHUNK_ROWS: usize = i16::MAX as usize / StoredDisplay::FIELD_COUNT;

pub(crate) struct SumDisplays;

impl Processor for SumDisplays {
    const NAME: &'static str = "sum_displays";

    type Value = StoredDisplay;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData { transactions, .. } = checkpoint.as_ref();

        let mut values = vec![];
        for tx in transactions {
            let Some(events) = &tx.events else {
                continue;
            };

            for event in &events.data {
                let Some((object_type, update)) = DisplayVersionUpdatedEvent::try_from_event(event)
                else {
                    continue;
                };

                values.push(StoredDisplay {
                    object_type: bcs::to_bytes(&object_type).map_err(|e| {
                        anyhow!(
                            "Error serializing object type {}: {e}",
                            object_type.to_canonical_display(/* with_prefix */ true)
                        )
                    })?,

                    display_id: update.id.bytes.to_vec(),
                    display_version: update.version as i16,
                    display: event.contents.clone(),
                })
            }
        }

        Ok(values)
    }
}

#[async_trait::async_trait]
impl Handler for SumDisplays {
    type Batch = BTreeMap<Vec<u8>, Self::Value>;

    fn batch(batch: &mut Self::Batch, values: Vec<Self::Value>) {
        for value in values {
            batch.insert(value.object_type.clone(), value);
        }
    }

    async fn commit(batch: &Self::Batch, conn: &mut db::Connection<'_>) -> Result<usize> {
        let values: Vec<_> = batch.values().cloned().collect();
        let updates = values
            .chunks(MAX_INSERT_CHUNK_ROWS)
            .map(|chunk: &[StoredDisplay]| {
                diesel::insert_into(sum_displays::table)
                    .values(chunk)
                    .on_conflict(sum_displays::object_type)
                    .do_update()
                    .set((
                        sum_displays::display_id.eq(excluded(sum_displays::display_id)),
                        sum_displays::display_version.eq(excluded(sum_displays::display_version)),
                        sum_displays::display.eq(excluded(sum_displays::display)),
                    ))
                    .execute(conn)
            });

        Ok(try_join_all(updates).await?.into_iter().sum())
    }
}
