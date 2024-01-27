// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, str::FromStr};

use sui_json_rpc::name_service::{Domain, NameRecord, SubDomainRegistration};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    dynamic_field::Field,
    full_checkpoint_content::CheckpointData,
    object::Object,
};

use crate::models::VerifiedDomain;

/// TODO(manos): Hardcode mainnet addresses.
const REGISTRY_TABLE_ID: &str =
    "0xe64cd9db9f829c6cc405d9790bd71567ae07259855f4fba6f02c84f52298c106";
const SUBDOMAIN_REGISTRATION_TYPE: &str = "0xe64cd9db9f829c6cc405d9790bd71567ae07259855f4fba6f02c84f52298c106::subdomain_registration::SubDomainRegistration";

#[derive(Debug, Clone)]
pub struct NameRecordChange {
    // the NameRecord entry in the table (DF).
    field: Field<Domain, NameRecord>,
    // the DF's ID.
    field_id: ObjectID,
}

pub struct SuinsIndexer {
    registry_table_id: String,
    subdomain_wrapper_type: String,
}

impl std::default::Default for SuinsIndexer {
    fn default() -> Self {
        Self::new(
            REGISTRY_TABLE_ID.to_owned(),
            SUBDOMAIN_REGISTRATION_TYPE.to_owned(),
        )
    }
}

impl SuinsIndexer {
    /// Create a new config by passing the table ID + subdomain wrapper type.
    pub fn new(registry_table_id: String, subdomain_wrapper_type: String) -> Self {
        Self {
            registry_table_id,
            subdomain_wrapper_type,
        }
    }

    /// Checks if the object referenced is a subdomain wrapper.
    /// For subdomain wrappers, we're saving the id of the wrapper in the name record,
    /// to make it easy to locate the NFT (since it is wrapped).
    pub fn is_subdomain_wrapper(&self, object: &Object) -> bool {
        object.struct_tag().is_some()
            && object.struct_tag().unwrap().to_canonical_string(true) == self.subdomain_wrapper_type
    }

    // Filter by owner.
    // Owner has to be the TABLE of the registry.
    // A table of that type can only have `Field<Domain,NameRecord> as a child so that check is enough to
    // make sure we're dealing with a registry change.
    pub fn is_name_record(&self, object: &Object) -> bool {
        object.get_single_owner().is_some()
            && object.get_single_owner()
                == Some(SuiAddress::from_str(&self.registry_table_id).unwrap())
    }

    /// Parses the name record changes + subdomain wraps.
    /// and immediately writes them into the supplied vector + hashmap.
    ///
    /// It is done in a way to allow a single iteration over the objects.
    pub fn parse_name_record_changes(
        &self,
        objects: &Vec<&Object>,
        name_record_changes: &mut Vec<NameRecordChange>,
        sub_domain_wrappers: &mut HashMap<String, String>,
    ) {
        for &x in objects {
            // Parse all the changes to name
            if self.is_name_record(x) {
                let name_record: Field<Domain, NameRecord> = x
                    .to_rust()
                    .unwrap_or_else(|| panic!("Failed to parse name record for {:?}", x));
                name_record_changes.push(NameRecordChange {
                    field: name_record,
                    field_id: x.id(),
                });
            }
            // Parse subdomain wrappers and save them in our hashmap.
            // Later, we'll save the id of the wrapper in the name record.
            // NameRecords & their equivalent SubdomainWrappers are always created in the same PTB, so we can safely assumy
            // that the wrapper will be created on the same checkpoint as the name record and vice versa.
            if self.is_subdomain_wrapper(x) {
                let sub_domain: SubDomainRegistration = x.to_rust().unwrap();
                sub_domain_wrappers.insert(
                    sub_domain.nft.domain_name,
                    sub_domain.id.id.bytes.to_string(),
                );
            };
        }
    }

    // For each input object, we're parsing the name record deletions
    // A deletion is derived on the fact that the object is deleted
    // and the type is of NameRecord type.
    pub fn parse_name_record_deletions(
        &self,
        checkpoint: &CheckpointData,
        removals: &mut Vec<String>,
    ) {
        // Gather all object ids that got deleted.
        // This way, we can delete removed name records
        // (detects burning of expired names or leaf names removal).
        let deleted_objects: Vec<_> = checkpoint
            .transactions
            .iter()
            .flat_map(|x| x.effects.all_removed_objects())
            .map(|((id, _, _), _)| id)
            .collect();

        for x in checkpoint.input_objects() {
            if self.is_name_record(x) && deleted_objects.contains(&x.id()) {
                removals.push(x.id().to_string());
            }
        }
    }

    pub fn process_checkpoint(
        &self,
        checkpoint: CheckpointData,
    ) -> (Vec<VerifiedDomain>, Vec<String>) {
        let mut name_records: Vec<NameRecordChange> = vec![];
        let mut subdomain_wrappers: HashMap<String, String> = HashMap::new();
        let mut removals: Vec<String> = vec![];

        self.parse_name_record_changes(
            &checkpoint.output_objects(),
            &mut name_records,
            &mut subdomain_wrappers,
        );

        self.parse_name_record_deletions(&checkpoint, &mut removals);

        let db_updates = prepare_db_updates(
            &name_records,
            &subdomain_wrappers,
            checkpoint.checkpoint_summary.sequence_number,
        );

        (db_updates, removals)
    }
}

/// Allows us to format a SuiNS specific query for updating the DB entries
/// only if the checkpoint is newer than the last checkpoint we have in the DB.
/// And only if the expiration timestamp is either the same or in the future.
/// Doing that, we do not care about the order of execution and we can use multiple workers.
pub fn format_update_field_query(field: &str) -> String {
    format!(
        "CASE WHEN excluded.last_checkpoint_updated > domains.last_checkpoint_updated THEN excluded.{field} ELSE domains.{field} END"
    )
}

/// Prepares a vector of `VerifiedDomain` to be inserted into the DB.
/// Subdomain wrappers are also required to map the wrapped NFT ID to the name record.
pub fn prepare_db_updates(
    name_record_changes: &[NameRecordChange],
    subdomain_wrappers: &HashMap<String, String>,
    checkpoint_seq_num: u64,
) -> Vec<VerifiedDomain> {
    let mut updates: Vec<VerifiedDomain> = vec![];

    for name_record_change in name_record_changes.iter() {
        let name_record = &name_record_change.field;

        let parent = name_record.name.parent().to_string();
        let nft_id = name_record.value.nft_id.bytes.to_string();

        updates.push(VerifiedDomain {
            field_id: name_record_change.field_id.to_string(),
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
            last_checkpoint_updated: checkpoint_seq_num as i64,
            subdomain_wrapper_id: subdomain_wrappers
                .get(&name_record.name.to_string())
                .cloned(),
        });
    }

    updates
}
