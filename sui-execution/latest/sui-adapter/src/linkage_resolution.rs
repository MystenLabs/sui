// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{execution_mode::ExecutionMode, programmable_transactions::datastore::PackageStore};
use move_binary_format::{binary_config::BinaryConfig, file_format::Visibility};
use move_vm_runtime::shared::linkage_context::LinkageContext;
use std::collections::{btree_map::Entry, BTreeMap};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    error::{ExecutionError, ExecutionErrorKind},
    execution_config_utils::to_binary_config,
    move_package::MovePackage,
    transaction::Command,
    type_input::TypeInput,
    MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID, SUI_SYSTEM_PACKAGE_ID,
};

/// Max number of packages to cache in the PTBLinkageMetadata. If we have more than this, we'll
/// drop the cache and restart the cache.
const MAX_CACHED_PACKAGES: usize = 200;

/// These are the set of native packages in Sui -- importantly they can be used implicitly by
/// different parts of the system and are not required to be explicitly imported always.
/// Additionally, there is no versioning concerns around these as they are "stable" for a given
/// epoch, and are the special packages that are always available, and updated in-place.
const NATIVE_PACKAGE_IDS: &[ObjectID] = &[
    SUI_FRAMEWORK_PACKAGE_ID,
    SUI_SYSTEM_PACKAGE_ID,
    MOVE_STDLIB_PACKAGE_ID,
];

pub trait LinkageAnalysis {
    fn add_command(
        &mut self,
        command: &Command,
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError>;

    fn resolver(&mut self) -> &mut PTBLinkageResolver;
}

pub fn linkage_analysis_for_protocol_config<Mode: ExecutionMode>(
    protocol_config: &ProtocolConfig,
    store: &dyn PackageStore,
) -> Result<Box<dyn LinkageAnalysis>, ExecutionError> {
    Ok(Box::new(PerCommandLinkage::new(
        !Mode::packages_are_predefined(),
        to_binary_config(protocol_config),
        store,
    )?))
}

type TypeOriginMap = BTreeMap<ObjectID, BTreeMap<(String, String), ObjectID>>;

/// Metadata and shared operations for the PTB linkage analysis.
#[derive(Debug)]
pub struct PTBLinkageResolver {
    /// Config to use for the linkage analysis.
    pub linkage_config: LinkageConfig,
    /// Config to use for the binary analysis (needed for deserialization to determine if a
    /// function is a non-public entry function).
    pub binary_config: BinaryConfig,
    /// Cache for packages that we've loaded so far. Note: We may drop this cache if it grows too
    /// large.
    pub package_cache: BTreeMap<ObjectID, MovePackage>,
    /// A mapping of the (original package ID)::<module_name>::<type_name> to the defining ID for
    /// that type.
    pub type_origin_cache: TypeOriginMap,
}

/// Configuration for the linkage analysis.
#[derive(Debug)]
pub struct LinkageConfig {
    /// Do we introduce an `Exact(<pkg_id>)` for each top-level function <pkg_id>::mname::fname?
    pub fix_top_level_functions: bool,
    /// Do we introduce an `Exact(<pkg_id>)` for each type <pkg_id>::mname::tname?
    pub fix_types: bool,
    /// Do we introduce an `Exact(<pkg_id>)` for each transitive dependency of a non-public entry function?
    pub exact_entry_transitive_deps: bool,
    /// Do we introduce an `Exact(<pkg>)` for each transitive dependency of a
    /// function?
    pub exact_transitive_deps: bool,
    /// Whether system packages should always be included as a member in the generated linkage.
    /// This is almost always true except for system transactions and genesis transactions.
    pub always_include_system_packages: bool,
}

/// Unifiers. These are used to determine how to unify two packages.
#[derive(Debug, Clone)]
pub enum ConflictResolution {
    /// An exact constraint unifies as follows:
    /// 1. Exact(a) ~ Exact(b) ==> Exact(a), iff a == b
    /// 2. Exact(a) ~ AtLeast(b) ==> Exact(a), iff a >= b
    Exact(SequenceNumber, ObjectID),
    /// An at least constraint unifies as follows:
    /// * AtLeast(a, a_version) ~ AtLeast(b, b_version) ==> AtLeast(x, max(a_version, b_version)),
    ///   where x is the package id of either a or b (the one with the greatest version).
    AtLeast(SequenceNumber, ObjectID),
}

#[derive(Debug, Clone)]
struct ResolutionTable {
    pub resolution_table: BTreeMap<ObjectID, ConflictResolution>,
    /// For every version of every package that we have seen, a mapping of the ObjectID for that
    /// package to its runtime ID.
    pub all_versions_resolution_table: BTreeMap<ObjectID, ObjectID>,
}

#[derive(Debug)]
pub struct ResolvedLinkage {
    pub linkage: BTreeMap<ObjectID, ObjectID>,
    // A mapping of every package ID to its runtime ID.
    // Note: Multiple packages can have the same runtime ID in this mapping, and domain of this map
    // is a superset of range of `linkage`.
    pub linkage_resolution: BTreeMap<ObjectID, ObjectID>,
    pub versions: BTreeMap<ObjectID, SequenceNumber>,
}

#[derive(Debug)]
pub struct PerCommandLinkage {
    internal: PTBLinkageResolver,
}

#[derive(Debug)]
pub struct UnifiedLinkage {
    /// Current unification table we have for packages. This is a mapping from the original
    /// package ID for a package to its current resolution. This is the "constraint set" that we
    /// are building/solving as we progress across the PTB.
    unification_table: ResolutionTable,
    internal: PTBLinkageResolver,
}

impl LinkageAnalysis for PerCommandLinkage {
    fn add_command(
        &mut self,
        command: &Command,
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        self.add_command(command, store)
    }

