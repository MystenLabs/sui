// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use fastcrypto::encoding::{Base64, Encoding};

use sui_indexer::framework::Handler;
use sui_rest_api::{CheckpointData, CheckpointTransaction};
use sui_types::effects::TransactionEffects;
use sui_types::object::Object;

use crate::handlers::{
    get_owner_address, get_owner_type, initial_shared_version, AnalyticsHandler,
    ObjectStatusTracker,
};
use crate::tables::ObjectEntry;
use crate::FileType;

pub struct ObjectHandler {
    objects: Vec<ObjectEntry>,
}

#[async_trait::async_trait]
impl Handler for ObjectHandler {
    fn name(&self) -> &str {
        "object"
    }
    async fn process_checkpoint(&mut self, checkpoint_data: &CheckpointData) -> Result<()> {
        let CheckpointData {
            checkpoint_summary,
            transactions: checkpoint_transactions,
            ..
        } = checkpoint_data;
        for checkpoint_transaction in checkpoint_transactions {
            self.process_transaction(
                checkpoint_summary.epoch,
                checkpoint_summary.sequence_number,
                checkpoint_summary.timestamp_ms,
                checkpoint_transaction,
                &checkpoint_transaction.effects,
            );
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<ObjectEntry> for ObjectHandler {
    fn read(&mut self) -> Result<Vec<ObjectEntry>> {
        let cloned = self.objects.clone();
        self.objects.clear();
        Ok(cloned)
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::Object)
    }
}

impl ObjectHandler {
    pub fn new() -> Self {
        ObjectHandler { objects: vec![] }
    }
    fn process_transaction(
        &mut self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
        effects: &TransactionEffects,
    ) {
        let object_status_tracker = ObjectStatusTracker::new(effects);
        checkpoint_transaction
            .output_objects
            .iter()
            .for_each(|object| {
                self.process_object(
                    epoch,
                    checkpoint,
                    timestamp_ms,
                    object,
                    &object_status_tracker,
                )
            });
    }
    // Object data. Only called if there are objects in the transaction.
    // Responsible to build the live object table.
    fn process_object(
        &mut self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        object: &Object,
        object_status_tracker: &ObjectStatusTracker,
    ) {
        let move_obj_opt = object.data.try_as_move();
        let has_public_transfer = move_obj_opt
            .map(|o| o.has_public_transfer())
            .unwrap_or(false);
        let object_type = move_obj_opt.map(|o| o.type_().to_string());
        let object_id = object.id();
        let entry = ObjectEntry {
            object_id: object_id.to_string(),
            digest: object.digest().to_string(),
            version: object.version().value(),
            type_: object_type,
            checkpoint,
            epoch,
            timestamp_ms,
            owner_type: get_owner_type(object),
            owner_address: get_owner_address(object),
            object_status: object_status_tracker
                .get_object_status(&object_id)
                .expect("Object must be in output objects"),
            initial_shared_version: initial_shared_version(object),
            previous_transaction: object.previous_transaction.base58_encode(),
            has_public_transfer,
            storage_rebate: object.storage_rebate,
            bcs: Base64::encode(bcs::to_bytes(object).unwrap()),
            coin_type: object.coin_type_maybe().map(|t| t.to_string()),
            coin_balance: if object.coin_type_maybe().is_some() {
                Some(object.get_coin_value_unsafe())
            } else {
                None
            },
        };
        self.objects.push(entry);
    }
}
