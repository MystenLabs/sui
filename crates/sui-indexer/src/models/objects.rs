// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, str::FromStr};

use diesel::deserialize::FromSql;
use diesel::pg::{Pg, PgValue};
use diesel::prelude::*;
use diesel::serialize::{Output, ToSql, WriteTuple};
use diesel::sql_types::{Bytea, Nullable, Record, VarChar};
use diesel::SqlType;
use diesel_derive_enum::DbEnum;
use fastcrypto::encoding::{Base64, Encoding};
use serde::{Deserialize, Serialize};
use serde_json;

use move_bytecode_utils::module_cache::GetModule;
use sui_json_rpc_types::{SuiObjectData, SuiObjectRef, SuiRawData};
use sui_types::base_types::{ObjectID, ObjectRef, ObjectType, SequenceNumber, SuiAddress};
use sui_types::digests::TransactionDigest;
use sui_types::move_package::MovePackage;
use sui_types::object::{Data, MoveObject, ObjectFormatOptions, ObjectRead, Owner};

use crate::errors::IndexerError;
use crate::models::owners::OwnerType;
use crate::schema::objects;
use crate::schema::sql_types::BcsBytes;

const OBJECT: &str = "object";

// NOTE: please add updating statement like below in pg_indexer_store.rs,
// if new columns are added here:
// objects::epoch.eq(excluded(objects::epoch))
#[derive(Queryable, Insertable, Debug, Identifiable, Clone, QueryableByName)]
#[diesel(table_name = objects, primary_key(object_id))]
pub struct Object {
    // epoch id in which this object got update.
    pub epoch: i64,
    // checkpoint seq number in which this object got updated,
    // it can be temp -1 for object updates from fast path,
    // it will be updated to the real checkpoint seq number in the following checkpoint.
    pub checkpoint: i64,
    pub object_id: String,
    pub version: i64,
    pub object_digest: String,
    pub owner_type: OwnerType,
    pub owner_address: Option<String>,
    pub initial_shared_version: Option<i64>,
    pub previous_transaction: String,
    pub object_type: String,
    pub object_status: ObjectStatus,
    pub has_public_transfer: bool,
    pub storage_rebate: i64,
    pub bcs: Vec<NamedBcsBytes>,
}
#[derive(SqlType, Debug, Clone)]
#[diesel(sql_type = crate::schema::sql_types::BcsBytes)]
pub struct NamedBcsBytes(pub String, pub Vec<u8>);

impl ToSql<Nullable<BcsBytes>, Pg> for NamedBcsBytes {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> diesel::serialize::Result {
        WriteTuple::<(VarChar, Bytea)>::write_tuple(&(self.0.clone(), self.1.clone()), out)
    }
}

impl FromSql<Nullable<BcsBytes>, Pg> for NamedBcsBytes {
    fn from_sql(bytes: PgValue) -> diesel::deserialize::Result<Self> {
        let (name, data) = FromSql::<Record<(VarChar, Bytea)>, Pg>::from_sql(bytes)?;
        Ok(NamedBcsBytes(name, data))
    }
}

#[derive(Debug, Clone)]
pub struct DeletedObject {
    // epoch id in which this object got deleted.
    pub epoch: i64,
    // checkpoint seq number in which this object got deleted.
    pub checkpoint: Option<i64>,
    pub object_id: String,
    pub version: i64,
    pub object_digest: String,
    pub owner_type: OwnerType,
    pub previous_transaction: String,
    pub object_type: String,
    pub object_status: ObjectStatus,
    pub has_public_transfer: bool,
}

impl From<DeletedObject> for Object {
    fn from(o: DeletedObject) -> Self {
        Object {
            epoch: o.epoch,
            // NOTE: -1 as temp checkpoint for object updates from fast path,
            checkpoint: o.checkpoint.unwrap_or(-1),
            object_id: o.object_id,
            version: o.version,
            object_digest: o.object_digest,
            owner_type: o.owner_type,
            owner_address: None,
            initial_shared_version: None,
            previous_transaction: o.previous_transaction,
            object_type: o.object_type,
            object_status: o.object_status,
            has_public_transfer: o.has_public_transfer,
            storage_rebate: 0,
            bcs: vec![],
        }
    }
}

