// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::models::owners::OwnerType;
use crate::schema::objects;
use crate::schema::sql_types::BcsBytes;
use diesel::deserialize::FromSql;
use diesel::pg::{Pg, PgValue};
use diesel::prelude::*;
use diesel::serialize::{Output, ToSql, WriteTuple};
use diesel::sql_types::{Bytea, Nullable, Record, VarChar};
use diesel::SqlType;
use diesel_derive_enum::DbEnum;
use sui_json_rpc_types::{SuiObjectData, SuiObjectRef, SuiRawData};
use sui_types::base_types::EpochId;
use sui_types::digests::TransactionDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Owner;
const OBJECT: &str = "object";

#[derive(Queryable, Insertable, Debug, Identifiable, Clone)]
#[diesel(table_name = objects, primary_key(object_id))]
pub struct Object {
    // epoch id in which this object got update.
    pub epoch: i64,
    // checkpoint seq number in which this object got update.
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

impl FromSql<BcsBytes, Pg> for NamedBcsBytes {
    fn from_sql(bytes: PgValue) -> diesel::deserialize::Result<Self> {
        let (name, data) = FromSql::<Record<(VarChar, Bytea)>, Pg>::from_sql(bytes)?;
        Ok(NamedBcsBytes(name, data))
    }
}

#[derive(Insertable, Debug, Identifiable, Clone)]
#[diesel(table_name = objects, primary_key(object_id))]
pub struct DeletedObject {
    // epoch id in which this object got deleted.
    pub epoch: i64,
    // checkpoint seq number in which this object got deleted.
    pub checkpoint: i64,
    pub object_id: String,
    pub version: i64,
    pub object_digest: String,
    pub owner_type: OwnerType,
    pub previous_transaction: String,
    pub object_type: String,
    pub object_status: ObjectStatus,
}

#[derive(DbEnum, Debug, Clone, Copy)]
#[ExistingTypePath = "crate::schema::sql_types::ObjectStatus"]
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
        epoch: &EpochId,
        checkpoint: &CheckpointSequenceNumber,
        status: &ObjectStatus,
        o: &SuiObjectData,
    ) -> Self {
        let (owner_type, owner_address, initial_shared_version) =
            owner_to_owner_info(&o.owner.expect("Expect the owner type to be non-empty"));

        let bcs = match o.bcs.clone().expect("Expect BCS data to be non-empty") {
            SuiRawData::MoveObject(o) => vec![NamedBcsBytes(OBJECT.to_string(), o.bcs_bytes)],
            SuiRawData::Package(p) => p
                .module_map
                .into_iter()
                .map(|(k, v)| NamedBcsBytes(k, v))
                .collect(),
        };

        Object {
            epoch: *epoch as i64,
            checkpoint: *checkpoint as i64,
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
            bcs,
        }
    }
}

impl DeletedObject {
    pub fn from(
        epoch: &EpochId,
        checkpoint: &CheckpointSequenceNumber,
        oref: &SuiObjectRef,
        previous_tx: &TransactionDigest,
        status: ObjectStatus,
    ) -> Self {
        Self {
            epoch: *epoch as i64,
            checkpoint: *checkpoint as i64,
            object_id: oref.object_id.to_string(),
            version: oref.version.value() as i64,
            // DeleteObject is use for upsert only, this value will not be inserted into the DB
            // this dummy value is use to satisfy non null constrain.
            object_digest: "DELETED".to_string(),
            owner_type: OwnerType::AddressOwner,
            previous_transaction: previous_tx.base58_encode(),
            object_type: "DELETED".to_string(),
            object_status: status,
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
