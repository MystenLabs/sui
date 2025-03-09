// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{binary_config::BinaryConfig, file_format::Visibility};
use std::collections::BTreeMap;
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    error::{ExecutionError, ExecutionErrorKind, SuiResult, UserInputError},
    move_package::MovePackage,
    storage::BackingPackageStore,
    transaction::{Command, ProgrammableTransaction},
    type_input::TypeInput,
};

/// Max number of packages to cache in the PTBLinkageMetadata. If we have more than this, we'll
/// drop the cache and restart the cache.
const MAX_CACHED_PACKAGES: usize = 200;

/// Metadata for the PTB linkage analysis.
#[derive(Debug)]
pub struct PTBLinkageMetadata {
    /// Config to use for the linkage analysis.
    pub linkage_config: LinkageConfig,
    /// Config to use for the binary analysis (needed for deserialization to determine if a
    /// function is a non-public entry function).
    pub binary_config: BinaryConfig,
    /// Cache for packages that we've loaded so far. Note: We may drop this cache if it grows too
    /// large.
    pub all_packages: BTreeMap<ObjectID, MovePackage>,
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
}

/// Unifiers. These are used to determine how to unify two packages.
#[derive(Debug)]
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

pub type ResolvedLinkage = BTreeMap<ObjectID, ObjectID>;

#[derive(Debug)]
pub struct PerCommandLinkage {
    internal: PTBLinkageMetadata,
}

#[derive(Debug)]
pub struct UnifiedLinkage {
    /// Current unification table we have for packages. This is a mapping from the original
    /// package ID for a package to its current resolution. This is the "constraint set" that we
    /// are building/solving as we progress across the PTB.
    unification_table: BTreeMap<ObjectID, ConflictResolution>,
    internal: PTBLinkageMetadata,
}

pub trait LinkageAnalysis {
    fn add_command(
        &mut self,
        command: &Command,
        store: &dyn BackingPackageStore,
    ) -> SuiResult<ResolvedLinkage>;
}

impl LinkageAnalysis for PerCommandLinkage {
    fn add_command(
        &mut self,
        command: &Command,
        store: &dyn BackingPackageStore,
    ) -> SuiResult<ResolvedLinkage> {
        self.add_command(command, store)
    }
}

impl LinkageAnalysis for UnifiedLinkage {
    fn add_command(
        &mut self,
        command: &Command,
        store: &dyn BackingPackageStore,
    ) -> SuiResult<ResolvedLinkage> {
        self.add_command(command, store)
    }
}

impl LinkageConfig {
    pub fn unified_linkage_settings() -> Self {
        Self {
            fix_top_level_functions: true,
            fix_types: false,
            exact_entry_transitive_deps: false,
            exact_transitive_deps: false,
        }
    }

    pub fn per_command_linkage_settings() -> Self {
        Self {
            fix_top_level_functions: true,
            fix_types: false,
            exact_entry_transitive_deps: true,
            exact_transitive_deps: true,
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
}

impl ConflictResolution {
    pub fn exact<'a>(pkg: &MovePackage) -> ConflictResolution {
        ConflictResolution::Exact(pkg.version(), pkg.id())
    }

    pub fn at_least<'a>(pkg: &MovePackage) -> ConflictResolution {
        ConflictResolution::AtLeast(pkg.version(), pkg.id())
    }

    pub fn unify(&self, other: &ConflictResolution) -> SuiResult<ConflictResolution> {
        match (&self, other) {
            // If we have two exact resolutions, they must be the same.
            (ConflictResolution::Exact(sv, self_id), ConflictResolution::Exact(ov, other_id)) => {
                if self_id != other_id || sv != ov {
                    return Err(
                        ExecutionError::new_with_source(
                            ExecutionErrorKind::InvalidUnifiedLinkage,
                            format!(
                                "exact/exact conflicting resolutions for package: linkage requires the same package \
                                 at different versions. Linkage requires exactly {self_id} (version {sv}) and \
                                 {other_id} (version {ov}) to be used in the same transaction"
                            )
                        ).into()
                    );
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
                            ExecutionErrorKind::InvalidUnifiedLinkage,
                            format!(
                                "Exact/AtLeast conflicting resolutions for package: linkage requires exactly this \
                                 package {exact_id} (version {exact_version}) and also at least the following \
                                 version of the package {at_least_id} at version {at_least_version}. However \
                                 {exact_id} is at version {exact_version} which is less than {at_least_version}."
                            )
                        ).into()
                    );
                }

                Ok(ConflictResolution::Exact(*exact_version, *exact_id))
            }
        }
    }
}

