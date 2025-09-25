// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::PackageStore,
    static_programmable_transactions::{
        linkage::{
            analysis::LinkageAnalysis,
            config::{LinkageConfig, ResolutionConfig},
            resolution::{ConflictResolution, ResolutionTable, add_and_unify, get_package},
            resolved_linkage::{ExecutableLinkage, ResolvedLinkage},
        },
        loading::ast::Type,
    },
};
use move_binary_format::{binary_config::BinaryConfig, file_format::Visibility};
use move_core_types::identifier::IdentStr;
use move_vm_runtime::validation::verification::ast::Package as VerifiedPackage;
use sui_types::{
    base_types::ObjectID,
    error::{ExecutionError, ExecutionErrorKind},
};

#[derive(Debug)]
pub struct LegacyLinkage {
    internal: ResolutionConfig,
}

impl LinkageAnalysis for LegacyLinkage {
    fn compute_call_linkage(
        &self,
        package: &ObjectID,
        module_name: &IdentStr,
        function_name: &IdentStr,
        type_args: &[Type],
        store: &dyn PackageStore,
    ) -> Result<ExecutableLinkage, ExecutionError> {
        Ok(ExecutableLinkage::new(
            ResolvedLinkage::from_resolution_table(self.compute_call_linkage(
                package,
                module_name,
                function_name,
                type_args,
                store,
            )?),
        ))
    }

    fn compute_publication_linkage(
        &self,
        deps: &[ObjectID],
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        Ok(ResolvedLinkage::from_resolution_table(
            self.compute_publication_linkage(deps, store)?,
        ))
    }

    fn config(&self) -> &ResolutionConfig {
        &self.internal
    }
}

impl LegacyLinkage {
    #[allow(dead_code)]
    pub fn new(
        always_include_system_packages: bool,
        binary_config: BinaryConfig,
        _store: &dyn PackageStore,
    ) -> Result<Self, ExecutionError> {
        let linkage_config = LinkageConfig::legacy_linkage_settings(always_include_system_packages);
        Ok(Self {
            internal: ResolutionConfig {
                linkage_config,
                binary_config,
            },
        })
    }

    fn compute_call_linkage(
        &self,
        package: &ObjectID,
        module_name: &IdentStr,
        function_name: &IdentStr,
        type_args: &[Type],
        store: &dyn PackageStore,
    ) -> Result<ResolutionTable, ExecutionError> {
        let mut resolution_table = self
            .internal
            .linkage_config
            .resolution_table_with_native_packages(store)?;

        fn add_package(
            object_id: &ObjectID,
            store: &dyn PackageStore,
            resolution_table: &mut ResolutionTable,
            self_resolution_fn: fn(&VerifiedPackage) -> Option<ConflictResolution>,
            dep_resolution_fn: fn(&VerifiedPackage) -> Option<ConflictResolution>,
        ) -> Result<(), ExecutionError> {
            let pkg = get_package(object_id, store)?;
            let transitive_deps = pkg
                .linkage_table()
                .values()
                .map(|version_id| ObjectID::from(*version_id))
                .collect::<Vec<_>>();
            for object_id in transitive_deps.iter() {
                add_and_unify(object_id, store, resolution_table, dep_resolution_fn)?;
            }
            add_and_unify(object_id, store, resolution_table, self_resolution_fn)?;
            Ok(())
        }

        let pkg = get_package(package, store)?;
        let fn_not_found_err = || {
            ExecutionError::new_with_source(
                ExecutionErrorKind::FunctionNotFound,
                format!(
                    "Could not resolve function '{}' in module {}::{}",
                    function_name, package, module_name
                ),
            )
        };
        let fdef = pkg
            .modules()
            .iter()
            .find(|m| m.0.name() == module_name)
            .ok_or_else(fn_not_found_err)?
            .1
            .compiled_module()
            .find_function_def_by_name(function_name.as_str())
            .ok_or_else(fn_not_found_err)?;

        let dep_resolution_fn = match fdef.1.visibility {
            Visibility::Public => ConflictResolution::at_least,
            Visibility::Private | Visibility::Friend => ConflictResolution::exact,
        };

        add_package(
            package,
            store,
            &mut resolution_table,
            ConflictResolution::exact,
            dep_resolution_fn,
        )?;

        for type_defining_id in type_args.iter().flat_map(|ty| ty.all_addresses()) {
            // Type arguments are "at least" constraints
            add_package(
                &ObjectID::from(type_defining_id),
                store,
                &mut resolution_table,
                ConflictResolution::at_least,
                ConflictResolution::at_least,
            )?;
        }

        Ok(resolution_table)
    }

    /// Compute the linkage for a publish or upgrade command. This is a special case because
    pub(crate) fn compute_publication_linkage(
        &self,
        deps: &[ObjectID],
        store: &dyn PackageStore,
    ) -> Result<ResolutionTable, ExecutionError> {
        let mut resolution_table = self
            .internal
            .linkage_config
            .resolution_table_with_native_packages(store)?;
        for id in deps {
            let pkg = get_package(id, store)?;
            add_and_unify(id, store, &mut resolution_table, ConflictResolution::exact)?;
            resolution_table
                .all_versions_resolution_table
                .insert(pkg.version_id().into(), pkg.original_id().into());
        }
        Ok(resolution_table)
    }
}
