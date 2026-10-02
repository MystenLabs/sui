// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::{PackageMetadata, PackageStore},
    static_programmable_transactions::linkage::config::ResolutionConfig,
};
use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
};
use sui_types::base_types::ObjectID;
use sui_types::{
    error::ExecutionErrorTrait, execution_status::ExecutionErrorKind, package_config::MinVersion,
};

/// Unifiers. These are used to determine how to unify two packages.
#[derive(Debug, Clone)]
pub enum VersionConstraint {
    /// An exact constraint unifies as follows:
    /// 1. Exact(a) ~ Exact(b) ==> Exact(a), iff a == b
    /// 2. Exact(a) ~ AtLeast(b) ==> Exact(a), iff a >= b
    Exact(u64, ObjectID),
    /// An at least constraint unifies as follows:
    /// * AtLeast(a, a_version) ~ AtLeast(b, b_version) ==> AtLeast(x, max(a_version, b_version)),
    ///   where x is the package id of either a or b (the one with the greatest version).
    AtLeast(u64, ObjectID),
}

/// How a specific version of a package resolves, recorded for every package version the linkage
/// refinement touched.
#[derive(Debug, Clone, Copy)]
pub struct PackageResolution {
    /// The original id of the package.
    pub original_id: ObjectID,
    /// The resolved version of the package. `None` only for the late-bound self entry that
    /// `ResolvedLinkage::update_for_publication` adds.
    pub version: Option<u64>,
}

pub(crate) type MinVersionResolver<'a, E> = dyn Fn(ObjectID) -> Result<Option<MinVersion>, E> + 'a;

#[derive(Debug, Clone)]
pub(crate) struct ResolutionTable {
    pub(crate) config: ResolutionConfig,
    pub(crate) resolution_table: BTreeMap<ObjectID, VersionConstraint>,
    /// For every version of every package that we have seen, a mapping of the ObjectID for that
    /// package to its runtime ID.
    pub(crate) all_versions_resolution_table: BTreeMap<ObjectID, PackageResolution>,
}

impl ResolutionTable {
    pub fn empty(config: ResolutionConfig) -> Self {
        Self {
            config,
            resolution_table: BTreeMap::new(),
            all_versions_resolution_table: BTreeMap::new(),
        }
    }

    /// Given a list of object IDs, generate a `ResolvedLinkage` for them.
    /// Since this linkage analysis should only be used for types, all packages are resolved
    /// "upwards" (i.e., later versions of the package are preferred).
    pub fn add_type_linkages_to_table<I, E, S>(&mut self, ids: I, store: &S) -> Result<(), E>
    where
        S: PackageStore + ?Sized,
        E: ExecutionErrorTrait,
        I: IntoIterator,
        I::Item: Borrow<ObjectID>,
    {
        for id in ids {
            let pkg = get_package(id.borrow(), store)?;
            let transitive_deps = self
                .config
                .linkage_table(&pkg)
                .into_values()
                .map(ObjectID::from);
            let package_id = pkg.version_id();
            add_and_unify(&package_id, store, self, VersionConstraint::at_least)?;
            for object_id in transitive_deps {
                add_and_unify(&object_id, store, self, VersionConstraint::at_least)?;
            }
        }
        Ok(())
    }
}

impl VersionConstraint {
    pub(crate) fn object_id(&self) -> ObjectID {
        match self {
            VersionConstraint::Exact(_, id) | VersionConstraint::AtLeast(_, id) => *id,
        }
    }

    pub(crate) fn exact<P: PackageMetadata>(pkg: &P) -> Option<VersionConstraint> {
        Some(VersionConstraint::Exact(pkg.version(), pkg.version_id()))
    }

    pub(crate) fn at_least<P: PackageMetadata>(pkg: &P) -> Option<VersionConstraint> {
        Some(VersionConstraint::AtLeast(pkg.version(), pkg.version_id()))
    }