impl PerCommandLinkage {
    pub fn new(binary_config: BinaryConfig) -> Self {
        Self {
            internal: PTBLinkageMetadata {
                all_packages: BTreeMap::new(),
                linkage_config: LinkageConfig::per_command_linkage_settings(),
                binary_config,
            },
        }
    }

    pub fn add_command(
        &mut self,
        command: &Command,
        store: &dyn BackingPackageStore,
    ) -> SuiResult<ResolvedLinkage> {
        let mut unification_table = BTreeMap::new();
        self.internal
            .add_command(command, store, &mut unification_table)
    }

    pub fn from_ptb_for_testing_only(
        ptb: &ProgrammableTransaction,
        store: &dyn BackingPackageStore,
        binary_config: BinaryConfig,
    ) -> SuiResult<()> {
        let mut u = Self::new(binary_config);

        for command in &ptb.commands {
            u.add_command(command, store)?;
        }

        Ok(())
    }
}

impl UnifiedLinkage {
    pub fn new(binary_config: BinaryConfig) -> Self {
        Self {
            internal: PTBLinkageMetadata {
                all_packages: BTreeMap::new(),
                linkage_config: LinkageConfig::unified_linkage_settings(),
                binary_config,
            },
            unification_table: BTreeMap::new(),
        }
    }

    pub fn add_command(
        &mut self,
        command: &Command,
        store: &dyn BackingPackageStore,
    ) -> SuiResult<ResolvedLinkage> {
        self.internal
            .add_command(command, store, &mut self.unification_table)
    }

    pub fn from_ptb_for_testing_only(
        ptb: &ProgrammableTransaction,
        store: &dyn BackingPackageStore,
        binary_config: BinaryConfig,
    ) -> SuiResult<()> {
        let mut u = Self::new(binary_config);

        for command in &ptb.commands {
            u.add_command(command, store)?;
        }

        Ok(())
    }
}

impl PTBLinkageMetadata {
    pub fn new(linkage_config: LinkageConfig, binary_config: BinaryConfig) -> SuiResult<Self> {
        Ok(Self {
            all_packages: BTreeMap::new(),
            linkage_config,
            binary_config,
        })
    }