    fn resolver(&mut self) -> &mut PTBLinkageResolver {
        &mut self.internal
    }
}

impl LinkageAnalysis for UnifiedLinkage {
    fn add_command(
        &mut self,
        command: &Command,
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        self.add_command(command, store)
    }

    fn resolver(&mut self) -> &mut PTBLinkageResolver {
        &mut self.internal
    }
}

impl LinkageConfig {
    pub fn unified_linkage_settings(always_include_system_packages: bool) -> Self {
        Self {
            fix_top_level_functions: true,
            fix_types: false,
            exact_entry_transitive_deps: false,
            exact_transitive_deps: false,
            always_include_system_packages,
        }
    }

    pub fn per_command_linkage_settings(always_include_system_packages: bool) -> Self {
        Self {
            fix_top_level_functions: true,
            fix_types: false,
            exact_entry_transitive_deps: true,
            exact_transitive_deps: true,
            always_include_system_packages,
        }
    }

    pub fn generate_top_level_fn_constraint(
        &self,
    ) -> for<'a> fn(&'a MovePackage) -> ConflictResolution {
        if self.fix_top_level_functions {
            ConflictResolution::exact
        } else {
            ConflictResolution::at_least
        }
    }

    pub fn generate_type_constraint(&self) -> for<'a> fn(&'a MovePackage) -> ConflictResolution {
        if self.fix_types {
            ConflictResolution::exact
        } else {
            ConflictResolution::at_least
        }
    }

    pub fn generate_entry_transitive_dep_constraint(
        &self,
    ) -> for<'a> fn(&'a MovePackage) -> ConflictResolution {
        if self.exact_entry_transitive_deps {
            ConflictResolution::exact
        } else {
            ConflictResolution::at_least
        }
    }

    pub fn generate_transitive_dep_constraint(
        &self,
    ) -> for<'a> fn(&'a MovePackage) -> ConflictResolution {
        if self.exact_transitive_deps {
            ConflictResolution::exact
        } else {
            ConflictResolution::at_least
        }
    }

    fn resolution_table_with_native_packages(
        &self,
        package_cache: &mut BTreeMap<ObjectID, MovePackage>,
        type_origin_map: &mut TypeOriginMap,
        store: &dyn PackageStore,
    ) -> Result<ResolutionTable, ExecutionError> {
        let mut resolution_table = ResolutionTable::empty();
        if self.always_include_system_packages {
            for id in NATIVE_PACKAGE_IDS {
                let package =
                    PTBLinkageResolver::get_package(package_cache, type_origin_map, id, store)?;
                debug_assert_eq!(package.id(), *id);
                debug_assert_eq!(package.original_package_id(), *id);
                resolution_table
                    .resolution_table
                    .insert(*id, ConflictResolution::Exact(package.version(), *id));
                resolution_table
                    .all_versions_resolution_table
                    .insert(*id, *id);
            }
        }

        Ok(resolution_table)
    }
}

