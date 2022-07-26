// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rocksdb::MultiThreaded;
use std::collections::BTreeMap;
use std::path::PathBuf;
use sui_core::authority::authority_store_tables::StoreTables;
use sui_storage::default_db_options;
use sui_types::crypto::{AuthoritySignInfo, EmptySignInfo};

pub fn list_tables(path: PathBuf) -> anyhow::Result<Vec<String>> {
    rocksdb::DBWithThreadMode::<MultiThreaded>::list_cf(&default_db_options(None, None).0, &path)
        .map_err(|e| e.into())
        .map(|q| {
            q.iter()
                .filter_map(|s| {
                    // The `default` table is not used
                    if s != "default" {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .collect()
        })
}

pub fn dump_table(
    gateway: bool,
    path: PathBuf,
    table_name: &str,
) -> anyhow::Result<BTreeMap<String, String>> {
    let temp_dir = tempfile::tempdir()?.into_path();

    // TODO: Combine these lines in future using Box and dyn skills
    if gateway {
        let store: StoreTables<EmptySignInfo> = StoreTables::open_read_only(path, temp_dir, None);
        store.dump(table_name)
    } else {
        let store: StoreTables<AuthoritySignInfo> =
            StoreTables::open_read_only(path, temp_dir, None);
        store.dump(table_name)
    }
}

#[cfg(test)]
mod test {
    use sui_core::authority::authority_store_tables::StoreTables;
    use sui_types::crypto::AuthoritySignInfo;

    use crate::db_tool::db_dump::{dump_table, list_tables};

    #[tokio::test]
    async fn db_dump_population() -> Result<(), anyhow::Error> {
        let primary_path = tempfile::tempdir()?.into_path();

        // Open the DB for writing
        let _: StoreTables<AuthoritySignInfo> =
            StoreTables::open_read_write(primary_path.clone(), None);

        // Get all the tables
        let tables = list_tables(primary_path.clone()).unwrap();

        let mut missing_tables = vec![];
        for t in tables {
            println!("{}", t);
            if dump_table(false, primary_path.clone(), &t).is_err() {
                missing_tables.push(t);
            }
        }
        if missing_tables.is_empty() {
            return Ok(());
        }
        panic!(
            "{}",
            format!(
                "Missing {} table(s) from DB dump registration function: {:?} \n Update the dump function.",
                missing_tables.len(),
                missing_tables
            )
        );
    }
}
