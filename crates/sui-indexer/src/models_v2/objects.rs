// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use sui_types::digests::ObjectDigest;

use move_bytecode_utils::module_cache::GetModule;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber};
use sui_types::dynamic_field::{DynamicFieldInfo, DynamicFieldType};
use sui_types::object::Object;
use sui_types::object::{ObjectFormatOptions, ObjectRead};

use crate::errors::IndexerError;
use crate::schema_v2::objects;
use crate::types_v2::IndexedObject;

// NOTE: please add updating statement like below in pg_indexer_store_v2.rs,
// if new columns are added here:
// objects::epoch.eq(excluded(objects::epoch))
#[derive(Queryable, Insertable, Debug, Identifiable, Clone, QueryableByName)]
#[diesel(table_name = objects, primary_key(object_id))]
pub struct StoredObject {
    pub object_id: Vec<u8>,
    pub object_version: i64,
    pub object_digest: Vec<u8>,
    pub checkpoint_sequence_number: i64,
    pub owner_type: i16,
    pub owner_id: Option<Vec<u8>>,
    pub serialized_object: Vec<u8>,
    pub coin_type: Option<String>,
    // TODO deal with overflow
    pub coin_balance: Option<i64>,
    pub df_kind: Option<i16>,
    pub df_name: Option<Vec<u8>>,
    pub df_object_type: Option<String>,
    pub df_object_id: Option<Vec<u8>>,
}

#[derive(Queryable, Insertable, Debug, Identifiable, Clone, QueryableByName)]
#[diesel(table_name = objects, primary_key(object_id))]
pub struct StoredDeletedObject {
    pub object_id: Vec<u8>,
}

impl From<IndexedObject> for StoredObject {
    fn from(o: IndexedObject) -> Self {
        Self {
            object_id: o.object_id.to_vec(),
            object_version: o.object_version as i64,
            object_digest: o.object_digest.into_inner().to_vec(),
            checkpoint_sequence_number: o.checkpoint_sequence_number as i64,
            owner_type: o.owner_type as i16,
            owner_id: o.owner_id.map(|id| id.to_vec()),
            serialized_object: bcs::to_bytes(&o.object).unwrap(),
            coin_type: o.coin_type,
            coin_balance: o.coin_balance.map(|b| b as i64),
            df_kind: o.df_info.as_ref().map(|k| match k.type_ {
                DynamicFieldType::DynamicField => 0,
                DynamicFieldType::DynamicObject => 1,
            }),
            df_name: o.df_info.as_ref().map(|n| bcs::to_bytes(&n.name).unwrap()),
            df_object_type: o.df_info.as_ref().map(|v| v.object_type.clone()),
            df_object_id: o.df_info.as_ref().map(|v| v.object_id.to_vec()),
        }
    }
}

impl TryFrom<StoredObject> for Object {
    type Error = IndexerError;

    fn try_from(o: StoredObject) -> Result<Self, Self::Error> {
        bcs::from_bytes(&o.serialized_object).map_err(|e| {
            IndexerError::SerdeError(format!(
                "Failed to deserialize object: {:?}, error: {}",
                o.object_id, e
            ))
        })
    }
}

impl StoredObject {
    pub fn try_into_object_read(
        self,
        module_cache: &impl GetModule,
    ) -> Result<ObjectRead, IndexerError> {
        let oref = self.get_object_ref()?;
        let object: sui_types::object::Object = self.try_into()?;
        let layout = object.get_layout(ObjectFormatOptions::default(), module_cache)?;
        Ok(ObjectRead::Exists(oref, object, layout))
    }

    pub fn try_into_dynamic_field_info(self) -> Result<Option<DynamicFieldInfo>, IndexerError> {
        if self.df_kind.is_none() {
            return Ok(None);
        }

        // Past this point, if there is any unexpected field, it's a data corruption error
        let object_id = ObjectID::from_bytes(&self.object_id).map_err(|_| {
            IndexerError::PersistentStorageDataCorruptionError(format!(
                "Can't convert {:?} to object_id",
                self.object_id
            ))
        })?;
        let object_digest = ObjectDigest::try_from(self.object_digest.as_slice()).map_err(|e| {
            IndexerError::PersistentStorageDataCorruptionError(format!(
                "object {} has incompatible object digest. Error: {e}",
                object_id
            ))
        })?;
        let df_object_id = if let Some(df_object_id) = self.df_object_id {
            ObjectID::from_bytes(df_object_id).map_err(|e| {
                IndexerError::PersistentStorageDataCorruptionError(format!(
                    "object {} has incompatible dynamic field type: df_object_id. Error: {e}",
                    object_id
                ))
            })
        } else {
            return Err(IndexerError::PersistentStorageDataCorruptionError(format!(
                "object {} has incompatible dynamic field type: empty df_object_id",
                object_id
            )));
        }?;
        let type_ = match self.df_kind {
            Some(0) => DynamicFieldType::DynamicField,
            Some(1) => DynamicFieldType::DynamicObject,
            _ => {
                return Err(IndexerError::PersistentStorageDataCorruptionError(format!(
                    "object {} has incompatible dynamic field type: empty df_kind",
                    object_id
                )))
            }
        };
        let (name, bcs_name) = if let Some(bcs_name) = self.df_name {
            let name = bcs::from_bytes(&bcs_name).map_err(|e| {
                IndexerError::PersistentStorageDataCorruptionError(format!(
                    "object {} has incompatible dynamic field type: df_name. Error: {e}",
                    object_id
                ))
            })?;
            Ok::<_, IndexerError>((name, bcs_name))
        } else {
            return Err(IndexerError::PersistentStorageDataCorruptionError(format!(
                "object {} has incompatible dynamic field type: empty df_name",
                object_id
            )));
        }?;
        let object_type =
            self.df_object_type
                .ok_or(IndexerError::PersistentStorageDataCorruptionError(format!(
                    "object {} has incompatible dynamic field type: empty df_object_type",
                    object_id
                )))?;
        Ok(Some(DynamicFieldInfo {
            version: SequenceNumber::from_u64(self.object_version as u64),
            digest: object_digest,
            type_,
            name,
            bcs_name: bcs_name.to_vec(),
            object_type,
            object_id: df_object_id,
        }))
    }

    pub fn get_object_ref(&self) -> Result<ObjectRef, IndexerError> {
        let object_id = ObjectID::from_bytes(self.object_id.clone()).map_err(|_| {
            IndexerError::SerdeError(format!("Can't convert {:?} to object_id", self.object_id))
        })?;
        let object_digest =
            ObjectDigest::try_from(self.object_digest.as_slice()).map_err(|_| {
                IndexerError::SerdeError(format!(
                    "Can't convert {:?} to object_digest",
                    self.object_digest
                ))
            })?;
        Ok((
            object_id,
            (self.object_version as u64).into(),
            object_digest,
        ))
    }
}
