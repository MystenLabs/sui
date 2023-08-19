// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, HashMap},
    str::FromStr,
};

use diesel::pg::{Pg, PgValue};
use diesel::prelude::*;
use diesel::serialize::{Output, ToSql, WriteTuple};
use diesel::sql_types::{Bytea, Nullable, Record, VarChar};
use diesel::SqlType;
use diesel::{deserialize::FromSql, expression::is_aggregate::No};
use diesel_derive_enum::DbEnum;
use fastcrypto::encoding::{Base64, Encoding};
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::hash_map::Entry;

use move_bytecode_utils::module_cache::GetModule;
use sui_json_rpc_types::{SuiObjectData, SuiObjectRef, SuiRawData};
use sui_types::{digests::TransactionDigest, object::Object, dynamic_field::DynamicFieldInfo};
use sui_types::move_package::MovePackage;
use sui_types::object::{Data, MoveObject, ObjectFormatOptions, ObjectRead, Owner};
use sui_types::{
    base_types::{ObjectID, ObjectRef, ObjectType, SequenceNumber, SuiAddress},
    storage::WriteKind,
};

use crate::errors::IndexerError;
use crate::schema::objects;
// use crate::schema::sql_types::BcsBytes;

// const OBJECT: &str = "object";

#[derive(Debug, Copy, Clone)]
pub enum OwnerType {
    Immutable = 0,
    Address = 1,
    Object = 2,
    Shared = 3,
    Deleted = 4,
}

#[derive(Debug, Copy, Clone)]
pub enum ObjectStatus {
    NotExist = 0,
    Exists = 1,
    NotFound = 2,
}

// NOTE: please add updating statement like below in pg_indexer_store.rs,
// if new columns are added here:
// objects::epoch.eq(excluded(objects::epoch))
#[derive(Queryable, Insertable, Debug, Identifiable, Clone, QueryableByName)]
#[diesel(table_name = objects, primary_key(object_id))]
pub struct StoredObject {
    pub object_id: Vec<u8>,
    pub object_version: i64,
    pub checkpoint_sequence_number: i64,
    pub object_status: i16,
    pub owner_type: i16,
    pub owner_id: Option<Vec<u8>>,
    pub serialized_object: Vec<u8>,
    pub coin_type: Option<String>,
    // TODO hmmm overflow?
    pub coin_balance: Option<i64>,
    pub dynamic_field_name_type: Option<String>,
    pub dynamic_field_value: Option<String>,
    pub dynamic_field_type: Option<i16>,
}

#[derive(Debug)]
pub struct IndexedObject {
    pub object_id: ObjectID,
    pub object_version: u64,
    pub checkpoint_sequence_number: u64,
    pub object_status: ObjectStatus,
    pub owner_type: OwnerType,
    pub owner_id: Option<SuiAddress>,
    pub object: Object,
    pub coin_type: Option<String>,
    // TODO hmmm overflow?
    pub coin_balance: Option<u64>,
    pub df_info: Option<DynamicFieldInfo>,
    // pub dynamic_field_name_type: Option<String>,
    // pub dynamic_field_value: Option<String>,
    // pub dynamic_field_type: Option<DynamicFieldType>,
}


// #[derive(SqlType, Debug, Clone)]
// #[diesel(sql_type = crate::schema::sql_types::BcsBytes)]
// pub struct NamedBcsBytes(pub String, pub Vec<u8>);

// impl ToSql<Nullable<BcsBytes>, Pg> for NamedBcsBytes {
//     fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> diesel::serialize::Result {
//         WriteTuple::<(VarChar, Bytea)>::write_tuple(&(self.0.clone(), self.1.clone()), out)
//     }
// }

// impl FromSql<Nullable<BcsBytes>, Pg> for NamedBcsBytes {
//     fn from_sql(bytes: PgValue) -> diesel::deserialize::Result<Self> {
//         let (name, data) = FromSql::<Record<(VarChar, Bytea)>, Pg>::from_sql(bytes)?;
//         Ok(NamedBcsBytes(name, data))
//     }
// }

// #[derive(Debug, Clone)]
// pub struct DeletedObject {
//     // epoch id in which this object got deleted.
//     pub epoch: i64,
//     // checkpoint seq number in which this object got deleted.
//     pub checkpoint: Option<i64>,
//     pub object_id: String,
//     pub version: i64,
//     pub object_digest: String,
//     pub owner_type: OwnerType,
//     pub previous_transaction: String,
//     pub object_type: String,
//     pub object_status: ObjectStatus,
//     pub has_public_transfer: bool,
// }

// impl From<DeletedObject> for Object {
//     fn from(o: DeletedObject) -> Self {
//         Object {
//             epoch: o.epoch,
//             // NOTE: -1 as temp checkpoint for object updates from fast path,
//             checkpoint: o.checkpoint.unwrap_or(-1),
//             object_id: o.object_id,
//             version: o.version,
//             object_digest: o.object_digest,
//             owner_type: o.owner_type,
//             owner_address: None,
//             initial_shared_version: None,
//             previous_transaction: o.previous_transaction,
//             object_type: o.object_type,
//             object_status: o.object_status,
//             has_public_transfer: o.has_public_transfer,
//             storage_rebate: 0,
//             bcs: vec![],
//         }
//     }
// }

// #[derive(DbEnum, Debug, Clone, Copy, Deserialize, Serialize)]
// #[ExistingTypePath = "crate::schema::sql_types::ObjectStatus"]
// #[serde(rename_all = "snake_case")]
// pub enum ObjectStatus {
//     Created,
//     Mutated,
//     Deleted,
//     Wrapped,
//     Unwrapped,
//     UnwrappedThenDeleted,
// }

