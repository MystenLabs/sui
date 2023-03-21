// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use core::fmt;
use futures::future;
use move_binary_format::access::ModuleAccess;
use move_binary_format::CompiledModule;
use std::{collections::HashMap, fmt::Debug};
use sui_types::error::UserInputError;
use thiserror::Error;

use move_compiler::compiled_unit::{CompiledUnitEnum, NamedCompiledModule};
use move_core_types::account_address::AccountAddress;
use move_package::compilation::compiled_package::CompiledPackage;
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
    SuiObjectRefFailure(UserInputError),

    #[error("Dependency ID contains a Sui object, not a Move package: {0}")]
    ObjectFoundWhenPackageExpected(ObjectID, SuiRawMoveObject),

    #[error("On-chain version of dependency {package}::{module} was not found.")]
    OnChainDependencyNotFound { package: Symbol, module: Symbol },

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
    pub verbose: bool,
    rpc_client: &'a ReadApi,
}

/// Map package addresses and module names to package names and bytecode.
type LocalBytes = HashMap<(AccountAddress, Symbol), (Symbol, Vec<u8>)>;
/// Map package addresses and modules names to bytecode (package names are gone in the on-chain
/// representation).
type OnChainBytes = HashMap<(AccountAddress, Symbol), Vec<u8>>;

impl<'a> BytecodeSourceVerifier<'a> {
    pub fn new(rpc_client: &'a ReadApi, verbose: bool) -> Self {
        BytecodeSourceVerifier {
            verbose,
            rpc_client,
        }
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
        // On-chain address for matching root package cannot be zero
        if let SourceMode::VerifyAt(root_address) = &source_mode {
            if *root_address == AccountAddress::ZERO {
                return Err(SourceVerificationError::ZeroOnChainAddresSpecifiedFailure.into());
            }
        }

        let local_modules = local_bytes(compiled_package, verify_deps, source_mode)?;
        let mut on_chain_modules = self
            .on_chain_bytes(local_modules.keys().map(|(addr, _)| *addr))
            .await?;

        let mut errors = Vec::new();
        for ((address, module), (package, local_bytes)) in local_modules {
            let Some(on_chain_bytes) = on_chain_modules.remove(&(address, module)) else {
                errors.push(SourceVerificationError::OnChainDependencyNotFound {
                    package, module,
                });
		continue;
            };

            // compare local bytecode to on-chain bytecode to ensure integrity of our
            // dependencies
            if local_bytes != on_chain_bytes {
                errors.push(SourceVerificationError::ModuleBytecodeMismatch {
                    address,
                    package,
                    module,
                });
            }

            if self.verbose {
                println!(
                    "{}::{} - {} bytes, code matches",
                    package.as_ref(),
                    module.as_ref(),
                    on_chain_bytes.len()
                );
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

    async fn on_chain_bytes(
        &self,
        addresses: impl Iterator<Item = AccountAddress> + Clone,
    ) -> Result<OnChainBytes, SourceVerificationError> {
        let resp = future::join_all(addresses.clone().map(|addr| self.pkg_for_address(addr))).await;
        let mut map = OnChainBytes::new();

        for (addr, pkg) in addresses.zip(resp) {
            let SuiRawMovePackage { module_map, .. } = pkg?;
            map.extend(
                module_map
                    .into_iter()
                    .map(move |(module, bytes)| ((addr, Symbol::from(module)), bytes)),
            )
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

fn local_bytes(
    compiled_package: &CompiledPackage,
    include_deps: bool,
    source_mode: SourceMode,
) -> Result<LocalBytes, SourceVerificationError> {
    let mut map = LocalBytes::new();

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

            let mut bytes = vec![];
            m.module.serialize(&mut bytes).unwrap();
            map.insert((address, module), (*package, bytes));
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

                let mut bytes = vec![];
                m.module.serialize(&mut bytes).unwrap();
                map.insert((address, module), (root_package, bytes));
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
                let mut bytes = vec![];
                substitute_root_address(m, root_address)?
                    .serialize(&mut bytes)
                    .unwrap();
                map.insert((root_address, module), (root_package, bytes));
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

                let mut bytes = vec![];
                substitute_root_address(m, root_address)?
                    .serialize(&mut bytes)
                    .unwrap();
                map.insert((root_address, module), (*package, bytes));
            }
        }
    }

    Ok(map)
}