#[derive(DbEnum, Debug, Clone, Copy, Deserialize, Serialize)]
#[ExistingTypePath = "crate::schema::sql_types::ObjectStatus"]
#[serde(rename_all = "snake_case")]
pub enum ObjectStatus {
    Created,
    Mutated,
    Deleted,
    Wrapped,
    Unwrapped,
    UnwrappedThenDeleted,
}

impl Object {
    pub fn from(
        epoch: u64,
        checkpoint: Option<u64>,
        status: &ObjectStatus,
        o: &SuiObjectData,
    ) -> Self {
        let (owner_type, owner_address, initial_shared_version) =
            owner_to_owner_info(&o.owner.expect("Expect the owner type to be non-empty"));

        let (has_public_transfer, bcs) =
            match o.bcs.clone().expect("Expect BCS data to be non-empty") {
                SuiRawData::MoveObject(o) => (
                    o.has_public_transfer,
                    vec![NamedBcsBytes(OBJECT.to_string(), o.bcs_bytes)],
                ),
                SuiRawData::Package(p) => (
                    false,
                    p.module_map
                        .into_iter()
                        .map(|(k, v)| NamedBcsBytes(k, v))
                        .collect(),
                ),
            };

        Object {
            epoch: epoch as i64,
            // NOTE: -1 as temp checkpoint for object updates from fast path,
            checkpoint: checkpoint.map(|v| v as i64).unwrap_or(-1),
            object_id: o.object_id.to_string(),
            version: o.version.value() as i64,
            object_digest: o.digest.base58_encode(),
            owner_type,
            owner_address,
            initial_shared_version,
            previous_transaction: o
                .previous_transaction
                .expect("Expect previous transaction to be non-empty")
                .base58_encode(),
            object_type: o
                .type_
                .as_ref()
                .expect("Expect the object type to be non-empty")
                .to_string(),
            object_status: *status,
            has_public_transfer,
            storage_rebate: o.storage_rebate.unwrap_or_default() as i64,
            bcs,
        }
    }

    pub fn try_into_object_read(
        self,
        module_cache: &impl GetModule,
    ) -> Result<ObjectRead, IndexerError> {
        Ok(match self.object_status {
            ObjectStatus::Deleted | ObjectStatus::UnwrappedThenDeleted => {
                ObjectRead::Deleted(self.get_object_ref()?)
            }
            _ => {
                let oref = self.get_object_ref()?;
                let object: sui_types::object::Object = self.try_into()?;
                let layout = object.get_layout(ObjectFormatOptions::default(), module_cache)?;
                ObjectRead::Exists(oref, object, layout)
            }
        })
    }

    pub fn get_object_ref(&self) -> Result<ObjectRef, IndexerError> {
        let object_id = self.object_id.parse()?;
        let digest = self.object_digest.parse().map_err(|e| {
            IndexerError::SerdeError(format!(
                "Failed to parse object digest: {}, error: {}",
                self.object_digest, e
            ))
        })?;
        Ok((object_id, (self.version as u64).into(), digest))
    }

    // MUSTFIX(gegaowp): trim data to reduce short-term storage consumption.
    pub fn trim_data(&mut self) {
        self.bcs.clear();
    }
}

impl TryFrom<Object> for sui_types::object::Object {
    type Error = IndexerError;