// impl From<WriteKind> for ObjectStatus {
//     fn from(value: WriteKind) -> Self {
//         match value {
//             WriteKind::Mutate => Self::Mutated,
//             WriteKind::Create => Self::Created,
//             WriteKind::Unwrap => Self::Unwrapped,
//         }
//     }
// }

impl IndexedObject {
    pub fn from_object(
        checkpoint_sequence_number: u64,
        object: Object,
        df_info: Option<DynamicFieldInfo>,
    ) -> Self {
        let (owner_type, owner_id) = owner_to_owner_info(&object.owner);
        let coin_type = object.coin_type_maybe().map(|t| t.to_string());
        let coin_balance = if coin_type.is_some() {
            Some(object.get_coin_value_unsafe())
        } else {
            None
        };

        Self {
            checkpoint_sequence_number,
            object_id: object.id(),
            object_version: object.version().value(),
            object_status: ObjectStatus::Exists,
            owner_type,
            owner_id,
            object,
            coin_type,
            coin_balance,
            df_info,
        }
    }
}
    // pub fn from(
    //     // epoch: u64,
    //     checkpoint: u64,
    //     // status: &ObjectStatus,
    //     o: &SuiObjectData,
    // ) -> Self {
    //     let object: sui_types::object::Object = o.try_into()
    //         .map_err(|e | IndexerError::DataTransformationError(format!("Can't convert SuiObjectData into Object: {e}")))?;
    //     let (owner_type, owner_id) = owner_to_owner_info(&object.owner);
    //     // let (has_public_transfer, bcs) =
    //     //     match o.bcs.clone().expect("Expect BCS data to be non-empty") {
    //     //         SuiRawData::MoveObject(o) => (
    //     //             o.has_public_transfer,
    //     //             vec![NamedBcsBytes(OBJECT.to_string(), o.bcs_bytes)],
    //     //         ),
    //     //         SuiRawData::Package(p) => (
    //     //             false,
    //     //             p.module_map
    //     //                 .into_iter()
    //     //                 .map(|(k, v)| NamedBcsBytes(k, v))
    //     //                 .collect(),
    //     //         ),
    //     //     };

    //     Self {
    //         checkpoint_sequence_number: checkpoint as i64,
    //         object_id: object.id().into_bytes(),
    //         object_version: object.version().value() as i64,
    //         owner_type,
    //         owner_id,
    //         serialized_object: bcs::to_bytes(&object).unwrap(),
    //         // FIXME
    //         coin_type: None,
    //         coin_balance: None,
    //         dynamic_field_name_type: None,
    //         dynamic_field_type: None,
    //         dynamic_field_value: None,

    //     }
    // }

impl StoredObject {
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
        let object_id = ObjectID::from_bytes(self.object_id).map_err(IndexerError::CorruptedData(
            format!("Can't convert {} to object_id", self.object_id),
        ));
        let digest = self.object_digest.parse().map_err(|e| {
            IndexerError::SerdeError(format!(
                "Failed to parse object digest: {}, error: {}",
                self.object_digest, e
            ))
        })?;
        Ok((object_id, (self.version as u64).into(), digest))
    }

    pub fn make_deleted(
        // epoch: u64,
        checkpoint: u64,
        oref: &SuiObjectRef,
        // previous_tx: &TransactionDigest,
        // status: &ObjectStatus,
    ) -> Self {
        Self {
            checkpoint_sequence_number: checkpoint as i64,
            object_id: oref.object_id.into_bytes(),
            object_version: oref.version.value() as i64,
            object_status: ObjectStatus::Deleted,
            owner_type: OwnerType::Deleted,
            owner_id: None,
            serialized_object: vec![],
            coin_type: None,
            coin_balance: None,
            dynamic_field_name_type: None,
            dynamic_field_type: None,
            dynamic_field_value: None,
            // DeleteObject is use for upsert only, this value will not be inserted into the DB
            // this dummy value is use to satisfy non null constrain.
            // object_digest: "DELETED".to_string(),
            // owner_type: OwnerType::AddressOwner,
            // previous_transaction: previous_tx.base58_encode(),
            // object_type: "DELETED".to_string(),
            // object_status: *status,
            // has_public_transfer: false,
        }
    }
}

impl TryFrom<Object> for sui_types::object::Object {
    type Error = IndexerError;

    fn try_from(o: Object) -> Result<Self, Self::Error> {
        o.try_into().map_err(|e| {
            IndexerError::DataTransformationError(format!(
                "Can't convert SuiObjectData into Object: {e}"
            ))
        })
    }
}

// return owner_type, owner_address
pub fn owner_to_owner_info(owner: &Owner) -> (OwnerType, Option<SuiAddress>) {
    match owner {
        Owner::AddressOwner(address) => (OwnerType::Address, Some(address)),
        Owner::ObjectOwner(address) => (OwnerType::ObjectOwner, Some(address)),
        Owner::Shared { .. } => (OwnerType::Shared, None),
        Owner::Immutable => (OwnerType::Immutable, None),
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

pub fn filter_latest_objects(objects: Vec<Object>) -> Vec<Object> {
    // Transactions in checkpoint are ordered by causal depedencies.
    // But HashMap is not a lot more costly than HashSet, and it
    // may be good to still keep the relative order of objects in
    // the checkpoint.
    let mut latest_objects = HashMap::new();
    for object in objects {
        match latest_objects.entry(object.object_id.clone()) {
            Entry::Vacant(e) => {
                e.insert(object);
            }
            Entry::Occupied(mut e) => {
                if object.version > e.get().version {
                    e.insert(object);
                }
            }
        }
    }
    latest_objects.into_values().collect()
}