    pub fn unify<E: ExecutionErrorTrait>(
        &self,
        other: &VersionConstraint,
    ) -> Result<VersionConstraint, E> {
        match (&self, other) {
            // If we have two exact resolutions, they must be the same.
            (VersionConstraint::Exact(sv, self_id), VersionConstraint::Exact(ov, other_id)) => {
                if self_id != other_id || sv != ov {
                    Err(E::new_with_source(
                        ExecutionErrorKind::InvalidLinkage,
                        format!(
                            "exact/exact conflicting resolutions for package: linkage requires the same package \
                                 at different versions. Linkage requires exactly {self_id} (version {sv}) and \
                                 {other_id} (version {ov}) to be used in the same transaction"
                        ),
                    ))
                } else {
                    Ok(VersionConstraint::Exact(*sv, *self_id))
                }
            }
            // Take the max if you have two at least resolutions.
            (
                VersionConstraint::AtLeast(self_version, sid),
                VersionConstraint::AtLeast(other_version, oid),
            ) => {
                let id = if self_version > other_version {
                    *sid
                } else {
                    *oid
                };

                Ok(VersionConstraint::AtLeast(
                    *self_version.max(other_version),
                    id,
                ))
            }
            // If you unify an exact and an at least, the exact must be greater than or equal to
            // the at least. It unifies to an exact.
            (
                VersionConstraint::Exact(exact_version, exact_id),
                VersionConstraint::AtLeast(at_least_version, at_least_id),
            )
            | (
                VersionConstraint::AtLeast(at_least_version, at_least_id),
                VersionConstraint::Exact(exact_version, exact_id),
            ) => {
                if exact_version < at_least_version {
                    return Err(E::new_with_source(
                        ExecutionErrorKind::InvalidLinkage,
                        format!(
                            "Exact/AtLeast conflicting resolutions for package: linkage requires exactly this \
                                 package {exact_id} (version {exact_version}) and also at least the following \
                                 version of the package {at_least_id} at version {at_least_version}. However \
                                 {exact_id} is at version {exact_version} which is less than {at_least_version}."
                        ),
                    ));
                }

                Ok(VersionConstraint::Exact(*exact_version, *exact_id))
            }
        }
    }
}

/// Load a package from the store, and update the type origin map with the types in that
/// package.
pub(crate) fn get_package<E: ExecutionErrorTrait, S: PackageStore + ?Sized>(
    object_id: &ObjectID,
    store: &S,
) -> Result<S::Package, E> {
    store
        .get_package(object_id)
        .map_err(|e| E::new_with_source(ExecutionErrorKind::PublishUpgradeMissingDependency, e))?
        .ok_or_else(|| E::from_kind(ExecutionErrorKind::InvalidLinkage))
}

// Add a package to the unification table, unifying it with any existing package in the table.
// Errors if the packages cannot be unified (e.g., if one is exact and the other is not).
pub(crate) fn add_and_unify<E: ExecutionErrorTrait, S: PackageStore + ?Sized>(
    object_id: &ObjectID,
    store: &S,
    resolution_table: &mut ResolutionTable,
    resolution_fn: fn(&S::Package) -> Option<VersionConstraint>,
) -> Result<(), E> {
    let package = get_package(object_id, store)?;

    let Some(resolution) = resolution_fn(&package) else {
        // If the resolution function returns None, we do not need to add this package to the
        // resolution table, and this does not contribute to the linkage analysis.
        return Ok(());
    };
    let original_pkg_id = package.original_id();

    if let Entry::Vacant(e) = resolution_table.resolution_table.entry(original_pkg_id) {
        e.insert(resolution);
    } else {
        let existing_unifier = resolution_table
            .resolution_table
            .get_mut(&original_pkg_id)
            .expect("Guaranteed to exist");
        *existing_unifier = existing_unifier.unify(&resolution)?;
    }

    record_package_resolution(resolution_table, &package);

    Ok(())
}

