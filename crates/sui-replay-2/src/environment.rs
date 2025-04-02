// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::{DataStore, InputObject},
    epoch_store::EpochStore,
    errors::ReplayError,
    replay_txn_data::packages_from_type_tag,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    ops::Bound,
};
use sui_types::{base_types::ObjectID, object::Object};
use tracing::debug;

// True if the package is a system package
pub fn is_framework_package(pkg_id: &ObjectID) -> bool {
    sui_types::SYSTEM_PACKAGE_ADDRESSES.contains(pkg_id)
}

pub struct ReplayEnvironment {
    // data store access
    pub data_store: DataStore,
    // responsible for epoch information (protocol configs, rgps, timestamps)
    pub epoch_store: EpochStore,

    //
    // caches
    //

    // system packages as pkg_id -> epoch -> MovePackage (as Object)
    system_packages: BTreeMap<ObjectID, BTreeMap<u64, Object>>,
    // all package objects as pkg_id -> Object
    pub(crate) package_objects: BTreeMap<ObjectID, Object>,
    // objects as object_id -> version -> Object
    pub(crate) objects: BTreeMap<ObjectID, BTreeMap<u64, Object>>,
}

impl ReplayEnvironment {
    pub async fn new(data_store: DataStore, epoch_store: EpochStore) -> Result<Self, ReplayError> {
        // load system packages
        let system_packages = data_store.get_system_packages().await?;
        Ok(Self {
            data_store,
            epoch_store,
            system_packages,
            package_objects: BTreeMap::new(),
            objects: BTreeMap::new(),
        })
    }

    // Load and add objects to the environment
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

    pub fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.objects
            .get(object_id)
            .and_then(|versions| versions.last_key_value())
            .map(|(_, v)| v.clone())
    }

    pub fn get_object_by_version(&self, object_id: &ObjectID, version: u64) -> Option<Object> {
        self.objects
            .get(object_id)
            .and_then(|versions| versions.get(&version))
            .cloned()
    }

    // Load packages and their dependencies
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
        debug!("package_objects: {:#?}", package_objects);
        let deps = get_packages_deps(&package_objects)
            .into_iter()
            .map(|object_id| InputObject {
                object_id,
                version: None,
            })
            .collect();
        debug!("deps: {:#?}", deps);
        self.package_objects.extend(package_objects);
        let package_objects = self
            .data_store
            .load_objects(&deps)
            .await?
            .into_iter()
            .map(|(id, _version, obj)| (id, obj))
            .collect::<BTreeMap<_, _>>();
        debug!("deps: {:#?}", package_objects);
        self.package_objects.extend(package_objects);

        Ok(())
    }

    pub fn get_system_package_at_epoch(
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
            }),
        }
    }
}

fn get_packages_deps(packages: &BTreeMap<ObjectID, Object>) -> BTreeSet<ObjectID> {
    let mut deps = BTreeSet::new();
    for package in packages.values() {
        if let Some(package) = package.data.try_as_package() {
            for upgrade_info in package.linkage_table().values() {
                deps.insert(upgrade_info.upgraded_id);
            }
        } else {
            unreachable!("Not a package in package tables");
        }
    }
    packages.values().any(|pkg| deps.remove(&pkg.id()));
    deps
}

//
// Friendly Debug implementation for ReplayEnvironment. To remove when convenient
//

impl Debug for ReplayEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ReplayEnvironment {
            data_store: _,
            epoch_store: epoch_info,
            system_packages,
            package_objects,
            objects,
        } = self;
        writeln!(f, "ReplayEnvironment({:?})", self.data_store.node())?;
        writeln!(f, ">>>> Epoch Info: {:?}", epoch_info)?;
        print_system_packages(f, system_packages)?;
        print_packages(f, package_objects)?;
        print_objects(f, objects)
    }
}

#[allow(dead_code)]
fn print_system_packages(
    f: &mut std::fmt::Formatter<'_>,
    system_packages: &BTreeMap<ObjectID, BTreeMap<u64, Object>>,
) -> std::fmt::Result {
    writeln!(f, ">>>> System packages:")?;
    for (pkg_id, versions) in system_packages {
        for (epoch, pkg) in versions {
            writeln!(f, "{}[{}] - {:?}", pkg_id, pkg.version(), epoch)?;
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn print_packages(
    f: &mut std::fmt::Formatter<'_>,
    packages: &BTreeMap<ObjectID, Object>,
) -> std::fmt::Result {
    writeln!(f, ">>>> Packages:")?;
    for (pkg_id, pkg) in packages {
        if let Some(package) = pkg.data.try_as_package() {
            writeln!(f, "{}[{}]", pkg_id, package.version())?
        } else {
            writeln!(f, "NOT A PACKAGE {}", pkg_id)?
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn print_objects(
    f: &mut std::fmt::Formatter<'_>,
    objects: &BTreeMap<ObjectID, BTreeMap<u64, Object>>,
) -> std::fmt::Result {
    writeln!(f, ">>>> Objects:")?;
    for (obj_id, version_map) in objects {
        for (version, obj) in version_map {
            if let Some(struct_tag) = obj.struct_tag() {
                writeln!(f, "{}[{}]: {}", obj_id, version, struct_tag)?;
            } else {
                writeln!(
                    f,
                    "Package: {}[{}] (should not reach here)",
                    obj_id, version,
                )?;
            }
        }
    }
    Ok(())
}