    fn try_from(o: Object) -> Result<Self, Self::Error> {
        let object_type = ObjectType::from_str(&o.object_type)?;
        let object_id = ObjectID::from_str(&o.object_id)?;
        let version = SequenceNumber::from_u64(o.version as u64);
        let owner = match o.owner_type {
            OwnerType::AddressOwner => Owner::AddressOwner(SuiAddress::from_str(
                &o.owner_address.expect("Owner address should not be empty."),
            )?),
            OwnerType::ObjectOwner => Owner::ObjectOwner(SuiAddress::from_str(
                &o.owner_address.expect("Owner address should not be empty."),
            )?),
            OwnerType::Shared => Owner::Shared {
                initial_shared_version: SequenceNumber::from_u64(
                    o.initial_shared_version
                        .expect("Shared version should not be empty.") as u64,
                ),
            },
            OwnerType::Immutable => Owner::Immutable,
        };
        let previous_transaction = TransactionDigest::from_str(&o.previous_transaction)?;

        Ok(match object_type {
            ObjectType::Package => {
                let modules = o
                    .bcs
                    .into_iter()
                    .map(|NamedBcsBytes(name, bytes)| (name, bytes))
                    .collect();
                // Ok to unwrap, package size is safe guarded by the full node, we are not limiting size when reading back from DB.
                let package = MovePackage::new(
                    object_id,
                    version,
                    modules,
                    u64::MAX,
                    // TODO: these represent internal data needed for Move code execution and as
                    // long as this MovePackage does not find its way to the Move adapter (which is
                    // the assumption here) they can remain uninitialized though we could consider
                    // storing them in the database and properly initializing here for completeness
                    Vec::new(),
                    BTreeMap::new(),
                )
                .unwrap();
                sui_types::object::Object {
                    data: Data::Package(package),
                    owner,
                    previous_transaction,
                    storage_rebate: o.storage_rebate as u64,
                }
            }
            // Reconstructing MoveObject form database table, move VM safety concern is irrelevant here.
            ObjectType::Struct(object_type) => unsafe {
                let content = o
                    .bcs
                    .first()
                    .expect("BCS content should not be empty")
                    .1
                    .clone();
                // Ok to unwrap, object size is safe guarded by the full node, we are not limiting size when reading back from DB.
                let object = MoveObject::new_from_execution_with_limit(
                    object_type,
                    o.has_public_transfer,
                    version,
                    content,
                    u64::MAX,
                )
                .unwrap();

                sui_types::object::Object {
                    data: Data::Move(object),
                    owner,
                    previous_transaction,
                    storage_rebate: o.storage_rebate as u64,
                }
            },
        })
    }
}

impl DeletedObject {
    pub fn from(
        epoch: u64,
        checkpoint: Option<u64>,
        oref: &SuiObjectRef,
        previous_tx: &TransactionDigest,
        status: &ObjectStatus,
    ) -> Self {
        Self {
            epoch: epoch as i64,
            checkpoint: checkpoint.map(|c| c as i64),
            object_id: oref.object_id.to_string(),
            version: oref.version.value() as i64,
            // DeleteObject is use for upsert only, this value will not be inserted into the DB
            // this dummy value is use to satisfy non null constrain.
            object_digest: "DELETED".to_string(),
            owner_type: OwnerType::AddressOwner,
            previous_transaction: previous_tx.base58_encode(),
            object_type: "DELETED".to_string(),
            object_status: *status,
            has_public_transfer: false,
        }
    }
}

// return owner_type, owner_address and initial_shared_version
pub fn owner_to_owner_info(owner: &Owner) -> (OwnerType, Option<String>, Option<i64>) {
    match owner {
        Owner::AddressOwner(address) => (OwnerType::AddressOwner, Some(address.to_string()), None),
        Owner::ObjectOwner(address) => (OwnerType::ObjectOwner, Some(address.to_string()), None),
        Owner::Shared {
            initial_shared_version,
        } => (
            OwnerType::Shared,
            None,
            Some(initial_shared_version.value() as i64),
        ),
        Owner::Immutable => (OwnerType::Immutable, None, None),
    }
}

pub fn compose_object_bulk_insert_update_query(objects: &[Object]) -> String {
    let insert_query = compose_object_bulk_insert_query(objects)
        .as_str()
        .trim_matches(';')
        .to_string();
    let insert_update_query = format!(
        "{} ON CONFLICT (object_id) 
        DO UPDATE SET 
            epoch = EXCLUDED.epoch,
            checkpoint = EXCLUDED.checkpoint,
            version = EXCLUDED.version,
            object_digest = EXCLUDED.object_digest,
            owner_type = EXCLUDED.owner_type,
            owner_address = EXCLUDED.owner_address,
            initial_shared_version = EXCLUDED.initial_shared_version,
            previous_transaction = EXCLUDED.previous_transaction,
            object_type = EXCLUDED.object_type,
            object_status = EXCLUDED.object_status,
            has_public_transfer = EXCLUDED.has_public_transfer,
            storage_rebate = EXCLUDED.storage_rebate,
            bcs = EXCLUDED.bcs;",
        insert_query
    );
    insert_update_query
}

