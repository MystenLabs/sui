// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use move_core_types::language_storage::StructTag;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::str::FromStr;
use sui_protocol_config::ProtocolConfig;
use sui_sdk::rpc_types::SuiObjectDataOptions;
use sui_sdk::rpc_types::SuiRawData;
use sui_sdk::SuiClientBuilder;
use sui_types::{
    base_types::{MoveObjectType, ObjectID},
    in_memory_storage::InMemoryStorage,
    object::{Data, MoveObject, Object},
};

pub struct ForkStateLoader {
    rpc_url: String,
}

impl ForkStateLoader {
    pub fn new(rpc_url: String) -> Self {
        Self { rpc_url }
    }

    pub async fn load_objects_from_file(&self, object_id_file: String) -> Result<InMemoryStorage> {
        println!(
            "Loading objects from {} via {}",
            object_id_file, self.rpc_url
        );

        let client = SuiClientBuilder::default()
            .build(&self.rpc_url)
            .await
            .map_err(|e| anyhow!("Failed to create Sui client: {}", e))?;

        let object_ids_to_load = self.load_object_ids_from_file(&object_id_file)?;

        if object_ids_to_load.is_empty() {
            println!("No objects found in file");
            return Ok(InMemoryStorage::default());
        }

        println!("Found {} object IDs to fetch", object_ids_to_load.len());

        let objects = self.fetch_objects(&client, object_ids_to_load).await?;

        let mut storage = InMemoryStorage::default();
        for obj in objects {
            storage.insert_object(obj);
        }

        println!("Successfully loaded {} objects", storage.objects().len());

        Ok(storage)
    }

    fn load_object_ids_from_file(&self, file_path: &str) -> Result<HashSet<ObjectID>> {
        println!("Loading object IDs from file: {}", file_path);
        let file = File::open(file_path)
            .map_err(|e| anyhow!("Failed to open object ID file {}: {}", file_path, e))?;
        let reader = BufReader::new(file);
        let mut object_ids = HashSet::new();

        for line in reader.lines() {
            let line = line.map_err(|e| anyhow!("Failed to read line from file: {}", e))?;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let object_id = ObjectID::from_hex_literal(line)
                .map_err(|e| anyhow!("Invalid object ID '{}': {}", line, e))?;
            object_ids.insert(object_id);
        }

        println!("Loaded {} object IDs from file", object_ids.len());
        Ok(object_ids)
    }

    async fn fetch_objects(
        &self,
        client: &sui_sdk::SuiClient,
        object_ids: HashSet<ObjectID>,
    ) -> Result<Vec<Object>> {
        let mut objects = Vec::new();
        let mut all_object_ids = object_ids.clone();
        let mut processed_ids = HashSet::new();

        // Process objects in waves to handle dynamic fields
        while !all_object_ids.is_empty() {
            let current_wave: Vec<ObjectID> = all_object_ids.iter().cloned().collect();
            all_object_ids.clear();

            for object_id in current_wave.iter() {
                if processed_ids.contains(object_id) {
                    continue;
                }
                processed_ids.insert(*object_id);
                
                // Fetch dynamic fields for this object and add them to the queue
                if let Ok(dynamic_fields) = self.fetch_dynamic_fields(client, *object_id).await {
                    for field_id in dynamic_fields {
                        if !processed_ids.contains(&field_id) && !all_object_ids.contains(&field_id) {
                            all_object_ids.insert(field_id);
                        }
                    }
                }
            }

            // Now fetch all objects in the current wave
            for object_id in current_wave.iter() {
                match client
                    .read_api()
                    .get_object_with_options(
                        *object_id,
                        SuiObjectDataOptions::new()
                            .with_bcs()
                            .with_owner()
                            .with_type()
                            .with_previous_transaction(),
                    )
                    .await
                {
                    Ok(response) => {
                        // Try to convert via bcs deserialization
                        if let Some(obj_data) = response.data {
                            if let Some(SuiRawData::MoveObject(move_obj_rpc)) = obj_data.bcs {
                                // Parse the type string into a StructTag
                                let type_str = move_obj_rpc.type_.to_string();

                                match StructTag::from_str(&type_str) {
                                    Ok(struct_tag) => {
                                        let protocol_config =
                                            ProtocolConfig::get_for_max_version_UNSAFE();

                                        // Construct MoveObject from the RPC data
                                        // The bcs_bytes contain just the Move struct contents, not the full MoveObject
                                        let move_obj = unsafe {
                                            MoveObject::new_from_execution(
                                                MoveObjectType::from(struct_tag),
                                                move_obj_rpc.has_public_transfer,
                                                move_obj_rpc.version,
                                                move_obj_rpc.bcs_bytes,
                                                &protocol_config,
                                                false, // not a system mutation
                                            )
                                        };

                                        match move_obj {
                                            Ok(move_obj) => {
                                                // Now wrap it in Data and create the full Object
                                                let obj = Object::new_from_genesis(
                                                    Data::Move(move_obj),
                                                    obj_data.owner.expect("Object should have owner"),
                                                    obj_data.previous_transaction.expect(
                                                        "Object should have previous_transaction",
                                                    ),
                                                );
                                                println!(
                                                    "  Successfully loaded object: {} (owner: {:?})",
                                                    object_id, obj.owner
                                                );
                                                objects.push(obj);
                                            }
                                            Err(e) => {
                                                eprintln!(
                                                    "  Failed to create MoveObject for {}: {}",
                                                    object_id, e
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "  Failed to parse type tag for {}: {}",
                                            object_id, e
                                        );
                                    }
                                }
                            }
                        } else {
                            eprintln!("  Object {} not found or deleted", object_id);
                        }
                    }
                    Err(e) => {
                        eprintln!("  Failed to fetch object {}: {}", object_id, e);
                    }
                }
            }
        }

        Ok(objects)
    }

    async fn fetch_dynamic_fields(
        &self,
        client: &sui_sdk::SuiClient,
        object_id: ObjectID,
    ) -> Result<Vec<ObjectID>> {
        let mut field_ids = Vec::new();
        
        // Try to get dynamic fields for this object
        match client
            .read_api()
            .get_dynamic_fields(object_id, None, None)
            .await 
        {
            Ok(response) => {
                for field_info in response.data {
                    // field_info.object_id is the ID of the dynamic field object
                    let field_obj_id = field_info.object_id;
                    field_ids.push(field_obj_id);
                    println!("  Found dynamic field: {} (parent: {}, name: {:?})", field_obj_id, object_id, field_info.name);
                }
                
                // If there are more pages, we could fetch them too
                // For now, we'll just handle the first page
            }
            Err(_) => {
                // Not all objects have dynamic fields, so errors are expected
                // No need to log this as an error
            }
        }
        
        Ok(field_ids)
    }
}
