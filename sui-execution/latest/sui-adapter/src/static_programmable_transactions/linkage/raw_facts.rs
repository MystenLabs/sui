// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use sui_types::{
    base_types::ObjectID,
    error::{SuiError, SuiResult},
    transaction::{Command, ProgrammableTransaction},
    type_input::TypeInput,
};

use super::{
    backing_package_metadata_store::{BackingPackageMetadata, BackingPackageMetadataStore},
    facts::{LinkageCommandFacts, LinkageFacts, ModuleInitFacts},
};
use sui_verifier::INIT_FN_NAME;

pub(crate) fn linkage_facts_from_programmable_transaction(
    pt: &ProgrammableTransaction,
    package_store: &BackingPackageMetadataStore<'_>,
) -> SuiResult<Vec<LinkageCommandFacts>> {
    pt.commands
        .iter()
        .map(|command| linkage_facts_from_command(command, package_store))
        .collect()
}

fn linkage_facts_from_command(
    command: &Command,
    package_store: &BackingPackageMetadataStore<'_>,
) -> SuiResult<LinkageCommandFacts> {
    match command {
        Command::MoveCall(move_call) => {
            let package = required_package(package_store, &move_call.package)?;
            let modules = package.modules()?;
            let module = modules
                .get(&move_call.module)
                .ok_or_else(|| missing_function(&move_call.module, &move_call.function))?;
            let function = module
                .function_defs()
                .iter()
                .find(|function_definition| {
                    let handle = module.function_handle_at(function_definition.function);
                    module.identifier_at(handle.name).as_str() == move_call.function
                })
                .ok_or_else(|| missing_function(&move_call.module, &move_call.function))?;
            let type_defining_ids = move_call
                .type_arguments
                .iter()
                .map(|type_input| resolve_type_input(package_store, type_input))
                .collect::<SuiResult<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect();

            Ok(LinkageCommandFacts::MoveCall {
                package: move_call.package,
                visibility: function.visibility,
                type_defining_ids,
            })
        }
        Command::Publish(serialized_modules, dependencies) => {
            let modules = package_store.deserialize_modules(serialized_modules)?;
            Ok(LinkageCommandFacts::Publish {
                has_init: modules.iter().any(module_has_init),
                linkage: publication_linkage(package_store, dependencies)?,
            })
        }
        Command::Upgrade(serialized_modules, dependencies, current_package_id, _) => {
            let current_package = required_package(package_store, current_package_id)?;
            let current_module_inits = current_package_module_inits(&current_package)?;
            let new_modules = package_store.deserialize_modules(serialized_modules)?;
            Ok(LinkageCommandFacts::Upgrade {
                current_package_id: *current_package_id,
                current_module_inits,
                new_modules,
                linkage: publication_linkage(package_store, dependencies)?,
            })
        }
        Command::MakeMoveVec(Some(type_input), _) => Ok(LinkageCommandFacts::MakeMoveVec {
            type_defining_ids: resolve_type_input(package_store, type_input)?,
        }),
        Command::MakeMoveVec(None, _)
        | Command::TransferObjects(_, _)
        | Command::SplitCoins(_, _)
        | Command::MergeCoins(_, _) => Ok(LinkageCommandFacts::Noop),
    }
}

fn resolve_type_input(
    package_store: &BackingPackageMetadataStore<'_>,
    type_input: &TypeInput,
) -> SuiResult<Vec<ObjectID>> {
    match type_input {
        TypeInput::Bool
        | TypeInput::U8
        | TypeInput::U16
        | TypeInput::U32
        | TypeInput::U64
        | TypeInput::U128
        | TypeInput::U256
        | TypeInput::Address
        | TypeInput::Signer => Ok(vec![]),
        TypeInput::Vector(element_type) => resolve_type_input(package_store, element_type),
        TypeInput::Struct(struct_input) => {
            let package_id: ObjectID = struct_input.address.into();
            let package = required_package(package_store, &package_id)?;
            let defining_id = package
                .type_origins()
                .get(&(struct_input.module.clone(), struct_input.name.clone()))
                .copied()
                .ok_or_else(|| {
                    SuiError::from(format!(
                        "Could not resolve defining ID for {}::{}::{}",
                        package_id, struct_input.module, struct_input.name
                    ))
                })?;
            let mut defining_ids = vec![defining_id];
            for type_param in &struct_input.type_params {
                defining_ids.extend(resolve_type_input(package_store, type_param)?);
            }
            Ok(defining_ids)
        }
    }
}

fn publication_linkage(
    package_store: &BackingPackageMetadataStore<'_>,
    dependencies: &[ObjectID],
) -> SuiResult<LinkageFacts> {
    let mut linkage = BTreeMap::new();
    for dependency in dependencies {
        let package = required_package(package_store, dependency)?;
        let original_id = package.original_id();
        if let Some(previous) = linkage.insert(original_id, package.id())
            && previous != package.id()
        {
            return Err(SuiError::from(format!(
                "Conflicting dependency versions for package {original_id}: {previous} and {}",
                package.id()
            )));
        }
    }
    Ok(linkage)
}

fn current_package_module_inits(package: &BackingPackageMetadata) -> SuiResult<ModuleInitFacts> {
    Ok(package
        .modules()?
        .iter()
        .map(|(module_name, module)| (module_name.clone(), module_has_init(module)))
        .collect())
}

fn module_has_init(module: &move_binary_format::CompiledModule) -> bool {
    module.function_defs().iter().any(|function_definition| {
        let handle = module.function_handle_at(function_definition.function);
        module.identifier_at(handle.name) == INIT_FN_NAME
    })
}

fn required_package(
    package_store: &BackingPackageMetadataStore<'_>,
    package_id: &ObjectID,
) -> SuiResult<std::rc::Rc<BackingPackageMetadata>> {
    package_store
        .get_package(package_id)?
        .ok_or_else(|| SuiError::from(format!("Package {package_id} not found")))
}

fn missing_function(module: &str, function: &str) -> SuiError {
    SuiError::from(format!(
        "Could not resolve function '{function}' in module '{module}'"
    ))
}