fn record_package_resolution<P: PackageMetadata>(
    resolution_table: &mut ResolutionTable,
    package: &P,
) {
    resolution_table
        .all_versions_resolution_table
        .entry(package.version_id())
        .or_insert(PackageResolution {
            original_id: package.original_id(),
            version: Some(package.version()),
        });
}

/// The use-site constraint retained while selected packages are expanded.
///
/// Besides selecting the `VersionConstraint` constructor, this is part of the expansion key: an
/// `Exact` expansion must not suppress a separate `AtLeast` expansion of the same package, or
/// vice versa, because they impose different constraints on its dependencies.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum ConstraintKind {
    Exact,
    AtLeast,
}

impl ConstraintKind {
    fn resolution_fn<P: PackageMetadata>(self) -> fn(&P) -> Option<VersionConstraint> {
        match self {
            Self::Exact => VersionConstraint::exact,
            Self::AtLeast => VersionConstraint::at_least,
        }
    }
}

/// Resolves package references and expands their direct linkage tables for one transaction.
pub(crate) struct LinkageStoreResolver<'a, S: PackageStore + ?Sized, E> {
    store: &'a S,
    minversion_resolver: Option<&'a MinVersionResolver<'a, E>>,
    minversion_cache: BTreeMap<ObjectID, Option<MinVersion>>,
    // A package must be expanded once per package/dependency constraint pair. Constraints are
    // unified before consulting this set, so a stricter dependency context is never discarded.
    expanded: BTreeSet<(ObjectID, ConstraintKind, ConstraintKind)>,
}

impl<'a, S: PackageStore + ?Sized, E: ExecutionErrorTrait> LinkageStoreResolver<'a, S, E> {
    pub(crate) fn new(
        store: &'a S,
        minversion_resolver: Option<&'a MinVersionResolver<'a, E>>,
    ) -> Self {
        Self {
            store,
            minversion_resolver,
            minversion_cache: BTreeMap::new(),
            expanded: BTreeSet::new(),
        }
    }

    pub(crate) fn store(&self) -> &S {
        self.store
    }

    /// Resolve a package reference through its stable minversion selection.
    pub(crate) fn resolve_package_id(&mut self, package_id: ObjectID) -> Result<ObjectID, E> {
        Ok(self.load_package(&package_id)?.version_id())
    }

    /// Resolve a package and iteratively add its selected dependency graph to the table.
    ///
    /// For example, if a reference to `P@v1` selects `P@v2` through minversion, this records both
    /// package IDs but expands `P@v2`'s direct linkage table.
    pub(crate) fn resolve_package(
        &mut self,
        resolution_table: &mut ResolutionTable,
        package_id: ObjectID,
        package_constraint: ConstraintKind,
        dependency_constraint: ConstraintKind,
    ) -> Result<(), E> {
        let mut pending = BTreeSet::from([(package_id, package_constraint, dependency_constraint)]);
        while let Some((package_id, package_constraint, dependency_constraint)) = pending.pop_last()
        {
            let referenced_package = get_package(&package_id, self.store)?;
            // Retain the historical reference for provenance. `add_and_unify` below records the
            // selected executable package too, so linkage lookups work for both package IDs.
            record_package_resolution(resolution_table, &referenced_package);
            let package = self.load_package(&package_id)?;
            let selected_id = package.version_id();
            add_and_unify(
                &selected_id,
                self.store,
                resolution_table,
                package_constraint.resolution_fn(),
            )?;
            if !self
                .expanded
                .insert((selected_id, package_constraint, dependency_constraint))
            {
                continue;
            }
            // The set deduplicates identical work, but retains distinct dependency constraints:
            // each is unified above and must expand dependencies under its own context.
            pending.extend(
                resolution_table
                    .config
                    .linkage_table(&package)
                    .into_values()
                    .map(ObjectID::from)
                    .map(|dependency_id| {
                        (dependency_id, dependency_constraint, dependency_constraint)
                    }),
            );
        }
        Ok(())
    }