impl ResolvedLinkage {
    fn from_resolution_table(resolution_table: ResolutionTable) -> Self {
        let mut linkage = BTreeMap::new();
        let mut versions = BTreeMap::new();
        for (runtime_id, resolution) in resolution_table.resolution_table {
            match resolution {
                ConflictResolution::Exact(version, object_id)
                | ConflictResolution::AtLeast(version, object_id) => {
                    linkage.insert(runtime_id, object_id);
                    versions.insert(runtime_id, version);
                }
            }
        }
        Self {
            linkage,
            linkage_resolution: resolution_table.all_versions_resolution_table,
            versions,
        }
    }

    pub fn linkage_context(&self) -> LinkageContext {
        LinkageContext::new(self.linkage.iter().map(|(k, v)| (**k, **v)).collect())
    }

    pub fn resolve_to_runtime_id(&self, object_id: &ObjectID) -> Option<ObjectID> {
        self.linkage_resolution.get(object_id).copied()
    }

    /// Given a module name and type name, resolve it to the defining package ID.
    /// The `module_address` can be _any_ valid referent of the package in question (i.e., any
    /// valid package ID for the package in question).
    pub fn resolve_type_to_defining_id(
        &self,
        resolver: &PTBLinkageResolver,
        module_address: ObjectID,
        module_name: String,
        type_name: String,
    ) -> Option<ObjectID> {
        let runtime_id = self.resolve_to_runtime_id(&module_address)?;
        let package_type_origins = resolver.type_origin_cache.get(&runtime_id)?;
        package_type_origins.get(&(module_name, type_name)).copied()
    }
}

impl ResolutionTable {
    pub fn empty() -> Self {
        Self {
            resolution_table: BTreeMap::new(),
            all_versions_resolution_table: BTreeMap::new(),
        }
    }
}

impl ConflictResolution {
    pub fn exact(pkg: &MovePackage) -> ConflictResolution {
        ConflictResolution::Exact(pkg.version(), pkg.id())
    }

    pub fn at_least(pkg: &MovePackage) -> ConflictResolution {
        ConflictResolution::AtLeast(pkg.version(), pkg.id())
    }

    pub fn unify(&self, other: &ConflictResolution) -> Result<ConflictResolution, ExecutionError> {
        match (&self, other) {
            // If we have two exact resolutions, they must be the same.
            (ConflictResolution::Exact(sv, self_id), ConflictResolution::Exact(ov, other_id)) => {
                if self_id != other_id || sv != ov {
                    Err(
                        ExecutionError::new_with_source(
                            ExecutionErrorKind::InvalidLinkage,
                            format!(
                                "exact/exact conflicting resolutions for package: linkage requires the same package \
                                 at different versions. Linkage requires exactly {self_id} (version {sv}) and \
                                 {other_id} (version {ov}) to be used in the same transaction"
                            )
                        )
                    )
                } else {
                    Ok(ConflictResolution::Exact(*sv, *self_id))
                }
            }
            // Take the max if you have two at least resolutions.
            (
                ConflictResolution::AtLeast(self_version, sid),
                ConflictResolution::AtLeast(other_version, oid),
            ) => {
                let id = if self_version > other_version {
                    *sid
                } else {
                    *oid
                };

                Ok(ConflictResolution::AtLeast(
                    *self_version.max(other_version),
                    id,
                ))
            }
            // If you unify an exact and an at least, the exact must be greater than or equal to
            // the at least. It unifies to an exact.
            (
                ConflictResolution::Exact(exact_version, exact_id),
                ConflictResolution::AtLeast(at_least_version, at_least_id),
            )
            | (
                ConflictResolution::AtLeast(at_least_version, at_least_id),
                ConflictResolution::Exact(exact_version, exact_id),
            ) => {
                if exact_version < at_least_version {
                    return Err(
                        ExecutionError::new_with_source(
                            ExecutionErrorKind::InvalidLinkage,
                            format!(
                                "Exact/AtLeast conflicting resolutions for package: linkage requires exactly this \
                                 package {exact_id} (version {exact_version}) and also at least the following \
                                 version of the package {at_least_id} at version {at_least_version}. However \
                                 {exact_id} is at version {exact_version} which is less than {at_least_version}."
                            )
                        )
                    );
                }

                Ok(ConflictResolution::Exact(*exact_version, *exact_id))
            }
        }
    }
}

