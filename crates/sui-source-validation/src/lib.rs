// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::{AggregateError, Error};
use futures::future;
use move_binary_format::CompiledModule;
use move_compiler::compiled_unit::NamedCompiledModule;
use move_core_types::account_address::AccountAddress;
use move_package::compilation::compiled_package::CompiledPackage as MoveCompiledPackage;
use move_symbol_pool::Symbol;
use std::collections::HashMap;
use sui_move_build::CompiledPackage;
use sui_sdk::apis::ReadApi;
use sui_sdk::error::Error as SdkError;
use sui_sdk::rpc_types::{SuiObjectDataOptions, SuiRawData, SuiRawMovePackage};
use sui_types::base_types::ObjectID;
use toolchain::units_for_toolchain;

pub mod error;
mod toolchain;

#[cfg(test)]
mod tests;

/// How to handle package source during bytecode verification.
#[derive(PartialEq, Eq)]
pub enum SourceMode {
    /// Don't verify source.
    Skip,

    /// Verify source at the address specified in its manifest.
    Verify,

    /// Verify source at an overridden address (only works if the package is not published)
    VerifyAt(AccountAddress),
}

pub struct BytecodeSourceVerifier<'a> {
    rpc_client: &'a ReadApi,
}

/// Map package addresses and module names to package names and bytecode.
type LocalModules = HashMap<(AccountAddress, Symbol), (Symbol, CompiledModule)>;
/// Map package addresses and modules names to bytecode (package names are gone in the on-chain
/// representation).
type OnChainModules = HashMap<(AccountAddress, Symbol), CompiledModule>;

impl<'a> BytecodeSourceVerifier<'a> {
    pub fn new(rpc_client: &'a ReadApi) -> Self {
        BytecodeSourceVerifier { rpc_client }
    }

    /// Helper wrapper to verify that all local Move package dependencies' and root bytecode matches
    /// the bytecode at the address specified on the Sui network we are publishing to.
    pub async fn verify_package_root_and_deps(
        &self,
        compiled_package: &CompiledPackage,
        root_on_chain_address: AccountAddress,
    ) -> Result<(), AggregateError> {
        self.verify_package(
            compiled_package,
            /* verify_deps */ true,
            SourceMode::VerifyAt(root_on_chain_address),
        )
        .await
    }

    /// Helper wrapper to verify that all local Move package root bytecode matches
    /// the bytecode at the address specified on the Sui network we are publishing to.
    pub async fn verify_package_root(
        &self,
        compiled_package: &CompiledPackage,
        root_on_chain_address: AccountAddress,
    ) -> Result<(), AggregateError> {
        self.verify_package(
            compiled_package,
            /* verify_deps */ false,
            SourceMode::VerifyAt(root_on_chain_address),
        )
        .await
    }

    /// Helper wrapper to verify that all local Move package dependencies' matches
    /// the bytecode at the address specified on the Sui network we are publishing to.
    pub async fn verify_package_deps(
        &self,
        compiled_package: &CompiledPackage,
    ) -> Result<(), AggregateError> {
        self.verify_package(
            compiled_package,
            /* verify_deps */ true,
            SourceMode::Skip,
        )
        .await
    }

    /// Verify that all local Move package dependencies' and/or root bytecode matches the bytecode
    /// at the address specified on the Sui network we are publishing to.  If `verify_deps` is true,
    /// the dependencies are verified.  If `root_on_chain_address` is specified, the root is
    /// verified against a package at `root_on_chain_address`.
    pub async fn verify_package(
        &self,
        compiled_package: &CompiledPackage,
        verify_deps: bool,
        source_mode: SourceMode,
    ) -> Result<(), AggregateError> {
        let mut on_chain_pkgs = vec![];
        match &source_mode {
            SourceMode::Skip => (),
            // On-chain address for matching root package cannot be zero
            SourceMode::VerifyAt(AccountAddress::ZERO) => {
                return Err(Error::ZeroOnChainAddresSpecifiedFailure.into())
            }
            SourceMode::VerifyAt(root_address) => on_chain_pkgs.push(*root_address),
            SourceMode::Verify => {
                on_chain_pkgs.extend(compiled_package.published_at.as_ref().map(|id| **id))
            }
        };

        if verify_deps {
            on_chain_pkgs.extend(
                compiled_package
                    .dependency_ids
                    .published
                    .values()
                    .map(|id| **id),
            );
        }

        let local_modules = local_modules(&compiled_package.package, verify_deps, source_mode)?;
        let mut on_chain_modules = self.on_chain_modules(on_chain_pkgs.into_iter()).await?;

        let mut errors = Vec::new();
        for ((address, module), (package, local_module)) in local_modules {
            let Some(on_chain_module) = on_chain_modules.remove(&(address, module)) else {
                errors.push(Error::OnChainDependencyNotFound { package, module });
                continue;
            };

            // compare local bytecode to on-chain bytecode to ensure integrity of our
            // dependencies
            if local_module != on_chain_module {
                errors.push(Error::ModuleBytecodeMismatch {
                    address,
                    package,
                    module,
                });
            }
        }

        if let Some(((address, module), _)) = on_chain_modules.into_iter().next() {
            errors.push(Error::LocalDependencyNotFound { address, module });
        }

        if !errors.is_empty() {
            return Err(AggregateError(errors));
        }

        Ok(())
    }

