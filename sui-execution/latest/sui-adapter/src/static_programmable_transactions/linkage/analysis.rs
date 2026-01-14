// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::PackageStore,
    execution_mode::ExecutionMode,
    execution_value::ExecutionState,
    static_programmable_transactions::{
        linkage::{
            config::{LinkageConfig, ResolutionConfig},
            resolution::{ResolutionTable, VersionConstraint, add_and_unify, get_package},
            resolved_linkage::{ExecutableLinkage, ResolvedLinkage},
        },
        loading::ast::Type,
    },
};
use move_binary_format::file_format::Visibility;
use move_core_types::identifier::IdentStr;
use move_vm_runtime::validation::verification::ast::Package as VerifiedPackage;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::ObjectID,
    error::{ExecutionError, ExecutionErrorKind},
    transaction::ProgrammableTransaction,
};

#[derive(Debug)]
pub struct LinkageAnalyzer {
    internal: ResolutionConfig,
}

impl LinkageAnalyzer {
    pub fn new<Mode: ExecutionMode>(
        protocol_config: &ProtocolConfig,
    ) -> Result<Self, ExecutionError> {
        let always_include_system_packages = !Mode::packages_are_predefined();
        let linkage_config = LinkageConfig::legacy_linkage_settings(always_include_system_packages);
        let binary_config = protocol_config.binary_config(None);
        Ok(Self {
            internal: ResolutionConfig {
                linkage_config,
                binary_config,
            },
        })
    }

    pub fn compute_call_linkage(
        &self,
        package: &ObjectID,
        module_name: &IdentStr,
        function_name: &IdentStr,
        type_args: &[Type],
        store: &dyn PackageStore,
    ) -> Result<ExecutableLinkage, ExecutionError> {
        Ok(ExecutableLinkage::new(
            ResolvedLinkage::from_resolution_table(self.compute_call_linkage_(
                package,
                module_name,
                function_name,
                type_args,
                store,
            )?),
        ))
    }

    pub fn compute_publication_linkage(
        &self,
        deps: &[ObjectID],
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        Ok(ResolvedLinkage::from_resolution_table(
            self.compute_publication_linkage_(deps, store)?,
        ))
    }

    pub fn config(&self) -> &ResolutionConfig {
        &self.internal
    }

    pub fn compute_input_type_resolution_linkage(
        &self,
        tx: &ProgrammableTransaction,
        package_store: &dyn PackageStore,
        object_store: &dyn ExecutionState,
    ) -> Result<ExecutableLinkage, ExecutionError> {
        input_type_resolution_analysis::compute_resolution_linkage(
            self,
            tx,
            package_store,
            object_store,
        )
    }

    fn compute_call_linkage_(
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
            self_resolution_fn: fn(&VerifiedPackage) -> Option<VersionConstraint>,
            dep_resolution_fn: fn(&VerifiedPackage) -> Option<VersionConstraint>,
        ) -> Result<(), ExecutionError> {
            let pkg = get_package(object_id, store)?;
            let transitive_deps = pkg.linkage_table().values().copied().map(ObjectID::from);
            for object_id in transitive_deps {
                add_and_unify(&object_id, store, resolution_table, dep_resolution_fn)?;
            }
            add_and_unify(object_id, store, resolution_table, self_resolution_fn)?;
            Ok(())
        }

        let pkg = get_package(package, store)?;
        let fn_not_found_err = || {
            ExecutionError::new_with_source(
                ExecutionErrorKind::FunctionNotFound,
                format!(
                    "Could not resolve function '{}' in module '{}::{}'",
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
            Visibility::Public => VersionConstraint::at_least,
            Visibility::Private | Visibility::Friend => VersionConstraint::exact,
        };

        add_package(
            package,
            store,
            &mut resolution_table,
            VersionConstraint::exact,
            dep_resolution_fn,
        )?;

        for type_defining_id in type_args.iter().flat_map(|ty| ty.all_addresses()) {
            // Type arguments are "at least" constraints
            add_package(
                &ObjectID::from(type_defining_id),
                store,
                &mut resolution_table,
                VersionConstraint::at_least,
                VersionConstraint::at_least,
            )?;
        }

        Ok(resolution_table)
    }

    /// Compute the linkage for a publish or upgrade command. This is a special case because
    fn compute_publication_linkage_(
        &self,
        deps: &[ObjectID],
        store: &dyn PackageStore,
    ) -> Result<ResolutionTable, ExecutionError> {
        let mut resolution_table = self
            .internal
            .linkage_config
            .resolution_table_with_native_packages(store)?;
        for id in deps {
            add_and_unify(id, store, &mut resolution_table, VersionConstraint::exact)?;
        }
        Ok(resolution_table)
    }
}

mod input_type_resolution_analysis {
    use crate::{
        data_store::PackageStore,
        execution_value::ExecutionState,
        static_programmable_transactions::linkage::{
            analysis::LinkageAnalyzer,
            resolution::ResolutionTable,
            resolved_linkage::{ExecutableLinkage, ResolvedLinkage},
        },
    };
    use move_core_types::language_storage::StructTag;
    use sui_types::{
        base_types::ObjectID,
        error::{ExecutionError, ExecutionErrorKind},
        transaction::{
            CallArg, Command, FundsWithdrawalArg, ObjectArg, ProgrammableMoveCall,
            ProgrammableTransaction, WithdrawalTypeArg,
        },
        type_input::TypeInput,
    };