pub fn compose_object_bulk_insert_query(objects: &[Object]) -> String {
    // Construct an array of rows to insert into the `objects` table
    let rows = objects
        .iter()
        .map(|obj| {
            let bcs_rows = obj
                .bcs
                .iter()
                .map(|bcs| (bcs.0.clone(), bcs.1.clone()))
                .collect::<Vec<_>>();
            let owner_type_str = serde_json::to_string(&obj.owner_type)
                .unwrap()
                .trim_matches('"')
                .to_string();
            let object_status_str = serde_json::to_string(&obj.object_status)
                .unwrap()
                .trim_matches('"')
                .to_string();
            (
                obj.epoch,
                obj.checkpoint,
                obj.object_id.to_string(),
                obj.version,
                obj.object_digest.clone(),
                owner_type_str,
                obj.owner_address.clone(),
                obj.initial_shared_version,
                obj.previous_transaction.clone(),
                obj.object_type.clone(),
                object_status_str,
                obj.has_public_transfer,
                obj.storage_rebate,
                bcs_rows,
            )
        })
        .collect::<Vec<_>>();

    let rows_query = rows
        .iter()
        .map(|row| {
            let (epoch, checkpoint, object_id, version, object_digest, owner_type, owner_address, initial_shared_version, previous_transaction, object_type, object_status, has_public_transfer, storage_rebate, bcs_rows) = row;

            let bcs_rows_query = bcs_rows
                .iter()
                .map(|bcs_row| {
                    let (bcs_key, bcs_value) = bcs_row;
                    let bytea_str = format!("decode('{}', 'base64')", Base64::encode(bcs_value));
                    format!(
                        "ROW('{}', {})",
                        bcs_key,
                        bytea_str
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "ROW({}::BIGINT, {}::BIGINT, '{}'::address, {}::BIGINT, '{}'::base58digest, '{}'::owner_type, 
                     '{}'::address, {}::BIGINT, '{}'::base58digest, '{}'::VARCHAR, '{}'::object_status,
                     {}::BOOLEAN, {}::BIGINT, ARRAY[{}]::bcs_bytes[])",
                epoch,
                checkpoint,
                object_id,
                version,
                object_digest,
                &owner_type,
                owner_address
                    .as_ref()
                    .map(|addr| addr.to_string())
                    .unwrap_or_else(|| "".to_string()),
                initial_shared_version
                    .as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "NULL".to_string()),
                previous_transaction,
                object_type,
                &object_status,
                has_public_transfer,
                storage_rebate,
                bcs_rows_query,
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    // Construct a prepared statement with placeholders for each row element
    let bulk_insert_query = format!(
        "INSERT INTO objects
            (epoch, checkpoint, object_id, version, object_digest, owner_type, owner_address, initial_shared_version, previous_transaction, object_type, object_status, has_public_transfer, storage_rebate, bcs)
        SELECT (unnest_arr).*
        FROM unnest(ARRAY[{}]::record[]) 
        AS unnest_arr(epoch BIGINT, checkpoint BIGINT, object_id address, version BIGINT, object_digest base58digest, owner_type owner_type, owner_address address, initial_shared_version BIGINT, previous_transaction base58digest, object_type VARCHAR, object_status object_status, has_public_transfer BOOLEAN, storage_rebate BIGINT, bcs bcs_bytes[]);",
        rows_query
    );
    bulk_insert_query
}

pub fn group_and_sort_objects(objects: Vec<Object>) -> Vec<Vec<Object>> {
    let mut objects_sorted = objects;
    objects_sorted.sort_by(|a, b| a.object_id.cmp(&b.object_id));
    // Group objects by object_id
    let mut groups: Vec<Vec<Object>> = vec![];
    let mut current_group: Vec<Object> = vec![];
    let mut current_object_id = String::new();
    for object in objects_sorted {
        if object.object_id != current_object_id {
            if !current_group.is_empty() {
                // Sort the group by version, in a reverse order to be popped later
                current_group.sort_by(|a, b| b.version.cmp(&a.version));
                groups.push(current_group);
            }
            current_group = vec![];
            current_object_id = object.object_id.clone();
        }
        current_group.push(object);
    }
    // Sort the last group by version, in a reverse order to be popped later
    if !current_group.is_empty() {
        current_group.sort_by(|a, b| b.version.cmp(&a.version));
        groups.push(current_group);
    }
    groups
}
