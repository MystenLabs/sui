// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use move_core_types::language_storage::StructTag;
use sui_name_service::{Domain, NameRecord, SubDomainRegistration};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    dynamic_field::Field,
    full_checkpoint_content::{CheckpointData, CheckpointTransaction},
    object::Object,
};

use crate::models::VerifiedDomain;

/// We default to mainnet for both.
const REGISTRY_TABLE_ID: &str =
    "0xe64cd9db9f829c6cc405d9790bd71567ae07259855f4fba6f02c84f52298c106";
const NAME_RECORD_TYPE: &str = "0x2::dynamic_field::Field<0xd22b24490e0bae52676651b4f56660a5ff8022a2576e0089f79b3c88d44e08f0::domain::Domain,0xd22b24490e0bae52676651b4f56660a5ff8022a2576e0089f79b3c88d44e08f0::name_record::NameRecord>";
const SUBDOMAIN_REGISTRATION_TYPE: &str =
    "0x00c2f85e07181b90c140b15c5ce27d863f93c4d9159d2a4e7bdaeb40e286d6f5::subdomain_registration::SubDomainRegistration";

#[derive(Debug, Clone)]
pub struct NameRecordChange(Field<Domain, NameRecord>);

pub struct SuinsIndexer {
    registry_table_id: SuiAddress,
    subdomain_wrapper_type: StructTag,
    name_record_type: StructTag,
}

impl std::default::Default for SuinsIndexer {
    fn default() -> Self {
        Self::new(
            REGISTRY_TABLE_ID.to_owned(),
            SUBDOMAIN_REGISTRATION_TYPE.to_owned(),
            NAME_RECORD_TYPE.to_owned(),
        )
    }
}

impl SuinsIndexer {
    /// Create a new config by passing the table ID + subdomain wrapper type.
    /// Useful for testing or custom environments.
    pub fn new(registry_address: String, wrapper_type: String, record_type: String) -> Self {
        let registry_table_id = SuiAddress::from_str(&registry_address).unwrap();
        let name_record_type = StructTag::from_str(&record_type).unwrap();
        let subdomain_wrapper_type = StructTag::from_str(&wrapper_type).unwrap();

        Self {
            registry_table_id,
            name_record_type,
            subdomain_wrapper_type,
        }
    }

    /// Checks if the object referenced is a subdomain wrapper.
    /// For subdomain wrappers, we're saving the ID of the wrapper object,
    /// to make it easy to locate the NFT (since the base NFT gets wrapped and indexing won't work there).
    pub fn is_subdomain_wrapper(&self, object: &Object) -> bool {
        object
            .struct_tag()
            .is_some_and(|tag| tag == self.subdomain_wrapper_type)
    }

    // Filter by the dynamic field value type.
    // A valid name record for an object has the type `Field<Domain,NameRecord>,
    // and the parent of it is the `registry` table id.
    pub fn is_name_record(&self, object: &Object) -> bool {
        object
            .get_single_owner()
            .is_some_and(|owner| owner == self.registry_table_id)
            && object
                .struct_tag()
                .is_some_and(|tag| tag == self.name_record_type)
    }

    /// Processes a checkpoint and produces a list of `updates` and a list of `removals`
    ///
    /// We can then use these to execute our DB bulk insertions + bulk deletions.
    ///
    /// Returns
    /// - `Vec<VerifiedDomain>`: A list of NameRecord updates for the database (including sequence number)
    /// - `Vec<String>`: A list of IDs to be deleted from the database (`field_id` is the matching column)
    pub fn process_checkpoint(&self, data: &CheckpointData) -> (Vec<VerifiedDomain>, Vec<String>) {
        let mut checkpoint = SuinsIndexerCheckpoint::new(data.checkpoint_summary.sequence_number);

        // loop through all the transactions in the checkpoint
        // Since the transactions are sequenced inside the checkpoint, we can safely assume
        // that we have the latest data for each name record in the end of the loop.
        for transaction in &data.transactions {
            // Add all name record changes to the name_records HashMap.
            // Remove any removals that got re-created.
            checkpoint.parse_record_changes(self, &transaction.output_objects);

            // Gather all removals from the transaction,
            // and delete any name records from the name_records if it got deleted.
            checkpoint.parse_record_deletions(self, transaction);
        }

        (
            // Convert our name_records & wrappers into a list of updates for the DB.
            checkpoint.prepare_db_updates(),
            checkpoint
                .removals
                .into_iter()
                .map(|id| id.to_string())
                .collect(),
        )
    }
}

pub struct SuinsIndexerCheckpoint {
    /// A list of name records that have been updated in the checkpoint.
    name_records: HashMap<ObjectID, NameRecordChange>,
    /// A list of subdomain wrappers that have been created in the checkpoint.
    subdomain_wrappers: HashMap<String, String>,
    /// A list of name records that have been deleted in the checkpoint.
    removals: HashSet<ObjectID>,
    /// The sequence number of the checkpoint.
    checkpoint_sequence_number: u64,
}