    /// Collect original IDs from the selected graph without applying linkage constraints.
    pub(crate) fn collect_original_ids(
        &mut self,
        resolution_table: &ResolutionTable,
        package_id: ObjectID,
        original_ids: &mut BTreeSet<ObjectID>,
    ) -> Result<(), E> {
        let mut pending = vec![package_id];
        let mut visited = BTreeSet::new();
        while let Some(package_id) = pending.pop() {
            // The forbid-list is only defined for user-package families. Skipping native packages
            // here prevents a system-only transaction from consulting the package-policy root.
            if sui_types::is_system_package(package_id) {
                continue;
            }
            let package = self.load_package(&package_id)?;
            if !visited.insert(package.version_id()) {
                continue;
            }
            original_ids.insert(package.original_id());
            pending.extend(
                resolution_table
                    .config
                    .linkage_table(&package)
                    .into_values()
                    .map(ObjectID::from),
            );
        }
        Ok(())
    }

    /// Collect exact package versions declared by a publish or upgrade, including transitive
    /// dependencies. This deliberately bypasses minversion because policy must validate the
    /// versions the command records, rather than substitutions used for executable linkage.
    pub(crate) fn collect_declared_package_versions(
        &self,
        resolution_table: &ResolutionTable,
        package_ids: impl IntoIterator<Item = ObjectID>,
        versions: &mut BTreeSet<(ObjectID, u64)>,
    ) -> Result<(), E> {
        let mut pending = package_ids.into_iter().collect::<Vec<_>>();
        let mut visited = BTreeSet::new();
        while let Some(package_id) = pending.pop() {
            if sui_types::is_system_package(package_id) {
                continue;
            }
            let package = get_package(&package_id, self.store)?;
            if !visited.insert(package.version_id()) {
                continue;
            }
            versions.insert((package.original_id(), package.version()));
            pending.extend(
                resolution_table
                    .config
                    .linkage_table(&package)
                    .into_values()
                    .map(ObjectID::from),
            );
        }
        Ok(())
    }

