// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::objects;
use crate::schema::objects::dsl::{
    object_id as object_id_column, object_status as object_status_column, objects as objects_table,
    version as version_column,
};
use crate::PgPoolConnection;

use diesel::pg::upsert::excluded;
use diesel::prelude::*;
use diesel::result::Error;
use std::collections::BTreeMap;
use sui_json_rpc_types::SuiEvent;
use sui_types::object::Owner;

#[derive(Queryable, Debug, Identifiable)]
#[diesel(primary_key(object_id))]
pub struct Object {
    pub id: i64,
    pub object_id: String,
    pub version: i64,
    pub owner_type: String,
    pub owner_address: Option<String>,
    pub initial_shared_version: Option<i64>,
    pub package_id: String,
    pub transaction_module: String,
    pub object_type: Option<String>,
    pub object_status: String,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = objects)]
pub struct NewObject {
    pub object_id: String,
    pub version: i64,
    pub owner_type: String,
    pub owner_address: Option<String>,
    pub initial_shared_version: Option<i64>,
    pub package_id: String,
    pub transaction_module: String,
    pub object_type: Option<String>,
    pub object_status: String,
}

pub fn commit_new_objects(
    pg_pool_conn: &mut PgPoolConnection,
    new_objects: Vec<NewObject>,
) -> Result<usize, IndexerError> {
    if new_objects.is_empty() {
        return Ok(0);
    }
    let new_obj_commit_result = pg_pool_conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|conn| {
            diesel::insert_into(objects::table)
                .values(&new_objects)
                .on_conflict(object_id_column)
                .do_nothing()
                .execute(conn)
        });

    new_obj_commit_result.map_err(|e| {
        IndexerError::PostgresWriteError(format!(
            "Failed inserting new objects into database with objects {:?} and error: {:?}",
            new_objects, e
        ))
    })
}

pub fn commit_object_transfers(
    pg_pool_conn: &mut PgPoolConnection,
    object_transfers: Vec<NewObject>,
) -> Result<usize, IndexerError> {
    if object_transfers.is_empty() {
        return Ok(0);
    }

    let obj_transfer_commit_result = pg_pool_conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|conn| {
            diesel::insert_into(objects::table)
                .values(&object_transfers)
                .on_conflict(object_id_column)
                .do_nothing()
                .execute(conn)
        });

    obj_transfer_commit_result.map_err(|e| {
        IndexerError::PostgresWriteError(format!(
            "Failed inserting object transfers into database with objects {:?} and error: {:?}",
            object_transfers, e
        ))
    })
}

// Multi-row update is not supported by Diesel, see https://github.com/diesel-rs/diesel/discussions/2879
// even though Postgres can do it via UPDATE ... FROM.
// As a work-around, this function will read all related rows into memory, update the columns and upsert back to DB.
pub fn commit_object_mutations(
    pg_pool_conn: &mut PgPoolConnection,
    object_mutations: Vec<(String, (i64, String))>,
) -> Result<usize, IndexerError> {
    if object_mutations.is_empty() {
        return Ok(0);
    }
    let object_map = BTreeMap::from_iter(object_mutations.into_iter());
    let object_ids: Vec<String> = object_map.keys().cloned().collect();

    let obj_read_result: Result<Vec<Object>, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| {
        objects_table
            .filter(object_id_column.eq_any(object_ids))
            .load::<Object>(conn)
    });

    let objs = obj_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading selected objects from database for object mutations with error: {:?}",
            e
        ))
    })?;
    let updated_new_objs: Vec<NewObject> = objs
        .into_iter()
        .map(|obj| {
            let updates = object_map.get(&obj.object_id);
            let (mut version, mut status) = (obj.version, obj.object_status);
            if let Some((new_version, new_status)) = updates {
                version = *new_version;
                status = new_status.clone();
            }
            NewObject {
                object_id: obj.object_id,
                version,
                owner_type: obj.owner_type,
                owner_address: obj.owner_address,
                initial_shared_version: obj.initial_shared_version,
                package_id: obj.package_id,
                transaction_module: obj.transaction_module,
                object_type: obj.object_type,
                object_status: status,
            }
        })
        .collect();

    let obj_mutation_commit_result = pg_pool_conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|conn| {
            diesel::insert_into(objects::table)
                .values(&updated_new_objs)
                .on_conflict(object_id_column)
                .do_update()
                .set((
                    version_column.eq(excluded(version_column)),
                    object_status_column.eq(excluded(object_status_column)),
                ))
                .execute(conn)
        });

    obj_mutation_commit_result.map_err(|e| IndexerError::PostgresWriteError(
        format!("Failed writing object mutations to Postgres DB with mutations updated_new_objs {:?} and error: {:?} ", updated_new_objs, e) ))
}