impl PerCommandLinkage {
    pub fn new(
        always_include_system_packages: bool,
        binary_config: BinaryConfig,
        _store: &dyn PackageStore,
    ) -> Result<Self, ExecutionError> {
        let linkage_config =
            LinkageConfig::per_command_linkage_settings(always_include_system_packages);
        Ok(Self {
            internal: PTBLinkageResolver {
                package_cache: BTreeMap::new(),
                type_origin_cache: TypeOriginMap::new(),
                linkage_config,
                binary_config,
            },
        })
    }

    pub fn add_command(
        &mut self,
        command: &Command,
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        let mut unification_table = ResolutionTable {
            resolution_table: BTreeMap::new(),
            all_versions_resolution_table: BTreeMap::new(),
        };
        Ok(ResolvedLinkage::from_resolution_table(
            self.internal
                .add_command(command, store, &mut unification_table)?,
        ))
    }
}

impl UnifiedLinkage {
    pub fn new(
        always_include_system_packages: bool,
        binary_config: BinaryConfig,
        store: &dyn PackageStore,
    ) -> Result<Self, ExecutionError> {
        let linkage_config =
            LinkageConfig::unified_linkage_settings(always_include_system_packages);
        let mut package_cache = BTreeMap::new();
        let mut type_origin_cache = TypeOriginMap::new();
        let unification_table = linkage_config.resolution_table_with_native_packages(
            &mut package_cache,
            &mut type_origin_cache,
            store,
        )?;
        Ok(Self {
            internal: PTBLinkageResolver {
                package_cache,
                linkage_config,
                binary_config,
                type_origin_cache,
            },
            unification_table,
        })
    }

    pub fn add_command(
        &mut self,
        command: &Command,
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        Ok(ResolvedLinkage::from_resolution_table(
            self.internal
                .add_command(command, store, &mut self.unification_table)?,
        ))
    }
}

impl PTBLinkageResolver {
    pub fn type_linkage(
        &mut self,
        ids: &[ObjectID],
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        let mut resolution_table = ResolutionTable::empty();
        for id in ids {
            let pkg = Self::get_package(
                &mut self.package_cache,
                &mut self.type_origin_cache,
                id,
                store,
            )?;
            let transitive_deps = pkg
                .linkage_table()
                .values()
                .map(|info| info.upgraded_id)
                .collect::<Vec<_>>();
            let package_id = pkg.id();
            self.add_and_unify(
                &package_id,
                store,
                &mut resolution_table,
                ConflictResolution::at_least,
            )?;
            for object_id in transitive_deps.iter() {
                self.add_and_unify(
                    object_id,
                    store,
                    &mut resolution_table,
                    ConflictResolution::at_least,
                )?;
            }
        }

        Ok(ResolvedLinkage::from_resolution_table(resolution_table))
    }

    pub fn publication_linkage(
        &mut self,
        linkage: &LinkageContext,
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        let mut resolution_table = ResolutionTable::empty();
        for (runtime_id, package_id) in linkage.linkage_table.iter() {
            let package = PTBLinkageResolver::get_package(
                &mut self.package_cache,
                &mut self.type_origin_cache,
                &ObjectID::from(*package_id),
                store,
            )?;

            assert_eq!(*package.id(), *package_id);
            assert_eq!(*package.original_package_id(), *runtime_id);

            self.add_and_unify(
                &ObjectID::from(*runtime_id),
                store,
                &mut resolution_table,
                ConflictResolution::exact,
            )?;
        }
        Ok(ResolvedLinkage::from_resolution_table(resolution_table))
    }
}

