// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context as _, Result, bail};
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_data_store::Node;
use sui_data_store::stores::DataStore;
use sui_data_store::{ObjectKey, ObjectStore as _, VersionQuery};
use sui_types::SYSTEM_PACKAGE_ADDRESSES;
use sui_types::base_types::ObjectID;
use sui_types::object::Object;

/// Reads objects at a historical checkpoint over GraphQL.
pub(crate) struct GqlClient {
    store: Arc<DataStore>,
}

impl GqlClient {
    pub(crate) fn new(node: Node) -> Result<Self> {
        let store = DataStore::new(node, env!("CARGO_PKG_VERSION"))
            .context("building the GraphQL data store")?;
        Ok(Self {
            store: Arc::new(store),
        })
    }

    /// The system (framework) packages as they were at `checkpoint`, keyed by id.
    ///
    /// Framework upgrades only ever land in an epoch-boundary transaction — the authority proposes
    /// `system_packages` only when the protocol version bumps — so these are constant across an
    /// epoch and one read at the epoch's first checkpoint covers it.
    ///
    /// A package not yet published at `checkpoint` (`0xb` and `0xdee9` were introduced mid-history)
    /// comes back absent and is simply omitted.
    pub(crate) async fn system_packages_at(
        &self,
        checkpoint: u64,
    ) -> Result<BTreeMap<ObjectID, Object>> {
        let ids: Vec<ObjectID> = SYSTEM_PACKAGE_ADDRESSES
            .iter()
            .copied()
            .map(Into::into)
            .collect();
        let keys: Vec<ObjectKey> = ids
            .iter()
            .map(|&object_id| ObjectKey {
                object_id,
                version_query: VersionQuery::AtCheckpoint(checkpoint),
            })
            .collect();

        // `ObjectStore::get_objects` is synchronous and drives the query with an internal
        // `block_on`, so keep it off the runtime's worker threads.
        let store = self.store.clone();
        let results = tokio::task::spawn_blocking(move || store.get_objects(&keys))
            .await
            .context("system package query panicked")?
            .with_context(|| format!("reading system packages at checkpoint {checkpoint}"))?;

        // The store contracts one positional result per requested key.
        if results.len() != ids.len() {
            bail!(
                "GraphQL returned {} objects for {} requested keys",
                results.len(),
                ids.len()
            );
        }

        let mut packages = BTreeMap::new();
        for (i, result) in results.into_iter().enumerate() {
            let Some((object, _version)) = result else {
                continue;
            };
            if !object.is_package() {
                bail!(
                    "object {} at checkpoint {checkpoint} is not a package",
                    ids[i]
                );
            }
            packages.insert(object.id(), object);
        }
        Ok(packages)
    }
}