    pub(crate) fn add_command(
        &mut self,
        command: &Command,
        store: &dyn BackingPackageStore,
        unification_table: &mut BTreeMap<ObjectID, ConflictResolution>,
    ) -> SuiResult<ResolvedLinkage> {
        match command {
            Command::MoveCall(programmable_move_call) => {
                let pkg = Self::get_package(
                    &mut self.all_packages,
                    &programmable_move_call.package,
                    store,
                )?;
                let pkg_id = pkg.id();
                let transitive_deps = pkg
                    .linkage_table()
                    .values()
                    .map(|info| info.upgraded_id)
                    .collect::<Vec<_>>();

                let m = pkg.deserialize_module_by_name(
                    &programmable_move_call.module,
                    &self.binary_config,
                )?;
                let Some(fdef) = m.function_defs().into_iter().find(|f| {
                    m.identifier_at(m.function_handle_at(f.function).name)
                        .as_str()
                        == &programmable_move_call.function
                }) else {
                    return Err(ExecutionError::new_with_source(
                        ExecutionErrorKind::FunctionNotFound,
                        format!(
                            "Function {} not found in module {}",
                            programmable_move_call.function, programmable_move_call.module
                        ),
                    )
                    .into());
                };

                for ty in &programmable_move_call.type_arguments {
                    self.add_type(ty, store, unification_table)?;
                }

                // Register function entrypoint
                if fdef.is_entry && fdef.visibility != Visibility::Public {
                    self.add_and_unify(
                        &pkg_id,
                        store,
                        unification_table,
                        ConflictResolution::exact,
                    )?;

                    // transitive closure of entry functions are fixed
                    for object_id in transitive_deps.iter() {
                        self.add_and_unify(
                            object_id,
                            store,
                            unification_table,
                            self.linkage_config
                                .generate_entry_transitive_dep_constraint(),
                        )?;
                    }
                } else {
                    self.add_and_unify(
                        &pkg_id,
                        store,
                        unification_table,
                        self.linkage_config.generate_top_level_fn_constraint(),
                    )?;

                    // transitive closure of non-entry functions are at-least
                    for object_id in transitive_deps.iter() {
                        self.add_and_unify(
                            object_id,
                            store,
                            unification_table,
                            self.linkage_config.generate_transitive_dep_constraint(),
                        )?;
                    }
                }
            }
            Command::MakeMoveVec(type_input, _) => {
                if let Some(ty) = type_input {
                    self.add_type(ty, store, unification_table)?;
                }
            }
            // Upgrades and Publishes don't count toward the global linkage determination.
            Command::Upgrade(_, _, _, _)
            | Command::Publish(_, _)
            | Command::TransferObjects(_, _)
            | Command::SplitCoins(_, _)
            | Command::MergeCoins(_, _) => (),
        };

        Ok(unification_table
            .iter()
            .map(|(k, v)| match v {
                ConflictResolution::Exact(_, object_id)
                | ConflictResolution::AtLeast(_, object_id) => (*k, *object_id),
            })
            .collect())
    }

    fn add_type(
        &mut self,
        ty: &TypeInput,
        store: &dyn BackingPackageStore,
        unification_table: &mut BTreeMap<ObjectID, ConflictResolution>,
    ) -> SuiResult<()> {
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
                    self.add_and_unify(
                        &ObjectID::from(struct_input.address),
                        store,
                        unification_table,
                        self.linkage_config.generate_type_constraint(),
                    )?;
                    for ty in struct_input.type_params.iter() {
                        stack.push(ty);
                    }
                }
            }
        }
        Ok(())
    }

    fn get_package<'a>(
        all_packages: &'a mut BTreeMap<ObjectID, MovePackage>,
        object_id: &ObjectID,
        store: &dyn BackingPackageStore,
    ) -> SuiResult<&'a MovePackage> {
        // If we've cached too many packages, clear the cache. We'll windup pulling in any more
        // that we need if we need them again.
        if all_packages.len() > MAX_CACHED_PACKAGES {
            all_packages.clear();
        }

        if !all_packages.contains_key(object_id) {
            let package = store
                .get_package_object(object_id)?
                .ok_or_else(|| UserInputError::ObjectNotFound {
                    object_id: *object_id,
                    version: None,
                })?
                .move_package()
                .clone();
            all_packages.insert(*object_id, package);
        }

        Ok(all_packages.get(object_id).expect("Guaranteed to exist"))
    }

    // Add a package to the unification table, unifying it with any existing package in the table.
    // Errors if the packages cannot be unified (e.g., if one is exact and the other is not).
    fn add_and_unify(
        &mut self,
        object_id: &ObjectID,
        store: &dyn BackingPackageStore,
        unification_table: &mut BTreeMap<ObjectID, ConflictResolution>,
        resolution_fn: fn(&MovePackage) -> ConflictResolution,
    ) -> SuiResult<()> {
        let package = Self::get_package(&mut self.all_packages, object_id, store)?;

        let resolution = resolution_fn(package);
        let original_pkg_id = package.original_package_id();

        if unification_table.contains_key(&original_pkg_id) {
            let existing_unifier = unification_table
                .get_mut(&original_pkg_id)
                .expect("Guaranteed to exist");
            *existing_unifier = existing_unifier.unify(&resolution)?;
        } else {
            unification_table.insert(original_pkg_id, resolution);
        }

        Ok(())
    }
}
