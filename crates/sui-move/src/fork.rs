// Copyright (c) Amber Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use move_core_types::language_storage::StructTag;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::str::FromStr;
use sui_protocol_config::ProtocolConfig;
use sui_sdk::rpc_types::SuiObjectDataOptions;
use sui_sdk::rpc_types::SuiRawData;
use sui_sdk::SuiClientBuilder;
use sui_types::{
    base_types::{MoveObjectType, ObjectID, SuiAddress},
    in_memory_storage::InMemoryStorage,
    object::{Data, MoveObject, Object},
};

#[derive(Debug, Deserialize)]
struct ObjectIdsToml {
    #[serde(default)]
    objects: Vec<String>,
    #[serde(default)]
    addresses: Vec<String>,
}

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

        let (object_ids_from_file, addresses_from_file) = if object_id_file.ends_with(".toml") {
            self.parse_toml_file(&object_id_file)?
        } else {
            self.parse_ids_and_addresses_from_file(&object_id_file)?
        };

        let mut all_object_ids = object_ids_from_file;

        // For each address found, fetch all owned objects
        for address in addresses_from_file {
            println!("Fetching owned objects for address: {}", address);
            match self.fetch_owned_objects(&client, address).await {
                Ok(owned_ids) => {
                    println!(
                        "  Found {} owned objects for address {}",
                        owned_ids.len(),
                        address
                    );
                    all_object_ids.extend(owned_ids);
                }
                Err(e) => {
                    eprintln!(
                        "  Warning: Failed to fetch owned objects for {}: {}",
                        address, e
                    );
                }
            }
        }

        if all_object_ids.is_empty() {
            println!("No objects found in file");
            return Ok(InMemoryStorage::default());
        }

        println!("Total {} object IDs to fetch", all_object_ids.len());

        let objects = self.fetch_objects(&client, all_object_ids).await?;

        let mut storage = InMemoryStorage::default();
        for obj in objects {
            storage.insert_object(obj);
        }

        println!("Successfully loaded {} objects", storage.objects().len());

        Ok(storage)
    }

    /// Parse a TOML file with structured format
    fn parse_toml_file(
        &self,
        file_path: &str,
    ) -> Result<(HashSet<ObjectID>, HashSet<SuiAddress>)> {
        println!("Parsing TOML file: {}", file_path);
        let content = std::fs::read_to_string(file_path)
            .map_err(|e| anyhow!("Failed to read TOML file {}: {}", file_path, e))?;

        let config: ObjectIdsToml = toml::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse TOML file {}: {}", file_path, e))?;

        let mut object_ids = HashSet::new();
        let mut addresses = HashSet::new();

        // Parse objects
        for obj_str in config.objects {
            let obj_str = obj_str.trim();
            if obj_str.is_empty() {
                continue;
            }
            match ObjectID::from_hex_literal(obj_str) {
                Ok(id) => {
                    object_ids.insert(id);
                    println!("  Found object: {}", id);
                }
                Err(e) => {
                    eprintln!("  Warning: Invalid object ID '{}': {}", obj_str, e);
                }
            }
        }

        // Parse addresses
        for addr_str in config.addresses {
            let addr_str = addr_str.trim();
            if addr_str.is_empty() {
                continue;
            }
            match SuiAddress::from_str(addr_str) {
                Ok(addr) => {
                    addresses.insert(addr);
                    println!("  Found address to preload: {}", addr);
                }
                Err(e) => {
                    eprintln!("  Warning: Invalid address '{}': {}", addr_str, e);
                }
            }
        }

        println!(
            "Parsed {} object IDs and {} addresses from TOML file",
            object_ids.len(),
            addresses.len()
        );
        Ok((object_ids, addresses))
    }

    /// Parse a line as either an ObjectID or a SuiAddress
    /// Returns (object_ids, addresses) from parsing the file
    fn parse_ids_and_addresses_from_file(
        &self,
        file_path: &str,
    ) -> Result<(HashSet<ObjectID>, HashSet<SuiAddress>)> {
        println!("Parsing IDs and addresses from file: {}", file_path);
        let file = File::open(file_path)
            .map_err(|e| anyhow!("Failed to open object ID file {}: {}", file_path, e))?;
        let reader = BufReader::new(file);
        let mut object_ids = HashSet::new();
        let mut addresses = HashSet::new();

        for line in reader.lines() {
            let line = line.map_err(|e| anyhow!("Failed to read line from file: {}", e))?;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Try to parse as ObjectID first
            if let Ok(object_id) = ObjectID::from_hex_literal(line) {
                object_ids.insert(object_id);
            } else if let Ok(address) = SuiAddress::from_str(line) {
                // Try to parse as SuiAddress
                addresses.insert(address);
                println!("  Found user address to preload: {}", address);
            } else {
                eprintln!("  Warning: Could not parse line as ObjectID or Address: {}", line);
            }
        }

        println!(
            "Parsed {} object IDs and {} addresses from file",
            object_ids.len(),
            addresses.len()
        );
        Ok((object_ids, addresses))
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

    /// Fetch all objects owned by a given address
    async fn fetch_owned_objects(
        &self,
        client: &sui_sdk::SuiClient,
        address: SuiAddress,
    ) -> Result<Vec<ObjectID>> {
        let mut all_object_ids = Vec::new();
        let mut cursor = None;

        // Paginate through all owned objects
        loop {
            match client
                .read_api()
                .get_owned_objects(
                    address,
                    Some(sui_sdk::rpc_types::SuiObjectResponseQuery::new(
                        None,
                        Some(
                            SuiObjectDataOptions::new()
                                .with_type()
                                .with_owner()
                                .with_bcs(),
                        ),
                    )),
                    cursor,
                    None, // Use default limit
                )
                .await
            {
                Ok(page) => {
                    for obj_info in &page.data {
                        if let Some(obj_data) = &obj_info.data {
                            all_object_ids.push(obj_data.object_id);
                        }
                    }

                    // Check if there are more pages
                    if page.has_next_page {
                        cursor = page.next_cursor;
                    } else {
                        break;
                    }
                }
                Err(e) => {
                    return Err(anyhow!("Failed to fetch owned objects: {}", e));
                }
            }
        }

        Ok(all_object_ids)
    }
}
