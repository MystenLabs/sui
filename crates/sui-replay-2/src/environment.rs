// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::{DataStore, InputObject},
    errors::ReplayError,
    replay_txn::packages_from_type_tag,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Bound,
};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    effects::TransactionEffects,
    object::Object,
    supported_protocol_versions::{Chain, ProtocolConfig},
    transaction::TransactionData,
};
use tracing::trace;

// True if the package is a system package
pub fn is_framework_package(pkg_id: &ObjectID) -> bool {
    sui_types::SYSTEM_PACKAGE_ADDRESSES.contains(pkg_id)
}

pub struct ReplayEnvironment {
    // data store access
    data_store: DataStore,
    // caches
    // system packages as pkg_id -> epoch -> MovePackage (as Object)
    system_packages: BTreeMap<ObjectID, BTreeMap<u64, Object>>,
    // all package objects as pkg_id -> Object
    package_objects: BTreeMap<ObjectID, Object>,
    // objects as object_id -> version -> Object
    objects: BTreeMap<ObjectID, BTreeMap<u64, Object>>,
}

impl ReplayEnvironment {
    pub async fn new(data_store: DataStore) -> Result<Self, ReplayError> {
        // load system packages
        let system_packages = data_store.load_system_packages().await?;
        Ok(Self {
            data_store,
            system_packages,
            package_objects: BTreeMap::new(),
            objects: BTreeMap::new(),
        })
    }

    // Get the chain the environment is running on (mainnet, testnet, etc.)
    pub fn chain(&self) -> Chain {
        self.data_store.chain()
    }

    //
    // EpochStore API
    //

    pub fn protocol_config(&self, epoch: u64, chain: Chain) -> Result<ProtocolConfig, ReplayError> {
        self.data_store.protocol_config(epoch, chain)
    }

    pub fn epoch_timestamp(&self, epoch: u64) -> Result<u64, ReplayError> {
        self.data_store.epoch_timestamp(epoch)
    }

    pub fn rgp(&self, epoch: u64) -> Result<u64, ReplayError> {
        self.data_store.rgp(epoch)
    }

    //
    // Transaction load API
    //

    // Load transaction data and effects
    pub async fn load_txn_data(
        &self,
        tx_digest: &str,
    ) -> Result<(TransactionData, TransactionEffects), ReplayError> {
        self.data_store
            .transaction_data_and_effects(tx_digest)
            .await
    }

    // Load and add objects to the environment.
    // Return the packages of the type parameters instantiated
    // (e.g. `SUI` in `Coin<SUI>`).
    pub async fn load_objects(
        &mut self,
        object_ids: &BTreeSet<InputObject>,
    ) -> Result<BTreeSet<ObjectID>, ReplayError> {
        let mut packages = BTreeSet::new();
        let objects = self.data_store.load_objects(object_ids).await?;
        for (object_id, version, object) in objects {
            if let Some(tag) = object.as_inner().struct_tag() {
                packages_from_type_tag(&tag.into(), &mut packages);
            }
            let version = version.unwrap();
            self.objects
                .entry(object_id)
                .or_default()
                .insert(version, object);
        }
        Ok(packages)
    }

    // Load packages and their dependencies.
    // It's a 2 step process: first loads all packages in `packages`,
    // then collects all dependencies and load them
    pub async fn load_packages(
        &mut self,
        packages: &BTreeSet<ObjectID>,
    ) -> Result<(), ReplayError> {
        let pkg_ids = packages
            .iter()
            .map(|id| InputObject {
                object_id: *id,
                version: None,
            })
            .collect::<BTreeSet<_>>();
        let package_objects = self
            .data_store
            .load_objects(&pkg_ids)
            .await?
            .into_iter()
            .map(|(id, _version, obj)| (id, obj))
            .collect::<BTreeMap<_, _>>();
        trace!("package_objects: {:#?}", package_objects);
        let deps = get_packages_deps(&package_objects)
            .into_iter()
            .map(|object_id| InputObject {
                object_id,
                version: None,
            })
            .collect();
        trace!("deps: {:#?}", deps);
        self.package_objects.extend(package_objects);
        let package_objects = self
            .data_store
            .load_objects(&deps)
            .await?
            .into_iter()
            .map(|(id, _version, obj)| (id, obj))
            .collect::<BTreeMap<_, _>>();
        trace!("deps: {:#?}", package_objects);
        self.package_objects.extend(package_objects);

        Ok(())
    }

    //
    // Execution API
    //

    // Get an object at the latest version known to the environment
    pub fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.objects
            .get(object_id)
            .and_then(|versions| versions.last_key_value())
            .map(|(_, v)| v.clone())
    }

    // Get an object at a specific version
    pub fn get_object_at_version(&self, object_id: &ObjectID, version: u64) -> Option<Object> {
        self.objects
            .get(object_id)
            .and_then(|versions| versions.get(&version))
            .cloned()
    }

    // Load a system package for the given epoch.
    // System package ids are stable and the version is only
    // known via the epoch.
    pub fn get_system_package_object(
        &self,
        pkg_id: &ObjectID,
        epoch: u64,
    ) -> Result<Object, ReplayError> {
        let pkgs = self.system_packages.get(pkg_id);
        match pkgs {
            Some(versions) => {
                if let Some((_, pkg)) = versions
                    .range((Bound::Unbounded, Bound::Included(&epoch)))
                    .next_back()
                {
                    Ok(pkg.clone())
                } else {
                    Err(ReplayError::MissingPackageAtEpoch {
                        pkg: pkg_id.to_string(),
                        epoch,
                    })
                }
            }
            None => Err(ReplayError::MissingSystemPackage {
                pkg: pkg_id.to_string(),
                epoch,
            }),
        }
    }

    // Get a package by its ID.
    // The package must exist in the environment, no loading at runtime.
    pub fn get_package_object(&self, package_id: &ObjectID) -> Result<Object, ReplayError> {
        self.package_objects
            .get(package_id)
            .cloned()
            .ok_or(ReplayError::PackageNotFound {
                pkg: package_id.to_string(),
            })
    }

    // Dynamic field access
    pub fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> Result<Option<Object>, ReplayError> {
        self.data_store
            .read_child_object(parent, child, child_version_upper_bound)
    }

    //
    // Tracing API: revisit...
    //
    pub fn package_objects(&self) -> &BTreeMap<ObjectID, Object> {
        &self.package_objects
    }
}

// Get all dependencies of `packages`.
// Collect all values() of the linkage_table map of each package.
fn get_packages_deps(packages: &BTreeMap<ObjectID, Object>) -> BTreeSet<ObjectID> {
    packages
        .values()
        .flat_map(|pkg| {
            if let Some(package) = pkg.data.try_as_package() {
                package
                    .linkage_table()
                    .values()
                    .map(|upgrade_info| upgrade_info.upgraded_id)
            } else {
                unreachable!("Not a package in package tables")
            }
        })
        .collect::<BTreeSet<_>>()
}