impl PTBLinkageResolver {
    pub fn new(linkage_config: LinkageConfig, binary_config: BinaryConfig) -> Self {
        Self {
            package_cache: BTreeMap::new(),
            type_origin_cache: TypeOriginMap::new(),
            linkage_config,
            binary_config,
        }
    }

    fn add_command(
        &mut self,
        command: &Command,
        store: &dyn PackageStore,
        resolution_table: &mut ResolutionTable,
    ) -> Result<ResolutionTable, ExecutionError> {
        match command {
            Command::MoveCall(programmable_move_call) => {
                let pkg = Self::get_package(
                    &mut self.package_cache,
                    &mut self.type_origin_cache,
                    &programmable_move_call.package,
                    store,
                )?;
                let pkg_id = pkg.id();
                let transitive_deps = pkg
                    .linkage_table()
                    .values()
                    .map(|info| info.upgraded_id)
                    .collect::<Vec<_>>();

                let m = pkg
                    .deserialize_module_by_str(&programmable_move_call.module, &self.binary_config)
                    .map_err(|e| {
                        ExecutionError::new_with_source(
                            ExecutionErrorKind::VMVerificationOrDeserializationError,
                            e,
                        )
                    })?;
                let Some(fdef) = m.function_defs().iter().find(|f| {
                    m.identifier_at(m.function_handle_at(f.function).name)
                        .as_str()
                        == programmable_move_call.function
                }) else {
                    return Err(ExecutionError::new_with_source(
                        ExecutionErrorKind::FunctionNotFound,
                        format!(
                            "Could not resolve function '{}' in module '{}::{}'",
                            programmable_move_call.function,
                            programmable_move_call.package,
                            programmable_move_call.module
                        ),
                    ));
                };

                for ty in &programmable_move_call.type_arguments {
                    self.add_type_input(ty, store, resolution_table)?;
                }

                // Register function entrypoint
                if fdef.is_entry && fdef.visibility != Visibility::Public {
                    self.add_and_unify(
                        &pkg_id,
                        store,
                        resolution_table,
                        ConflictResolution::exact,
                    )?;

                    // transitive closure of entry functions are fixed
                    for object_id in transitive_deps.iter() {
                        self.add_and_unify(
                            object_id,
                            store,
                            resolution_table,
                            self.linkage_config
                                .generate_entry_transitive_dep_constraint(),
                        )?;
                    }
                } else {
                    self.add_and_unify(
                        &pkg_id,
                        store,
                        resolution_table,
                        self.linkage_config.generate_top_level_fn_constraint(),
                    )?;

                    // transitive closure of non-entry functions are at-least
                    for object_id in transitive_deps.iter() {
                        self.add_and_unify(
                            object_id,
                            store,
                            resolution_table,
                            self.linkage_config.generate_transitive_dep_constraint(),
                        )?;
                    }
                }
            }
            Command::MakeMoveVec(type_input, _) => {
                if let Some(ty) = type_input {
                    self.add_type_input(ty, store, resolution_table)?;
                }
            }
            Command::Upgrade(_, deps, _, _) | Command::Publish(_, deps) => {
                let mut resolution_table =
                    self.linkage_config.resolution_table_with_native_packages(
                        &mut self.package_cache,
                        &mut self.type_origin_cache,
                        store,
                    )?;
                for id in deps {
                    let pkg = Self::get_package(
                        &mut self.package_cache,
                        &mut self.type_origin_cache,
                        id,
                        store,
                    )?;
                    resolution_table.resolution_table.insert(
                        pkg.original_package_id(),
                        ConflictResolution::Exact(pkg.version(), pkg.id()),
                    );
                    resolution_table
                        .all_versions_resolution_table
                        .insert(pkg.id(), pkg.original_package_id());
                }
                return Ok(resolution_table);
            }
            Command::TransferObjects(_, _)
            | Command::SplitCoins(_, _)
            | Command::MergeCoins(_, _) => (),
        };

        Ok(resolution_table.clone())
    }

