// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::PackageStore,
    linkage::analysis::{
        add_and_unify, get_package,
        resolution::{ConflictResolution, ResolutionTable},
    },
};
use move_binary_format::{binary_config::BinaryConfig, file_format::Visibility};
use sui_types::{
    MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID, SUI_SYSTEM_PACKAGE_ID,
    base_types::ObjectID,
    error::{ExecutionError, ExecutionErrorKind},
    move_package::MovePackage,
    transaction as P,
    type_input::TypeInput,
};

/// These are the set of native packages in Sui -- importantly they can be used implicitly by
/// different parts of the system and are not required to be explicitly imported always.
/// Additionally, there is no versioning concerns around these as they are "stable" for a given
/// epoch, and are the special packages that are always available, and updated in-place.
const NATIVE_PACKAGE_IDS: &[ObjectID] = &[
    SUI_FRAMEWORK_PACKAGE_ID,
    SUI_SYSTEM_PACKAGE_ID,
    MOVE_STDLIB_PACKAGE_ID,
];

/// Metadata and shared operations for the PTB linkage analysis.
#[derive(Debug)]
pub struct ResolutionConfig {
    /// Config to use for the linkage analysis.
    pub linkage_config: LinkageConfig,
    /// Config to use for the binary analysis (needed for deserialization to determine if a
    /// function is a non-public entry function).
    pub binary_config: BinaryConfig,
}

/// Configuration for the linkage analysis.
#[derive(Debug)]
pub struct LinkageConfig {
    /// Do we introduce an `Exact(<pkg_id>)` for each top-level function <pkg_id>::mname::fname?
    pub fix_top_level_functions: bool,
    /// Do we introduce an `Exact(<pkg_id>)` for each type <pkg_id>::mname::tname?
    pub type_argument_config: TypeArgumentConfig,
    /// Do we introduce an `Exact(<pkg_id>)` for each transitive dependency of a non-public entry function?
    pub exact_entry_transitive_deps: bool,
    /// Do we introduce an `Exact(<pkg>)` for each transitive dependency of a
    /// function?
    pub exact_transitive_deps: bool,
    /// Whether system packages should always be included as a member in the generated linkage.
    /// This is almost always true except for system transactions and genesis transactions.
    pub always_include_system_packages: bool,
}

#[derive(Debug)]
pub enum TypeArgumentConfig {
    /// Do not fix type arguments, but they contribute to the computed linkage as
    /// `AtLeast(<pkg_id>)`.
    AtLeast,
    /// Fix type arguments, i.e., introduce an `Exact(<pkg_id>)` for each type argument.
    Exact,
    /// Do not fix type arguments, and they do not contribute to the computed linkage.
    None,
}

impl ResolutionConfig {
    pub fn new(linkage_config: LinkageConfig, binary_config: BinaryConfig) -> Self {
        Self {
            linkage_config,
            binary_config,
        }
    }