pub fn commit_object_deletions(
    pg_pool_conn: &mut PgPoolConnection,
    object_deletions: Vec<String>,
) -> Result<usize, IndexerError> {
    if object_deletions.is_empty() {
        return Ok(0);
    }

    let obj_deletion_commit_result = pg_pool_conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|conn| {
            diesel::update(objects::table.filter(object_id_column.eq_any(object_deletions.clone())))
                .set(object_status_column.eq(DELETED_STATUS.to_string()))
                .execute(conn)
        });

    obj_deletion_commit_result.map_err(|e| {
        IndexerError::PostgresWriteError(format!(
            "Failed writing object deletions to Postgres DB with IDs {:?} and error: {:?} ",
            object_deletions, e
        ))
    })
}

// return owner_type, owner_address and initial_shared_version
fn owner_to_owner_info(owner: &Owner) -> (String, Option<String>, Option<i64>) {
    match owner {
        Owner::AddressOwner(address) => {
            ("AddressOwner".to_string(), Some(address.to_string()), None)
        }
        Owner::ObjectOwner(address) => ("ObjectOwner".to_string(), Some(address.to_string()), None),
        Owner::Shared {
            initial_shared_version,
        } => (
            "Shared".to_string(),
            None,
            Some(initial_shared_version.value() as i64),
        ),
        Owner::Immutable => ("Immutable".to_string(), None, None),
    }
}

const CREATED_STATUS: &str = "CREATED";
const MUTATED_STATUS: &str = "MUTATED";
const TRANSFERRED_STATUS: &str = "TRANSFERRED";
const DELETED_STATUS: &str = "DELETED";

pub fn commit_objects_from_events(
    pg_pool_conn: &mut PgPoolConnection,
    events: Vec<SuiEvent>,
) -> Result<(), IndexerError> {
    let mut new_objects = vec![];
    let mut object_transfers = vec![];
    let mut object_mutations = vec![];
    let mut object_deletions = vec![];

    for e in events.into_iter() {
        match e {
            SuiEvent::NewObject {
                package_id,
                transaction_module,
                sender: _,
                recipient,
                object_type,
                object_id,
                version,
            } => {
                let (owner_type, owner_address, initial_shared_version) =
                    owner_to_owner_info(&recipient);
                new_objects.push(NewObject {
                    object_id: object_id.to_string(),
                    version: version.value() as i64,
                    owner_type,
                    owner_address,
                    initial_shared_version,
                    package_id: package_id.to_string(),
                    transaction_module,
                    object_type: Some(object_type),
                    object_status: CREATED_STATUS.to_string(),
                });
            }
            SuiEvent::TransferObject {
                package_id,
                transaction_module,
                sender: _,
                recipient,
                object_type,
                object_id,
                version,
            } => {
                let (owner_type, owner_address, initial_shared_version) =
                    owner_to_owner_info(&recipient);
                object_transfers.push(NewObject {
                    object_id: object_id.to_string(),
                    version: version.value() as i64,
                    owner_type,
                    owner_address,
                    initial_shared_version,
                    package_id: package_id.to_string(),
                    transaction_module,
                    object_type: Some(object_type),
                    object_status: TRANSFERRED_STATUS.to_string(),
                });
            }
            SuiEvent::MutateObject {
                object_id, version, ..
            } => {
                object_mutations.push((
                    object_id.to_string(),
                    (version.value() as i64, MUTATED_STATUS.to_string()),
                ));
            }
            SuiEvent::DeleteObject { object_id, .. } => {
                object_deletions.push(object_id.to_string());
            }
            _ => {}
        }
    }
    commit_new_objects(pg_pool_conn, new_objects)?;
    commit_object_transfers(pg_pool_conn, object_transfers)?;
    commit_object_mutations(pg_pool_conn, object_mutations)?;
    commit_object_deletions(pg_pool_conn, object_deletions)?;

    Ok(())
}
