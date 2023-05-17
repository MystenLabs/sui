// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use core::fmt;
use futures::future;
use move_binary_format::access::ModuleAccess;
use move_binary_format::CompiledModule;
use std::{collections::HashMap, fmt::Debug};
use sui_move_build::CompiledPackage;
use sui_types::error::SuiObjectResponseError;
use thiserror::Error;

use move_compiler::compiled_unit::{CompiledUnitEnum, NamedCompiledModule};
use move_core_types::account_address::AccountAddress;
use move_package::compilation::compiled_package::CompiledPackage as MoveCompiledPackage;
use move_symbol_pool::Symbol;
use sui_sdk::apis::ReadApi;
use sui_sdk::error::Error;

use sui_sdk::rpc_types::{SuiObjectDataOptions, SuiRawData, SuiRawMoveObject, SuiRawMovePackage};
use sui_types::base_types::ObjectID;

#[cfg(test)]
mod tests;

#[derive(Debug, Error)]
pub enum SourceVerificationError {
    #[error("Could not read a dependency's on-chain object: {0:?}")]
    DependencyObjectReadFailure(Error),

    #[error("Dependency object does not exist or was deleted: {0:?}")]
    SuiObjectRefFailure(SuiObjectResponseError),

    #[error("Dependency ID contains a Sui object, not a Move package: {0}")]
    ObjectFoundWhenPackageExpected(ObjectID, SuiRawMoveObject),

    #[error("On-chain version of dependency {package}::{module} was not found.")]
    OnChainDependencyNotFound { package: Symbol, module: Symbol },

    #[error("Could not deserialize on-chain dependency {address}::{module}.")]
    OnChainDependencyDeserializationError {
        address: AccountAddress,
        module: Symbol,
    },

    #[error("Local version of dependency {address}::{module} was not found.")]
    LocalDependencyNotFound {
        address: AccountAddress,
        module: Symbol,
    },

    #[error(
        "Local dependency did not match its on-chain version at {address}::{package}::{module}"
    )]
    ModuleBytecodeMismatch {
        address: AccountAddress,
        package: Symbol,
        module: Symbol,
    },

    #[error("On-chain address cannot be zero")]
    ZeroOnChainAddresSpecifiedFailure,

    #[error("Invalid module {name} with error: {message}")]
    InvalidModuleFailure { name: String, message: String },
}

#[derive(Debug, Error)]
pub struct AggregateSourceVerificationError(Vec<SourceVerificationError>);

impl From<SourceVerificationError> for AggregateSourceVerificationError {
    fn from(error: SourceVerificationError) -> Self {
        AggregateSourceVerificationError(vec![error])
    }
}