    pub(super) fn compute_resolution_linkage(
        analyzer: &LinkageAnalyzer,
        tx: &ProgrammableTransaction,
        package_store: &dyn PackageStore,
        object_store: &dyn ExecutionState,
    ) -> Result<ExecutableLinkage, ExecutionError> {
        let ProgrammableTransaction { inputs, commands } = tx;

        let mut resolution_table = analyzer
            .internal
            .linkage_config
            .resolution_table_with_native_packages(package_store)?;
        for arg in inputs.iter() {
            input(&mut resolution_table, arg, package_store, object_store)?;
        }

        for cmd in commands.iter() {
            command(&mut resolution_table, cmd, package_store)?;
        }

        Ok(ExecutableLinkage::new(
            ResolvedLinkage::from_resolution_table(resolution_table),
        ))
    }

    fn input(
        resolution_table: &mut ResolutionTable,
        arg: &CallArg,
        package_store: &dyn PackageStore,
        object_store: &dyn ExecutionState,
    ) -> Result<(), ExecutionError> {
        match arg {
            CallArg::Pure(_) | CallArg::Object(ObjectArg::Receiving(_)) => (),
            CallArg::Object(
                ObjectArg::ImmOrOwnedObject((id, _, _)) | ObjectArg::SharedObject { id, .. },
            ) => {
                let Some(obj) = object_store.read_object(id) else {
                    invariant_violation!("Object {:?} not found in object store", id);
                };
                let Some(ty) = obj.type_() else {
                    invariant_violation!("Object {:?} has does not have a Move type", id);
                };

                // invariant: the addresses in the type are defining addresses for the types since
                // these are the types of the objects as stored on-chain.
                let tag: StructTag = ty.clone().into();
                let ids = tag.all_addresses().into_iter().map(ObjectID::from);
                resolution_table.add_type_linkages_to_table(ids, package_store)?;
            }
            CallArg::FundsWithdrawal(f) => {
                let FundsWithdrawalArg { type_arg, .. } = f;
                match type_arg {
                    WithdrawalTypeArg::Balance(inner) => {
                        let Ok(tag) = inner.to_type_tag() else {
                            invariant_violation!(
                                "Invalid type tag in funds withdrawal argument: {:?}",
                                inner
                            );
                        };
                        let ids = tag.all_addresses().into_iter().map(ObjectID::from);
                        resolution_table.add_type_linkages_to_table(ids, package_store)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn command(
        resolution_table: &mut ResolutionTable,
        command: &Command,
        package_store: &dyn PackageStore,
    ) -> Result<(), ExecutionError> {
        let mut add_ty_input = |ty: &TypeInput| {
            let tag = ty.to_type_tag().map_err(|e| {
                ExecutionError::new_with_source(
                    ExecutionErrorKind::InvalidLinkage,
                    format!("Invalid type tag in move call argument: {:?}", e),
                )
            })?;
            let ids = tag.all_addresses().into_iter().map(ObjectID::from);
            resolution_table.add_type_linkages_to_table(ids, package_store)
        };
        match command {
            Command::MoveCall(pmc) => {
                let ProgrammableMoveCall {
                    package,
                    type_arguments,
                    ..
                } = &**pmc;
                type_arguments.iter().try_for_each(add_ty_input)?;
                resolution_table.add_type_linkages_to_table([*package], package_store)?;
            }
            Command::MakeMoveVec(Some(ty), _) => {
                add_ty_input(ty)?;
            }
            Command::MakeMoveVec(None, _)
            | Command::TransferObjects(_, _)
            | Command::SplitCoins(_, _)
            | Command::MergeCoins(_, _) => (),
            Command::Publish(_, object_ids) => {
                resolution_table.add_type_linkages_to_table(object_ids, package_store)?;
            }
            Command::Upgrade(_, object_ids, object_id, _) => {
                resolution_table.add_type_linkages_to_table([*object_id], package_store)?;
                resolution_table.add_type_linkages_to_table(object_ids, package_store)?;
            }
        }

        Ok(())
    }
}