impl SuinsIndexerCheckpoint {
    pub fn new(checkpoint_sequence_number: u64) -> Self {
        Self {
            name_records: HashMap::new(),
            subdomain_wrappers: HashMap::new(),
            removals: HashSet::new(),
            checkpoint_sequence_number,
        }
    }

    /// Parses the name record changes + subdomain wraps.
    /// and pushes them into the supplied vector + hashmap.
    ///
    /// It is implemented in a way to do just a single iteration over the objects.
    pub fn parse_record_changes(&mut self, config: &SuinsIndexer, objects: &[Object]) {
        for object in objects {
            // Parse all the changes to a `NameRecord`
            if config.is_name_record(object) {
                let name_record: Field<Domain, NameRecord> = object
                    .to_rust()
                    .unwrap_or_else(|| panic!("Failed to parse name record for {:?}", object));

                let id = object.id();

                // Remove from the removals list if it's there.
                // The reason it might have been there is that the same name record might have been
                // deleted in a previous transaction in the same checkpoint, and now it got re-created.
                self.removals.remove(&id);

                self.name_records.insert(id, NameRecordChange(name_record));
            }
            // Parse subdomain wrappers and save them in our hashmap.
            // Later, we'll save the id of the wrapper in the name record.
            // NameRecords & their equivalent SubdomainWrappers are always created in the same PTB, so we can safely assume
            // that the wrapper will be created on the same checkpoint as the name record and vice versa.
            if config.is_subdomain_wrapper(object) {
                let sub_domain: SubDomainRegistration = object.to_rust().unwrap();
                self.subdomain_wrappers.insert(
                    sub_domain.nft.domain_name,
                    sub_domain.id.id.bytes.to_string(),
                );
            };
        }
    }

    /// Parses a list of the deletions in the checkpoint and adds them to the removals list.
    /// Also removes any name records from the updates, if they ended up being deleted in the same checkpoint.
    pub fn parse_record_deletions(
        &mut self,
        config: &SuinsIndexer,
        transaction: &CheckpointTransaction,
    ) {
        // a list of all the deleted objects in the transaction.
        let deleted_objects: HashSet<_> = transaction
            .effects
            .all_tombstones()
            .into_iter()
            .map(|(id, _)| id)
            .collect();

        for input in transaction.input_objects.iter() {
            if config.is_name_record(input) && deleted_objects.contains(&input.id()) {
                // since this record was deleted, we need to remove it from the name_records hashmap.
                // that catches a case where a name record was edited on a previous transaction in the checkpoint
                // and deleted from a different tx later in the checkpoint.
                self.name_records.remove(&input.id());

                // add it in the list of removals
                self.removals.insert(input.id());
            }
        }
    }

    /// Prepares a vector of `VerifiedDomain`s to be inserted into the DB, taking in account
    /// the list of subdomain wrappers created as well as the checkpoint's sequence number.
    pub fn prepare_db_updates(&self) -> Vec<VerifiedDomain> {
        let mut updates: Vec<VerifiedDomain> = vec![];

        for (field_id, name_record_change) in self.name_records.iter() {
            let name_record = &name_record_change.0;

            let parent = name_record.name.parent().to_string();
            let nft_id = name_record.value.nft_id.bytes.to_string();

            updates.push(VerifiedDomain {
                field_id: field_id.to_string(),
                name: name_record.name.to_string(),
                parent,
                expiration_timestamp_ms: name_record.value.expiration_timestamp_ms as i64,
                nft_id,
                target_address: if name_record.value.target_address.is_some() {
                    Some(SuiAddress::to_string(
                        &name_record.value.target_address.unwrap(),
                    ))
                } else {
                    None
                },
                // unwrapping must be safe as `value.data` is an on-chain value with VecMap<String,String> type.
                data: serde_json::to_value(&name_record.value.data).unwrap(),
                last_checkpoint_updated: self.checkpoint_sequence_number as i64,
                subdomain_wrapper_id: self
                    .subdomain_wrappers
                    .get(&name_record.name.to_string())
                    .cloned(),
            });
        }

        updates
    }
}

/// Allows us to format a SuiNS specific query for updating the DB entries
/// only if the checkpoint is newer than the last checkpoint we have in the DB.
/// Doing that, we do not care about the order of execution and we can use multiple threads
/// to commit from later checkpoints to the DB.
///
/// WARNING: This can easily be SQL-injected, so make sure to use it only with trusted inputs.
pub fn format_update_field_query(field: &str) -> String {
    format!(
        "CASE WHEN excluded.last_checkpoint_updated > domains.last_checkpoint_updated THEN excluded.{field} ELSE domains.{field} END"
    )
}

/// Update the subdomain wrapper ID only if it is part of the checkpoint.
pub fn format_update_subdomain_wrapper_query() -> String {
    "CASE WHEN excluded.subdomain_wrapper_id IS NOT NULL THEN excluded.subdomain_wrapper_id ELSE domains.subdomain_wrapper_id END".to_string()
}
