// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::packages;
use crate::schema::packages::dsl::{
    author as author_column, module_names as module_names_column,
    package_content as package_content_column, package_id as package_id_column,
};
use crate::utils::log_errors_to_pg;
use crate::PgPoolConnection;

use diesel::pg::upsert::excluded;
use diesel::prelude::*;
use diesel::result::Error;
use futures::future::join_all;
use sui_json_rpc_types::SuiEvent;
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectID, SuiAddress};

#[derive(Queryable, Debug, Identifiable)]
#[diesel(primary_key(package_id))]
pub struct Package {
    pub id: i64,
    pub package_id: String,
    pub author: String,
    pub module_names: Vec<Option<String>>,
    pub package_content: String,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = packages)]
pub struct NewPackage {
    pub package_id: String,
    pub author: String,
    pub module_names: Vec<Option<String>>,
    pub package_content: String,
}

pub async fn commit_packages_from_events(
    rpc_client: SuiClient,
    pg_pool_conn: &mut PgPoolConnection,
    events: Vec<SuiEvent>,
) -> Result<usize, IndexerError> {
    let sender_pkg_pair_iter = events.into_iter().filter_map(|event| match event {
        SuiEvent::Publish {
            sender, package_id, ..
        } => Some((sender, package_id)),
        _ => None,
    });

    let mut errors = vec![];
    let new_pkg_res_vec = join_all(
        sender_pkg_pair_iter
            .map(|(sender, pkg)| generate_new_package(rpc_client.clone(), sender, pkg)),
    )
    .await;
    let new_pkgs: Vec<NewPackage> = new_pkg_res_vec
        .into_iter()
        .filter_map(|f| f.map_err(|e| errors.push(e)).ok())
        .collect();

    log_errors_to_pg(pg_pool_conn, errors);
    commit_new_packages(pg_pool_conn, new_pkgs)
}

fn commit_new_packages(
    pg_pool_conn: &mut PgPoolConnection,
    new_pkgs: Vec<NewPackage>,
) -> Result<usize, IndexerError> {
    if new_pkgs.is_empty() {
        return Ok(0);
    }

    let pkg_commit_result: Result<usize, Error> = pg_pool_conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|conn| {
        diesel::insert_into(packages::table)
            .values(&new_pkgs)
            .on_conflict(package_id_column)
            .do_update()
            .set((
                author_column.eq(excluded(author_column)),
                module_names_column.eq(excluded(module_names_column)),
                package_content_column.eq(excluded(package_content_column)),
            ))
            .execute(conn)
    });

    pkg_commit_result.map_err(|e| {
        IndexerError::PostgresWriteError(format!(
            "Failed writing or updating packages {:?} with error: {:?}",
            new_pkgs, e
        ))
    })
}

async fn generate_new_package(
    rpc_client: SuiClient,
    sender: SuiAddress,
    package: ObjectID,
) -> Result<NewPackage, IndexerError> {
    let pkg_module_map = rpc_client
        .read_api()
        .get_normalized_move_modules_by_package(package)
        .await
        .map_err(|e| {
            IndexerError::FullNodeReadingError(format!(
                "Failed reading normalized package from Fullnode with package {:?} and error: {:?}",
                package, e
            ))
        })?;
    let module_names: Vec<Option<String>> = pkg_module_map.keys().cloned().map(Some).collect();
    let pkg_module_map_json = serde_json::to_string(&pkg_module_map).map_err(|err| {
        IndexerError::InsertableParsingError(format!(
            "Failed converting package module map to JSON with error: {:?}",
            err
        ))
    })?;

    Ok(NewPackage {
        package_id: package.to_string(),
        author: sender.to_string(),
        module_names,
        package_content: pkg_module_map_json,
    })
}