impl fmt::Display for AggregateSourceVerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let AggregateSourceVerificationError(errors) = self;
        match &errors[..] {
            [] => unreachable!("Aggregate error with no errors"),
            [error] => write!(f, "{}", error)?,
            errors => {
                writeln!(f, "Multiple source verification errors found:")?;
                for error in errors {
                    write!(f, "\n- {}", error)?;
                }
                return Ok(());
            }
        };
        Ok(())
    }
}

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
    ) -> Result<(), AggregateSourceVerificationError> {
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
    ) -> Result<(), AggregateSourceVerificationError> {
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
    ) -> Result<(), AggregateSourceVerificationError> {
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
    ) -> Result<(), AggregateSourceVerificationError> {
        let mut on_chain_pkgs = vec![];
        match &source_mode {
            SourceMode::Skip => (),
            // On-chain address for matching root package cannot be zero
            SourceMode::VerifyAt(AccountAddress::ZERO) => {
                return Err(SourceVerificationError::ZeroOnChainAddresSpecifiedFailure.into())
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
                errors.push(SourceVerificationError::OnChainDependencyNotFound {
                    package, module,
                });
		continue;
            };

            // compare local bytecode to on-chain bytecode to ensure integrity of our
            // dependencies
            if local_module != on_chain_module {
                errors.push(SourceVerificationError::ModuleBytecodeMismatch {
                    address,
                    package,
                    module,
                });
            }
        }

        if let Some(((address, module), _)) = on_chain_modules.into_iter().next() {
            errors.push(SourceVerificationError::LocalDependencyNotFound { address, module });
        }

        if !errors.is_empty() {
            return Err(AggregateSourceVerificationError(errors));
        }

        Ok(())
    }

    async fn pkg_for_address(
        &self,
        addr: AccountAddress,
    ) -> Result<SuiRawMovePackage, SourceVerificationError> {
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
            .map_err(SourceVerificationError::DependencyObjectReadFailure)?;

        let obj = obj_read
            .into_object()
            .map_err(SourceVerificationError::SuiObjectRefFailure)?
            .bcs
            .ok_or_else(|| {
                SourceVerificationError::DependencyObjectReadFailure(Error::DataError(
                    "Bcs field is not found".to_string(),
                ))
            })?;

        match obj {
            SuiRawData::Package(pkg) => Ok(pkg),
            SuiRawData::MoveObject(move_obj) => Err(
                SourceVerificationError::ObjectFoundWhenPackageExpected(obj_id, move_obj),
            ),
        }
    }

    async fn on_chain_modules(
        &self,
        addresses: impl Iterator<Item = AccountAddress> + Clone,
    ) -> Result<OnChainModules, AggregateSourceVerificationError> {
        let resp = future::join_all(addresses.clone().map(|addr| self.pkg_for_address(addr))).await;
        let mut map = OnChainModules::new();
        let mut err = vec![];

        for (storage_id, pkg) in addresses.zip(resp) {
            let SuiRawMovePackage { module_map, .. } = pkg?;
            for (name, bytes) in module_map {
                let Ok(module) = CompiledModule::deserialize_with_defaults(&bytes) else {
                    err.push(SourceVerificationError::OnChainDependencyDeserializationError {
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
            return Err(AggregateSourceVerificationError(err));
        }

        Ok(map)
    }
}

fn substitute_root_address(
    named_module: &NamedCompiledModule,
    root: AccountAddress,
) -> Result<CompiledModule, SourceVerificationError> {
    let mut module = named_module.module.clone();
    let address_idx = module.self_handle().address;

    let Some(addr) = module.address_identifiers.get_mut(address_idx.0 as usize) else {
        return Err(SourceVerificationError::InvalidModuleFailure {
            name: named_module.name.to_string(),
            message: "Self address field missing".into(),
        });
    };

    if *addr != AccountAddress::ZERO {
        return Err(SourceVerificationError::InvalidModuleFailure {
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
) -> Result<LocalModules, SourceVerificationError> {
    let mut map = LocalModules::new();

    if include_deps {
        for (package, local_unit) in &compiled_package.deps_compiled_units {
            let CompiledUnitEnum::Module(m) = &local_unit.unit else {
                continue;
            };

            let module = m.name;
            let address = m.address.into_inner();
            if address == AccountAddress::ZERO {
                continue;
            }

            map.insert((address, module), (*package, m.module.clone()));
        }
    }

    let root_package = compiled_package.compiled_package_info.package_name;
    match source_mode {
        SourceMode::Skip => { /* nop */ }

        // Include the root compiled units, at their current addresses.
        SourceMode::Verify => {
            for local_unit in &compiled_package.root_compiled_units {
                let CompiledUnitEnum::Module(m) = &local_unit.unit else {
                    continue;
                };

                let module = m.name;
                let address = m.address.into_inner();
                if address == AccountAddress::ZERO {
                    return Err(SourceVerificationError::InvalidModuleFailure {
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
            for local_unit in &compiled_package.root_compiled_units {
                let CompiledUnitEnum::Module(m) = &local_unit.unit else {
                    continue;
                };

                let module = m.name;
                map.insert(
                    (root_address, module),
                    (root_package, substitute_root_address(m, root_address)?),
                );
            }

            for (package, local_unit) in &compiled_package.deps_compiled_units {
                let CompiledUnitEnum::Module(m) = &local_unit.unit else {
                    continue;
                };

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
