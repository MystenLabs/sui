// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};

use anyhow::Context as _;
use async_graphql::dataloader::Loader;
use diesel::sql_types::{Array, BigInt, Bytea};
use sui_indexer_alt_schema::packages::{StoredPackage, StoredPackageOriginalId};
use sui_types::base_types::ObjectID;

use crate::{error::Error, pg_reader::PgReader};

/// Key for fetching the original ID of a package
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PackageOriginalIdKey(pub ObjectID);

/// Key for fetching the latest version of a package, based on its *original ID* and a checkpoint
/// bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct CheckpointBoundedOriginalPackageKey(pub ObjectID, pub u64);

/// Key for fetching a package by its original ID and version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VersionedOriginalPackageKey(pub ObjectID, pub u64);

#[async_trait::async_trait]
impl Loader<PackageOriginalIdKey> for PgReader {
    type Value = StoredPackageOriginalId;
    type Error = Error;

    async fn load(
        &self,
        keys: &[PackageOriginalIdKey],
    ) -> Result<HashMap<PackageOriginalIdKey, StoredPackageOriginalId>, Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await?;

        let ids: Vec<_> = keys.iter().map(|k| k.0.into_bytes()).collect();
        let query = diesel::sql_query(
            r#"
                SELECT
                    k.package_id,
                    v.original_id,
                    v.cp_sequence_number
                FROM (
                    SELECT UNNEST($1) package_id
                ) k
                CROSS JOIN LATERAL (
                    SELECT
                        original_id,
                        cp_sequence_number
                    FROM
                        kv_packages
                    WHERE
                        kv_packages.package_id = k.package_id
                    LIMIT
                        1
                ) v
            "#,
        )
        .bind::<Array<Bytea>, _>(ids);

        let stored: Vec<StoredPackageOriginalId> = conn.results(query).await?;
        let id_to_stored: HashMap<_, _> = stored
            .iter()
            .map(|package| (&package.package_id[..], package))
            .collect();

        Ok(keys
            .iter()
            .filter_map(|key| {
                let stored = *id_to_stored.get(key.0.into_bytes().as_ref())?;
                Some((*key, stored.clone()))
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl Loader<CheckpointBoundedOriginalPackageKey> for PgReader {
    type Value = StoredPackage;
    type Error = Error;

    async fn load(
        &self,
        keys: &[CheckpointBoundedOriginalPackageKey],
    ) -> Result<HashMap<CheckpointBoundedOriginalPackageKey, StoredPackage>, Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await?;

        let ids: Vec<_> = keys.iter().map(|k| k.0.into_bytes()).collect();
        let cps: Vec<_> = keys.iter().map(|k| k.1 as i64).collect();
        let query = diesel::sql_query(
            r#"
                SELECT
                    v.*
                FROM (
                    SELECT
                        UNNEST($1) original_id,
                        UNNEST($2) cp_sequence_number
                ) k
                CROSS JOIN LATERAL (
                    SELECT
                        package_id,
                        package_version,
                        original_id,
                        is_system_package,
                        serialized_object,
                        cp_sequence_number
                    FROM
                        kv_packages
                    WHERE
                        kv_packages.original_id = k.original_id
                    AND kv_packages.cp_sequence_number <= k.cp_sequence_number
                    ORDER BY
                        cp_sequence_number DESC,
                        package_version DESC
                    LIMIT
                        1
                ) v
            "#,
        )
        .bind::<Array<Bytea>, _>(ids)
        .bind::<Array<BigInt>, _>(cps);

        let stored_packages: Vec<StoredPackage> = conn.results(query).await?;

        // A single data loader request may contain multiple keys for the same package ID. Store
        // them in an ordered map, so that we can find the latest version for each key.
        let mut key_to_stored = BTreeMap::new();
        for package in stored_packages {
            let id = ObjectID::from_bytes(&package.original_id)
                .context("Failed to deserialize ObjectID")?;

            let cp_sequence_number = package.cp_sequence_number as u64;
            key_to_stored.insert(
                CheckpointBoundedOriginalPackageKey(id, cp_sequence_number),
                package,
            );
        }

        Ok(keys
            .iter()
            .filter_map(|key| {
                let (bound, stored) = key_to_stored.range(..=key).last()?;
                (key.0 == bound.0).then(|| (*key, stored.clone()))
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl Loader<VersionedOriginalPackageKey> for PgReader {
    type Value = StoredPackage;
    type Error = Error;

    async fn load(
        &self,
        keys: &[VersionedOriginalPackageKey],
    ) -> Result<HashMap<VersionedOriginalPackageKey, StoredPackage>, Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await?;

        let ids: Vec<_> = keys.iter().map(|k| k.0.into_bytes()).collect();
        let versions: Vec<_> = keys.iter().map(|k| k.1 as i64).collect();
        let query = diesel::sql_query(
            r#"
                SELECT
                    v.*
                FROM (
                    SELECT
                        UNNEST($1) original_id,
                        UNNEST($2) package_version
                ) k
                CROSS JOIN LATERAL (
                    SELECT
                        package_id,
                        package_version,
                        original_id,
                        is_system_package,
                        serialized_object,
                        cp_sequence_number
                    FROM
                        kv_packages
                    WHERE
                        kv_packages.original_id = k.original_id
                    AND kv_packages.package_version = k.package_version
                    LIMIT
                        1
                ) v
            "#,
        )
        .bind::<Array<Bytea>, _>(ids)
        .bind::<Array<BigInt>, _>(versions);

        let stored_packages: Vec<StoredPackage> = conn.results(query).await?;
        let key_to_stored: HashMap<_, _> = stored_packages
            .iter()
            .map(|stored| {
                let id = &stored.original_id[..];
                let version = stored.package_version as u64;
                ((id, version), stored)
            })
            .collect();

        Ok(keys
            .iter()
            .filter_map(|key| {
                let slice: &[u8] = key.0.as_ref();
                let stored = *key_to_stored.get(&(slice, key.1))?;
                Some((*key, stored.clone()))
            })
            .collect())
    }
}