    /// Add a comand to the linkage analysis, and add any constraints it introduces to the
    /// `ResolutionTable`. What constraints are added will depend on the command being added, and
    /// the linkage config.
    pub(crate) fn add_command(
        &self,
        command: &P::Command,
        store: &dyn PackageStore,
        resolution_table: &mut ResolutionTable,
    ) -> Result<ResolutionTable, ExecutionError> {
        match command {
            P::Command::MoveCall(programmable_move_call) => {
                let pkg = get_package(&programmable_move_call.package, store)?;
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
                    add_and_unify(&pkg_id, store, resolution_table, ConflictResolution::exact)?;

                    // transitive closure of entry functions are fixed
                    for object_id in transitive_deps.iter() {
                        add_and_unify(
                            object_id,
                            store,
                            resolution_table,
                            self.linkage_config
                                .generate_entry_transitive_dep_constraint(),
                        )?;
                    }
                } else {
                    add_and_unify(
                        &pkg_id,
                        store,
                        resolution_table,
                        self.linkage_config.generate_top_level_fn_constraint(),
                    )?;

                    // transitive closure of non-entry functions are at-least
                    for object_id in transitive_deps.iter() {
                        add_and_unify(
                            object_id,
                            store,
                            resolution_table,
                            self.linkage_config.generate_transitive_dep_constraint(),
                        )?;
                    }
                }
            }
            P::Command::MakeMoveVec(type_input, _) => {
                if let Some(ty) = type_input {
                    self.add_type_input(ty, store, resolution_table)?;
                }
            }
            P::Command::Upgrade(_, deps, _, _) | P::Command::Publish(_, deps) => {
                let mut resolution_table = self
                    .linkage_config
                    .resolution_table_with_native_packages(store)?;
                for id in deps {
                    let pkg = get_package(id, store)?;
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
            P::Command::TransferObjects(_, _)
            | P::Command::SplitCoins(_, _)
            | P::Command::MergeCoins(_, _) => (),
        };

        Ok(resolution_table.clone())
    }

    /// Add a type input to the linkage analysis, and add any constraints it introduces to the
    /// `ResolutionTable`. Note that the constraints added for the types will depend on the linkage
    /// config, and types may introduce an `at_least` constraint or an `exact` constraint.
    fn add_type_input(
        &self,
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
                    add_and_unify(
                        &sid,
                        store,
                        unification_table,
                        self.linkage_config.generate_type_constraint(),
                    )?;
                    let pkg = get_package(&ObjectID::from(struct_input.address), store)?;
                    let linkage_table = pkg
                        .linkage_table()
                        .values()
                        .map(|info| info.upgraded_id)
                        .collect::<Vec<_>>();
                    for dep_id in linkage_table {
                        add_and_unify(
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
}

impl LinkageConfig {
    pub fn unified_linkage_settings(always_include_system_packages: bool) -> Self {
        Self {
            fix_top_level_functions: true,
            type_argument_config: TypeArgumentConfig::AtLeast,
            exact_entry_transitive_deps: false,
            exact_transitive_deps: false,
            always_include_system_packages,
        }
    }

    pub fn per_command_linkage_settings(always_include_system_packages: bool) -> Self {
        Self {
            fix_top_level_functions: true,
            type_argument_config: TypeArgumentConfig::AtLeast,
            exact_entry_transitive_deps: true,
            exact_transitive_deps: true,
            always_include_system_packages,
        }
    }

    pub fn legacy_linkage_settings(always_include_system_packages: bool) -> Self {
        Self {
            fix_top_level_functions: true,
            type_argument_config: TypeArgumentConfig::None,
            exact_entry_transitive_deps: true,
            exact_transitive_deps: true,
            always_include_system_packages,
        }
    }

    pub fn generate_top_level_fn_constraint(
        &self,
    ) -> for<'a> fn(&'a MovePackage) -> Option<ConflictResolution> {
        if self.fix_top_level_functions {
            ConflictResolution::exact
        } else {
            ConflictResolution::at_least
        }
    }

    pub fn generate_type_constraint(
        &self,
    ) -> for<'a> fn(&'a MovePackage) -> Option<ConflictResolution> {
        match self.type_argument_config {
            TypeArgumentConfig::AtLeast => ConflictResolution::at_least,
            TypeArgumentConfig::Exact => ConflictResolution::exact,
            TypeArgumentConfig::None => ConflictResolution::no_constraint,
        }
    }

    pub fn generate_entry_transitive_dep_constraint(
        &self,
    ) -> for<'a> fn(&'a MovePackage) -> Option<ConflictResolution> {
        if self.exact_entry_transitive_deps {
            ConflictResolution::exact
        } else {
            ConflictResolution::at_least
        }
    }

    pub fn generate_transitive_dep_constraint(
        &self,
    ) -> for<'a> fn(&'a MovePackage) -> Option<ConflictResolution> {
        if self.exact_transitive_deps {
            ConflictResolution::exact
        } else {
            ConflictResolution::at_least
        }
    }

    pub(crate) fn resolution_table_with_native_packages(
        &self,
        store: &dyn PackageStore,
    ) -> Result<ResolutionTable, ExecutionError> {
        let mut resolution_table = ResolutionTable::empty();
        if self.always_include_system_packages {
            for id in NATIVE_PACKAGE_IDS {
                let package = get_package(id, store)?;
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