    async fn pkg_for_address(&self, addr: AccountAddress) -> Result<SuiRawMovePackage, Error> {
        // Move packages are specified with an AccountAddress, but are
        // fetched from a sui network via sui_getObject, which takes an object ID
        let obj_id = ObjectID::from(addr);

        // fetch the Sui object at the address specified for the package in the local resolution table
        // if future packages with a large set of dependency packages prove too slow to verify,
        // batched object fetching should be added to the ReadApi & used here
        let obj_read = self
            .rpc_client
            .get_object_with_options(obj_id, SuiObjectDataOptions::new().with_bcs())
            .await
            .map_err(Error::DependencyObjectReadFailure)?;

        let obj = obj_read
            .into_object()
            .map_err(Error::SuiObjectRefFailure)?
            .bcs
            .ok_or_else(|| {
                Error::DependencyObjectReadFailure(SdkError::DataError(
                    "Bcs field is not found".to_string(),
                ))
            })?;

        match obj {
            SuiRawData::Package(pkg) => Ok(pkg),
            SuiRawData::MoveObject(move_obj) => {
                Err(Error::ObjectFoundWhenPackageExpected(obj_id, move_obj))
            }
        }
    }

    async fn on_chain_modules(
        &self,
        addresses: impl Iterator<Item = AccountAddress> + Clone,
    ) -> Result<OnChainModules, AggregateError> {
        let resp = future::join_all(addresses.clone().map(|addr| self.pkg_for_address(addr))).await;
        let mut map = OnChainModules::new();
        let mut err = vec![];

        for (storage_id, pkg) in addresses.zip(resp) {
            let SuiRawMovePackage { module_map, .. } = pkg?;
            for (name, bytes) in module_map {
                let Ok(module) = CompiledModule::deserialize_with_defaults(&bytes) else {
                    err.push(Error::OnChainDependencyDeserializationError {
                        address: storage_id,
                        module: name.into(),
                    });
                    continue;
                };

                let runtime_id = *module.self_id().address();
                map.insert((runtime_id, Symbol::from(name)), module);
            }
        }

        if !err.is_empty() {
            return Err(AggregateError(err));
        }

        Ok(map)
    }
}

fn substitute_root_address(
    named_module: &NamedCompiledModule,
    root: AccountAddress,
) -> Result<CompiledModule, Error> {
    let mut module = named_module.module.clone();
    let address_idx = module.self_handle().address;

    let Some(addr) = module.address_identifiers.get_mut(address_idx.0 as usize) else {
        return Err(Error::InvalidModuleFailure {
            name: named_module.name.to_string(),
            message: "Self address field missing".into(),
        });
    };

    if *addr != AccountAddress::ZERO {
        return Err(Error::InvalidModuleFailure {
            name: named_module.name.to_string(),
            message: "Self address already populated".to_string(),
        });
    }

    *addr = root;
    Ok(module)
}

fn local_modules(
    compiled_package: &MoveCompiledPackage,
    include_deps: bool,
    source_mode: SourceMode,
) -> Result<LocalModules, Error> {
    let mut map = LocalModules::new();

    if include_deps {
        // Compile dependencies with prior compilers if needed.
        let deps_compiled_units = units_for_toolchain(&compiled_package.deps_compiled_units)
            .map_err(|e| Error::CannotCheckLocalModules {
                package: compiled_package.compiled_package_info.package_name,
                message: e.to_string(),
            })?;

        for (package, local_unit) in deps_compiled_units {
            let m = &local_unit.unit;
            let module = m.name;
            let address = m.address.into_inner();
            if address == AccountAddress::ZERO {
                continue;
            }

            map.insert((address, module), (package, m.module.clone()));
        }
    }

    let root_package = compiled_package.compiled_package_info.package_name;
    match source_mode {
        SourceMode::Skip => { /* nop */ }

        // Include the root compiled units, at their current addresses.
        SourceMode::Verify => {
            // Compile root modules with prior compiler if needed.
            let root_compiled_units = {
                let root_compiled_units = compiled_package
                    .root_compiled_units
                    .iter()
                    .map(|u| ("root".into(), u.clone()))
                    .collect::<Vec<_>>();

                units_for_toolchain(&root_compiled_units).map_err(|e| {
                    Error::CannotCheckLocalModules {
                        package: compiled_package.compiled_package_info.package_name,
                        message: e.to_string(),
                    }
                })?
            };

            for (_, local_unit) in root_compiled_units {
                let m = &local_unit.unit;

                let module = m.name;
                let address = m.address.into_inner();
                if address == AccountAddress::ZERO {
                    return Err(Error::InvalidModuleFailure {
                        name: module.to_string(),
                        message: "Can't verify unpublished source".to_string(),
                    });
                }

                map.insert((address, module), (root_package, m.module.clone()));
            }
        }

        // Include the root compiled units, and any unpublished dependencies with their
        // addresses substituted
        SourceMode::VerifyAt(root_address) => {
            // Compile root modules with prior compiler if needed.
            let root_compiled_units = {
                let root_compiled_units = compiled_package
                    .root_compiled_units
                    .iter()
                    .map(|u| ("root".into(), u.clone()))
                    .collect::<Vec<_>>();

                units_for_toolchain(&root_compiled_units).map_err(|e| {
                    Error::CannotCheckLocalModules {
                        package: compiled_package.compiled_package_info.package_name,
                        message: e.to_string(),
                    }
                })?
            };

            for (_, local_unit) in root_compiled_units {
                let m = &local_unit.unit;

                let module = m.name;
                map.insert(
                    (root_address, module),
                    (root_package, substitute_root_address(m, root_address)?),
                );
            }

            for (package, local_unit) in &compiled_package.deps_compiled_units {
                let m = &local_unit.unit;
                let module = m.name;
                let address = m.address.into_inner();
                if address != AccountAddress::ZERO {
                    continue;
                }

                map.insert(
                    (root_address, module),
                    (*package, substitute_root_address(m, root_address)?),
                );
            }
        }
    }

    Ok(map)
}
