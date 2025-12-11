// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use move_core_types::annotated_value::MoveValue;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_json_rpc_types::type_and_fields_from_move_event_data;
use sui_types::base_types::EpochId;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::event::Event;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::Row;
use crate::package_store::PackageCache;
use crate::tables::EventRow;

pub struct EventProcessor {
    package_cache: Arc<PackageCache>,
}

impl EventProcessor {
    pub fn new(package_cache: Arc<PackageCache>) -> Self {
        Self { package_cache }
    }
}

impl Row for EventRow {
    fn get_epoch(&self) -> EpochId {
        self.epoch
    }

    fn get_checkpoint(&self) -> u64 {
        self.checkpoint
    }
}

#[async_trait]
impl Processor for EventProcessor {
    const NAME: &'static str = "events";
    const FANOUT: usize = 10;
    type Value = EventRow;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let epoch = checkpoint.summary.data().epoch;
        let checkpoint_seq = checkpoint.summary.data().sequence_number;
        let timestamp_ms = checkpoint.summary.data().timestamp_ms;

        let mut entries = Vec::new();

        for executed_tx in &checkpoint.transactions {
            let digest = executed_tx.effects.transaction_digest();

            if let Some(events) = &executed_tx.events {
                for (idx, event) in events.data.iter().enumerate() {
                    let Event {
                        package_id,
                        transaction_module,
                        sender,
                        type_,
                        contents,
                    } = event;

                    let layout = self
                        .package_cache
                        .resolver_for_epoch(epoch)
                        .type_layout(move_core_types::language_storage::TypeTag::Struct(
                            Box::new(type_.clone()),
                        ))
                        .await?;

                    let move_value = MoveValue::simple_deserialize(contents, &layout)?;
                    let (_, event_json) = type_and_fields_from_move_event_data(move_value)?;

                    let row = EventRow {
                        transaction_digest: digest.base58_encode(),
                        event_index: idx as u64,
                        checkpoint: checkpoint_seq,
                        epoch,
                        timestamp_ms,
                        sender: sender.to_string(),
                        package: package_id.to_string(),
                        module: transaction_module.to_string(),
                        event_type: type_.to_string(),
                        bcs: "".to_string(),
                        bcs_length: contents.len() as u64,
                        event_json: event_json.to_string(),
                    };

                    entries.push(row);
                }
            }
        }

        Ok(entries)
    }
}
