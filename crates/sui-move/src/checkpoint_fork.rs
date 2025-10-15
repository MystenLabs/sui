// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use sui_sdk::rpc_types::{
    SuiData, SuiObjectDataOptions, SuiTransactionBlockEffectsAPI,
    SuiTransactionBlockResponseOptions,
};
use sui_sdk::SuiClientBuilder;
use sui_types::{
    base_types::ObjectID,
    in_memory_storage::InMemoryStorage,
    object::Object,
};

const BATCH_SIZE: usize = 50;
const MAX_CHECKPOINTS_TO_SCAN: u64 = 1000;

pub struct CheckpointStateLoader {
    rpc_url: String,
}

impl CheckpointStateLoader {
    pub fn new(rpc_url: String) -> Self {
        Self { rpc_url }
    }

    pub async fn load_checkpoint_state(
        &self,
        checkpoint_seq: u64,
        object_id_file: Option<String>,
    ) -> Result<InMemoryStorage> {
        println!(
            "Loading checkpoint state from checkpoint {} at {}",
            checkpoint_seq, self.rpc_url
        );

        let client = SuiClientBuilder::default()
            .build(&self.rpc_url)
            .await
            .map_err(|e| anyhow!("Failed to create Sui client: {}", e))?;

        let checkpoint = client
            .read_api()
            .get_checkpoint(checkpoint_seq.into())
            .await
            .map_err(|e| anyhow!("Failed to fetch checkpoint {}: {}", checkpoint_seq, e))?;

        println!(
            "Checkpoint {} found at epoch {} with {} transactions",
            checkpoint_seq,
            checkpoint.epoch,
            checkpoint.transactions.len()
        );

        let object_ids_to_load = if let Some(file_path) = object_id_file {
            self.load_object_ids_from_file(&file_path)?
        } else {
            self.scan_checkpoints_for_objects(&client, checkpoint_seq).await?
        };

        if object_ids_to_load.is_empty() {
            println!("No objects found to load from checkpoint {}", checkpoint_seq);
            return Ok(InMemoryStorage::default());
        }

        println!("Found {} unique objects to fetch", object_ids_to_load.len());

        let objects = self.fetch_objects_batch(&client, object_ids_to_load).await?;

        let mut storage = InMemoryStorage::default();
        for obj in objects {
            storage.insert_object(obj);
        }

        println!(
            "Successfully loaded {} objects from checkpoint {}",
            storage.objects().len(),
            checkpoint_seq
        );

        Ok(storage)
    }

    fn load_object_ids_from_file(&self, file_path: &str) -> Result<Vec<ObjectID>> {
        println!("Loading object IDs from file: {}", file_path);
        let file = File::open(file_path)
            .map_err(|e| anyhow!("Failed to open object ID file {}: {}", file_path, e))?;
        let reader = BufReader::new(file);
        let mut object_ids = Vec::new();

        for line in reader.lines() {
            let line = line.map_err(|e| anyhow!("Failed to read line from file: {}", e))?;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let object_id = ObjectID::from_hex_literal(line)
                .map_err(|e| anyhow!("Invalid object ID '{}': {}", line, e))?;
            object_ids.push(object_id);
        }

        println!("Loaded {} object IDs from file", object_ids.len());
        Ok(object_ids)
    }

    async fn scan_checkpoints_for_objects(
        &self,
        client: &sui_sdk::SuiClient,
        target_checkpoint: u64,
    ) -> Result<Vec<ObjectID>> {
        let start_checkpoint = target_checkpoint.saturating_sub(MAX_CHECKPOINTS_TO_SCAN);
        println!(
            "Scanning checkpoints {} to {} for modified objects",
            start_checkpoint, target_checkpoint
        );

        let mut object_ids = HashSet::new();
        let mut checkpoint_num = start_checkpoint;

        while checkpoint_num <= target_checkpoint {
            let checkpoint = match client
                .read_api()
                .get_checkpoint(checkpoint_num.into())
                .await
            {
                Ok(cp) => cp,
                Err(_) => {
                    checkpoint_num += 1;
                    continue;
                }
            };

            for tx_digest in &checkpoint.transactions {
                if let Ok(effects) = client
                    .read_api()
                    .get_transaction_with_options(
                        *tx_digest,
                        SuiTransactionBlockResponseOptions::new()
                            .with_effects(),
                    )
                    .await
                {
                    if let Some(effects_data) = effects.effects {
                        for created in effects_data.created() {
                            object_ids.insert(created.object_id());
                        }
                        for mutated in effects_data.mutated() {
                            object_ids.insert(mutated.object_id());
                        }
                        for unwrapped in effects_data.unwrapped() {
                            object_ids.insert(unwrapped.object_id());
                        }
                    }
                }
            }

            if (checkpoint_num - start_checkpoint) % 100 == 0 {
                println!(
                    "Processed {} checkpoints, found {} unique objects",
                    checkpoint_num - start_checkpoint + 1,
                    object_ids.len()
                );
            }

            checkpoint_num += 1;
        }

        println!(
            "Found {} unique objects from {} checkpoints",
            object_ids.len(),
            target_checkpoint - start_checkpoint + 1
        );

        Ok(object_ids.into_iter().collect())
    }

    async fn fetch_objects_batch(
        &self,
        client: &sui_sdk::SuiClient,
        object_ids: Vec<ObjectID>,
    ) -> Result<Vec<Object>> {
        let total_batches = object_ids.len().div_ceil(BATCH_SIZE);
        let mut objects = Vec::new();

        for (batch_idx, chunk) in object_ids.chunks(BATCH_SIZE).enumerate() {
            println!(
                "Fetching batch {}/{} ({} objects)",
                batch_idx + 1,
                total_batches,
                chunk.len()
            );

            let responses = client
                .read_api()
                .multi_get_object_with_options(
                    chunk.to_vec(),
                    SuiObjectDataOptions::new()
                        .with_bcs()
                        .with_owner()
                        .with_type()
                        .with_previous_transaction(),
                )
                .await
                .map_err(|e| anyhow!("Failed to fetch objects batch {}: {}", batch_idx + 1, e))?;

            for obj_response in responses {
                if let Some(obj_data) = obj_response.data {
                    if let Some(bcs_bytes) = obj_data.bcs {
                        if let Some(move_obj) = bcs_bytes.try_as_move() {
                            match bcs::from_bytes::<Object>(&move_obj.bcs_bytes) {
                                Ok(obj) => objects.push(obj),
                                Err(e) => {
                                    eprintln!("Failed to deserialize object: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(objects)
    }
}