    /// Load a reference through its stable minversion setting, caching one setting per package family.
    pub(crate) fn load_package(&mut self, object_id: &ObjectID) -> Result<S::Package, E> {
        let package = get_package(object_id, self.store)?;
        // Native packages are not backing-store package objects and have no user policy entry.
        // Return before invoking the resolver so system-package execution does not read the root.
        if sui_types::is_system_package(package.version_id()) {
            return Ok(package);
        }
        let Some(minversion_resolver) = self.minversion_resolver else {
            return Ok(package);
        };
        let original_id = package.original_id();
        let minversion = match self.minversion_cache.entry(original_id) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(minversion_resolver(original_id)?),
        };
        let Some(minversion) = minversion else {
            return Ok(package);
        };
        // Minversion only raises older references. A reference that is already at or above the
        // selected version remains unchanged.
        if package.version() >= minversion.version {
            return Ok(package);
        }
        let selected_id = minversion.package_id.bytes;
        let selected = get_package::<E, _>(&selected_id, self.store).map_err(|error| {
            E::new_with_source(
                ExecutionErrorKind::InvalidLinkage,
                format!("invalid minversion selection for package {original_id}: {error}"),
            )
        })?;
        if selected.original_id() != original_id
            || selected.version_id() != selected_id
            || selected.version() != minversion.version
        {
            return Err(E::new_with_source(
                ExecutionErrorKind::InvalidLinkage,
                format!("invalid minversion selection for package {original_id}"),
            ));
        }
        Ok(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::static_programmable_transactions::linkage::config::{
        LinkageConfig, ResolutionConfig,
    };
    use move_binary_format::binary_config::BinaryConfig;
    use move_core_types::identifier::IdentStr;
    use move_vm_runtime::shared::types::{OriginalId, VersionId};
    use std::collections::BTreeMap;
    use sui_types::{
        SUI_FRAMEWORK_PACKAGE_ID,
        error::{ExecutionError, SuiResult},
        id::ID,
        package_config::MinVersion,
    };

    #[derive(Clone)]
    struct TestPackage {
        id: ObjectID,
        original_id: ObjectID,
        linkage: BTreeMap<OriginalId, VersionId>,
    }

    impl PackageMetadata for TestPackage {
        fn version(&self) -> u64 {
            u64::from(self.id.into_bytes()[ObjectID::LENGTH - 1])
        }

        fn version_id(&self) -> ObjectID {
            self.id
        }

        fn original_id(&self) -> ObjectID {
            self.original_id
        }

        fn linkage_table(&self) -> BTreeMap<OriginalId, VersionId> {
            self.linkage.clone()
        }
    }

    struct TestStore(BTreeMap<ObjectID, TestPackage>);

    impl PackageStore for TestStore {
        type Package = TestPackage;

        fn get_package(&self, id: &ObjectID) -> SuiResult<Option<Self::Package>> {
            Ok(self.0.get(id).cloned())
        }

        fn resolve_type_to_defining_id(
            &self,
            _module_address: ObjectID,
            _module_name: &IdentStr,
            _type_name: &IdentStr,
        ) -> SuiResult<Option<ObjectID>> {
            Ok(None)
        }
    }

    #[test]
    fn minversion_resolution_records_referenced_and_selected_packages() {
        let original_id = ObjectID::from_single_byte(1);
        let historical_id = ObjectID::from_single_byte(10);
        let selected_id = ObjectID::from_single_byte(12);
        let store = TestStore(BTreeMap::from([
            (
                historical_id,
                TestPackage {
                    id: historical_id,
                    original_id,
                    linkage: BTreeMap::new(),
                },
            ),
            (
                selected_id,
                TestPackage {
                    id: selected_id,
                    original_id,
                    linkage: BTreeMap::new(),
                },
            ),
        ]));
        let config =
            ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
        let mut resolution_table = ResolutionTable::empty(config);
        let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
            Ok((id == original_id).then_some(MinVersion {
                version: 12,
                package_id: ID::new(selected_id),
            }))
        };
        let mut resolver =
            LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));

        resolver
            .resolve_package(
                &mut resolution_table,
                historical_id,
                ConstraintKind::AtLeast,
                ConstraintKind::AtLeast,
            )
            .unwrap();

        assert!(matches!(
            resolution_table.resolution_table.get(&original_id),
            Some(VersionConstraint::AtLeast(12, id)) if *id == selected_id
        ));
        for (package_id, version) in [(historical_id, 10), (selected_id, 12)] {
            let resolution = resolution_table
                .all_versions_resolution_table
                .get(&package_id)
                .unwrap();
            assert_eq!(resolution.original_id, original_id);
            assert_eq!(resolution.version, Some(version));
        }
    }

    #[test]
    fn selected_package_collection_uses_selected_package_dependencies() {
        let original_id = ObjectID::from_single_byte(1);
        let historical_id = ObjectID::from_single_byte(10);
        let selected_id = ObjectID::from_single_byte(12);
        let selected_dependency = ObjectID::from_single_byte(100);
        let store = TestStore(BTreeMap::from([
            (
                historical_id,
                TestPackage {
                    id: historical_id,
                    original_id,
                    linkage: BTreeMap::new(),
                },
            ),
            (
                selected_id,
                TestPackage {
                    id: selected_id,
                    original_id,
                    linkage: BTreeMap::from([(
                        selected_dependency.into(),
                        selected_dependency.into(),
                    )]),
                },
            ),
            (
                selected_dependency,
                TestPackage {
                    id: selected_dependency,
                    original_id: selected_dependency,
                    linkage: BTreeMap::new(),
                },
            ),
        ]));
        let config =
            ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
        let resolution_table = ResolutionTable::empty(config);
        let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
            Ok((id == original_id).then_some(MinVersion {
                version: 12,
                package_id: ID::new(selected_id),
            }))
        };
        let mut builder =
            LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));
        let mut original_ids = BTreeSet::new();

        builder
            .collect_original_ids(&resolution_table, historical_id, &mut original_ids)
            .unwrap();

        assert_eq!(
            original_ids,
            BTreeSet::from([original_id, selected_dependency])
        );
    }

    #[test]
    fn system_packages_do_not_read_package_policy() {
        let store = TestStore(BTreeMap::new());
        let config =
            ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
        let resolution_table = ResolutionTable::empty(config);
        let minversion_resolver = |_| -> Result<Option<MinVersion>, ExecutionError> {
            panic!("system packages must not read package policy")
        };
        let mut resolver =
            LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));
        let mut original_ids = BTreeSet::new();

        resolver
            .collect_original_ids(
                &resolution_table,
                SUI_FRAMEWORK_PACKAGE_ID,
                &mut original_ids,
            )
            .unwrap();

        assert!(original_ids.is_empty());
    }

    #[test]
    fn minversion_does_not_downgrade_equal_or_newer_packages() {
        let original_id = ObjectID::from_single_byte(1);
        let older_id = ObjectID::from_single_byte(10);
        let stable_id = ObjectID::from_single_byte(12);
        let newer_id = ObjectID::from_single_byte(13);
        let store = TestStore(BTreeMap::from([
            (
                older_id,
                TestPackage {
                    id: older_id,
                    original_id,
                    linkage: BTreeMap::new(),
                },
            ),
            (
                stable_id,
                TestPackage {
                    id: stable_id,
                    original_id,
                    linkage: BTreeMap::new(),
                },
            ),
            (
                newer_id,
                TestPackage {
                    id: newer_id,
                    original_id,
                    linkage: BTreeMap::new(),
                },
            ),
        ]));
        let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
            Ok((id == original_id).then_some(MinVersion {
                version: 12,
                package_id: ID::new(stable_id),
            }))
        };
        let mut resolver =
            LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));

        assert_eq!(
            resolver.load_package(&older_id).unwrap().version_id(),
            stable_id
        );
        assert_eq!(
            resolver.load_package(&stable_id).unwrap().version_id(),
            stable_id
        );
        assert_eq!(
            resolver.load_package(&newer_id).unwrap().version_id(),
            newer_id
        );
    }

    #[test]
    fn invalid_minversion_selections_are_rejected() {
        let original_id = ObjectID::from_single_byte(1);
        let historical_id = ObjectID::from_single_byte(10);
        let selected_id = ObjectID::from_single_byte(12);
        let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
            Ok((id == original_id).then_some(MinVersion {
                version: 12,
                package_id: ID::new(selected_id),
            }))
        };

        let historical = TestPackage {
            id: historical_id,
            original_id,
            linkage: BTreeMap::new(),
        };
        let cases = [
            // The selected package is absent.
            TestStore(BTreeMap::from([(historical_id, historical.clone())])),
            // The selected package belongs to another original package family.
            TestStore(BTreeMap::from([
                (historical_id, historical.clone()),
                (
                    selected_id,
                    TestPackage {
                        id: selected_id,
                        original_id: ObjectID::from_single_byte(2),
                        linkage: BTreeMap::new(),
                    },
                ),
            ])),
            // The selected package ID does not have the configured version.
            TestStore(BTreeMap::from([
                (historical_id, historical),
                (
                    selected_id,
                    TestPackage {
                        id: ObjectID::from_single_byte(13),
                        original_id,
                        linkage: BTreeMap::new(),
                    },
                ),
            ])),
        ];

        for store in &cases {
            let mut resolver =
                LinkageStoreResolver::<_, ExecutionError>::new(store, Some(&minversion_resolver));
            assert!(resolver.load_package(&historical_id).is_err());
        }
    }

    #[test]
    fn cyclic_linkage_resolution_terminates() {
        let root = ObjectID::from_single_byte(1);
        let dependency = ObjectID::from_single_byte(2);
        let store = TestStore(BTreeMap::from([
            (
                root,
                TestPackage {
                    id: root,
                    original_id: root,
                    linkage: BTreeMap::from([(dependency.into(), dependency.into())]),
                },
            ),
            (
                dependency,
                TestPackage {
                    id: dependency,
                    original_id: dependency,
                    linkage: BTreeMap::from([(root.into(), root.into())]),
                },
            ),
        ]));
        let config =
            ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
        let mut resolution_table = ResolutionTable::empty(config);
        let mut resolver = LinkageStoreResolver::<_, ExecutionError>::new(&store, None);

        resolver
            .resolve_package(
                &mut resolution_table,
                root,
                ConstraintKind::AtLeast,
                ConstraintKind::AtLeast,
            )
            .unwrap();

        assert_eq!(resolution_table.resolution_table.len(), 2);
    }

    #[test]
    fn repeated_selected_dependencies_and_cycles_terminate() {
        let root_original_id = ObjectID::from_single_byte(1);
        let root_historical_id = ObjectID::from_single_byte(10);
        let root_selected_id = ObjectID::from_single_byte(12);
        let left = ObjectID::from_single_byte(2);
        let right = ObjectID::from_single_byte(3);
        let shared_original_id = ObjectID::from_single_byte(4);
        let shared_historical_id = ObjectID::from_single_byte(40);
        let shared_selected_id = ObjectID::from_single_byte(42);
        let tail = ObjectID::from_single_byte(5);
        let store = TestStore(BTreeMap::from([
            (
                root_historical_id,
                TestPackage {
                    id: root_historical_id,
                    original_id: root_original_id,
                    linkage: BTreeMap::new(),
                },
            ),
            (
                root_selected_id,
                TestPackage {
                    id: root_selected_id,
                    original_id: root_original_id,
                    linkage: BTreeMap::from([
                        (left.into(), left.into()),
                        (right.into(), right.into()),
                    ]),
                },
            ),
            (
                left,
                TestPackage {
                    id: left,
                    original_id: left,
                    linkage: BTreeMap::from([(
                        shared_original_id.into(),
                        shared_historical_id.into(),
                    )]),
                },
            ),
            (
                right,
                TestPackage {
                    id: right,
                    original_id: right,
                    linkage: BTreeMap::from([(
                        shared_original_id.into(),
                        shared_historical_id.into(),
                    )]),
                },
            ),
            (
                shared_historical_id,
                TestPackage {
                    id: shared_historical_id,
                    original_id: shared_original_id,
                    linkage: BTreeMap::new(),
                },
            ),
            (
                shared_selected_id,
                TestPackage {
                    id: shared_selected_id,
                    original_id: shared_original_id,
                    linkage: BTreeMap::from([(tail.into(), tail.into())]),
                },
            ),
            (
                tail,
                TestPackage {
                    id: tail,
                    original_id: tail,
                    linkage: BTreeMap::from([(right.into(), right.into())]),
                },
            ),
        ]));
        let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
            Ok(match id {
                id if id == root_original_id => Some(MinVersion {
                    version: 12,
                    package_id: ID::new(root_selected_id),
                }),
                id if id == shared_original_id => Some(MinVersion {
                    version: 42,
                    package_id: ID::new(shared_selected_id),
                }),
                _ => None,
            })
        };
        let config =
            ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
        let mut resolution_table = ResolutionTable::empty(config);
        let mut resolver =
            LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));

        resolver
            .resolve_package(
                &mut resolution_table,
                root_historical_id,
                ConstraintKind::AtLeast,
                ConstraintKind::AtLeast,
            )
            .unwrap();

        assert_eq!(resolution_table.resolution_table.len(), 5);
        assert!(matches!(
            resolution_table.resolution_table.get(&shared_original_id),
            Some(VersionConstraint::AtLeast(42, id)) if *id == shared_selected_id
        ));
        assert!(resolution_table.resolution_table.contains_key(&tail));
    }

    #[test]
    fn exact_expansion_is_not_suppressed_by_at_least_expansion() {
        let root_original_id = ObjectID::from_single_byte(1);
        let historical_root_id = ObjectID::from_single_byte(10);
        let selected_root_id = ObjectID::from_single_byte(12);
        let dependency = ObjectID::from_single_byte(2);
        let store = TestStore(BTreeMap::from([
            (
                historical_root_id,
                TestPackage {
                    id: historical_root_id,
                    original_id: root_original_id,
                    linkage: BTreeMap::new(),
                },
            ),
            (
                selected_root_id,
                TestPackage {
                    id: selected_root_id,
                    original_id: root_original_id,
                    linkage: BTreeMap::from([(dependency.into(), dependency.into())]),
                },
            ),
            (
                dependency,
                TestPackage {
                    id: dependency,
                    original_id: dependency,
                    linkage: BTreeMap::new(),
                },
            ),
        ]));
        let minversion_resolver = |id| -> Result<Option<MinVersion>, ExecutionError> {
            Ok((id == root_original_id).then_some(MinVersion {
                version: 12,
                package_id: ID::new(selected_root_id),
            }))
        };
        let config =
            ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
        let mut resolution_table = ResolutionTable::empty(config);
        let mut builder =
            LinkageStoreResolver::<_, ExecutionError>::new(&store, Some(&minversion_resolver));

        // Both historical references select the same v12 package. The second, exact traversal
        // must still expand its dependency under Exact rather than being suppressed by the first.
        builder
            .resolve_package(
                &mut resolution_table,
                historical_root_id,
                ConstraintKind::AtLeast,
                ConstraintKind::AtLeast,
            )
            .unwrap();
        builder
            .resolve_package(
                &mut resolution_table,
                historical_root_id,
                ConstraintKind::Exact,
                ConstraintKind::Exact,
            )
            .unwrap();

        assert!(matches!(
            resolution_table.resolution_table.get(&root_original_id),
            Some(VersionConstraint::Exact(12, id)) if *id == selected_root_id
        ));
        assert!(matches!(
            resolution_table.resolution_table.get(&dependency),
            Some(VersionConstraint::Exact(2, id)) if *id == dependency
        ));
    }

    #[test]
    fn exact_dependency_expansion_is_not_suppressed_by_at_least_dependencies() {
        let root = ObjectID::from_single_byte(1);
        let dependency = ObjectID::from_single_byte(2);
        let store = TestStore(BTreeMap::from([
            (
                root,
                TestPackage {
                    id: root,
                    original_id: root,
                    linkage: BTreeMap::from([(dependency.into(), dependency.into())]),
                },
            ),
            (
                dependency,
                TestPackage {
                    id: dependency,
                    original_id: dependency,
                    linkage: BTreeMap::new(),
                },
            ),
        ]));
        let config =
            ResolutionConfig::new(LinkageConfig::new(None, false), BinaryConfig::standard());
        let mut resolution_table = ResolutionTable::empty(config);
        let mut builder = LinkageStoreResolver::<_, ExecutionError>::new(&store, None);

        builder
            .resolve_package(
                &mut resolution_table,
                root,
                ConstraintKind::Exact,
                ConstraintKind::AtLeast,
            )
            .unwrap();
        builder
            .resolve_package(
                &mut resolution_table,
                root,
                ConstraintKind::Exact,
                ConstraintKind::Exact,
            )
            .unwrap();

        assert!(matches!(
            resolution_table.resolution_table.get(&dependency),
            Some(VersionConstraint::Exact(2, id)) if *id == dependency
        ));
    }
}