    fn add_type_input(
        &mut self,
        ty: &TypeInput,
        store: &dyn PackageStore,
        unification_table: &mut ResolutionTable,
    ) -> Result<(), ExecutionError> {
        let mut stack = vec![ty];
        while let Some(ty) = stack.pop() {
            match ty {
                TypeInput::Bool
                | TypeInput::U8
                | TypeInput::U64
                | TypeInput::U128
                | TypeInput::Address
                | TypeInput::Signer
                | TypeInput::U16
                | TypeInput::U32
                | TypeInput::U256 => (),
                TypeInput::Vector(type_input) => {
                    stack.push(&**type_input);
                }
                TypeInput::Struct(struct_input) => {
                    let sid = ObjectID::from(struct_input.address);
                    self.add_and_unify(
                        &sid,
                        store,
                        unification_table,
                        self.linkage_config.generate_type_constraint(),
                    )?;
                    let pkg = Self::get_package(
                        &mut self.package_cache,
                        &mut self.type_origin_cache,
                        &ObjectID::from(struct_input.address),
                        store,
                    )?;
                    let linkage_table = pkg
                        .linkage_table()
                        .values()
                        .map(|info| info.upgraded_id)
                        .collect::<Vec<_>>();
                    for dep_id in linkage_table {
                        self.add_and_unify(
                            &dep_id,
                            store,
                            unification_table,
                            self.linkage_config.generate_type_constraint(),
                        )?;
                    }
                    for ty in struct_input.type_params.iter() {
                        stack.push(ty);
                    }
                }
            }
        }
        Ok(())
    }

    fn get_package<'a>(
        package_cache: &'a mut BTreeMap<ObjectID, MovePackage>,
        type_origin_map: &mut TypeOriginMap,
        object_id: &ObjectID,
        store: &dyn PackageStore,
    ) -> Result<&'a MovePackage, ExecutionError> {
        // If we've cached too many packages, clear the cache. We'll windup pulling in any more
        // that we need if we need them again.
        if package_cache.len() > MAX_CACHED_PACKAGES {
            package_cache.clear();
        }

        if !package_cache.contains_key(object_id) {
            let package = store
                .get_package(object_id)
                .map_err(|e| {
                    ExecutionError::new_with_source(
                        ExecutionErrorKind::PublishUpgradeMissingDependency,
                        e,
                    )
                })?
                .ok_or_else(|| ExecutionError::from_kind(ExecutionErrorKind::InvalidLinkage))?;
            let original_package_id = package.original_package_id();
            let package_types = type_origin_map.entry(original_package_id).or_default();
            for ((module_name, type_name), defining_id) in package.type_origin_map().into_iter() {
                if let Some(other) = package_types.insert(
                    (module_name.to_string(), type_name.to_string()),
                    defining_id,
                ) {
                    assert_eq!(
                        other, defining_id,
                        "type origin map should never have conflicting entries"
                    );
                }
            }
            package_cache.insert(*object_id, package);
        }

        Ok(package_cache.get(object_id).expect("Guaranteed to exist"))
    }

    // Add a package to the unification table, unifying it with any existing package in the table.
    // Errors if the packages cannot be unified (e.g., if one is exact and the other is not).
    fn add_and_unify(
        &mut self,
        object_id: &ObjectID,
        store: &dyn PackageStore,
        resolution_table: &mut ResolutionTable,
        resolution_fn: fn(&MovePackage) -> ConflictResolution,
    ) -> Result<(), ExecutionError> {
        let package = Self::get_package(
            &mut self.package_cache,
            &mut self.type_origin_cache,
            object_id,
            store,
        )?;

        let resolution = resolution_fn(package);
        let original_pkg_id = package.original_package_id();

        match resolution_table.resolution_table.entry(original_pkg_id) {
            Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(resolution);
            }
            Entry::Occupied(mut occupied_entry) => {
                *occupied_entry.get_mut() = occupied_entry.get().unify(&resolution)?;
            }
        }

        if !resolution_table
            .all_versions_resolution_table
            .contains_key(object_id)
        {
            resolution_table
                .all_versions_resolution_table
                .insert(*object_id, original_pkg_id);
        }

        Ok(())
    }
}
