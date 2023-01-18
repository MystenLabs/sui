// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::access::ModuleAccess;
use std::{collections::HashMap, fmt::Debug};
use thiserror::Error;

use move_compiler::compiled_unit::{CompiledUnitEnum, NamedCompiledModule};
use move_core_types::account_address::AccountAddress;
use move_package::compilation::compiled_package::CompiledPackage;
use move_symbol_pool::Symbol;
use sui_sdk::apis::ReadApi;
use sui_sdk::error::Error;

use sui_sdk::rpc_types::{SuiRawData, SuiRawMoveObject, SuiRawMovePackage};
use sui_types::{base_types::ObjectID, error::SuiError};

#[cfg(test)]
mod tests;

#[derive(Debug, Error)]
pub enum SourceVerificationError {
    #[error("Could not read a dependency's on-chain object: {0:?}")]
    DependencyObjectReadFailure(Error),

    #[error("Dependency object does not exist or was deleted: {0:?}")]
    SuiObjectRefFailure(SuiError),

    #[error("Dependency ID contains a Sui object, not a Move package: {0}")]
    ObjectFoundWhenPackageExpected(ObjectID, SuiRawMoveObject),

    #[error("On-chain version of dependency {package}::{module} was not found.")]
    OnChainDependencyNotFound { package: Symbol, module: Symbol },

    #[error("Local version of dependency {package}::{module} was not found.")]
    LocalDependencyNotFound { package: Symbol, module: String },

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

/// Map the package's direct dependencies (keyed by their address and package name) to their module
/// bytecode (mapping from module name to byte array).
type ModuleBytesMap = HashMap<(AccountAddress, Symbol), HashMap<Symbol, Vec<u8>>>;

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
    ) -> Result<(), SourceVerificationError> {
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
    ) -> Result<(), SourceVerificationError> {
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
    ) -> Result<(), SourceVerificationError> {
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
    ) -> Result<(), SourceVerificationError> {
        // On-chain address for matching root package cannot be zero
        if let SourceMode::VerifyAt(root_address) = &source_mode {
            if *root_address == AccountAddress::ZERO {
                return Err(SourceVerificationError::ZeroOnChainAddresSpecifiedFailure);
            }
        }

        let compiled_dep_map = get_module_bytes_map(compiled_package, verify_deps, source_mode)?;

        for ((address, package), local_modules) in compiled_dep_map {
            // if `root_on_chain_address` is None, then Zero address is the package we're checking
            // dependencies for, it does not need to (and cannot) be verified.
            if address == AccountAddress::ZERO {
                continue;
            }

            // fetch the Sui object at the address specified for the package in the local resolution
            // table
            let SuiRawMovePackage {
                module_map: mut on_chain_modules,
                ..
            } = self.pkg_for_address(&address).await?;

            for (module, local_bytes) in local_modules {
                let Some(on_chain_bytes) = on_chain_modules.remove(module.as_ref()) else {
                    return Err(SourceVerificationError::OnChainDependencyNotFound {
                        package, module,
                    })
                };

                // compare local bytecode to on-chain bytecode to ensure integrity of our
                // dependencies
                if local_bytes != on_chain_bytes {
                    return Err(SourceVerificationError::ModuleBytecodeMismatch {
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

            if let Some((module, _)) = on_chain_modules.into_iter().next() {
                return Err(SourceVerificationError::LocalDependencyNotFound { package, module });
            }
        }

        Ok(())
    }

    async fn pkg_for_address(
        &self,
        addr: &AccountAddress,
    ) -> Result<SuiRawMovePackage, SourceVerificationError> {
        // Move packages are specified with an AccountAddress, but are
        // fetched from a sui network via sui_getObject, which takes an object ID
        let obj_id = ObjectID::from(*addr);

        // fetch the Sui object at the address specified for the package in the local resolution table
        // if future packages with a large set of dependency packages prove too slow to verify,
        // batched object fetching should be added to the ReadApi & used here
        let obj_read = self
            .rpc_client
            .get_object(obj_id)
            .await
            .map_err(SourceVerificationError::DependencyObjectReadFailure)?;

        let obj = obj_read
            .object()
            .map_err(SourceVerificationError::SuiObjectRefFailure)?;

        match obj.data.clone() {
            SuiRawData::Package(pkg) => Ok(pkg),
            SuiRawData::MoveObject(move_obj) => Err(
                SourceVerificationError::ObjectFoundWhenPackageExpected(obj_id, move_obj),
            ),
        }
    }
}

fn get_module_bytes_map(
    compiled_package: &CompiledPackage,
    include_deps: bool,
    source_mode: SourceMode,
) -> Result<ModuleBytesMap, SourceVerificationError> {
    let mut map: ModuleBytesMap = HashMap::new();

    #[allow(clippy::type_complexity)]
    fn make_map_entry(
        package: &Symbol,
        named_compiled_module: &NamedCompiledModule,
        subst_addr: Option<AccountAddress>,
    ) -> Result<((AccountAddress, Symbol), (Symbol, Vec<u8>)), SourceVerificationError> {
        let module = named_compiled_module.name;
        let address = subst_addr.unwrap_or_else(|| named_compiled_module.address.into_inner());
        let mut bytes = vec![];

        // Replace the zero address entries in the module if needed
        if let Some(new_address) = subst_addr {
            let mut compiled_module = named_compiled_module.module.clone();
            let self_handle = compiled_module.self_handle().clone();
            let self_address_idx = self_handle.address;

            let addrs = &mut compiled_module.address_identifiers;
            let Some(address_mut) = addrs.get_mut(self_address_idx.0 as usize) else {
                let name = compiled_module.identifier_at(self_handle.name);
                return Err(SourceVerificationError::InvalidModuleFailure {
                    name: name.to_string(),
                    message: "Self address field missing".to_string()
                });
            };

            if *address_mut != AccountAddress::ZERO {
                let name = compiled_module.identifier_at(self_handle.name);
                return Err(SourceVerificationError::InvalidModuleFailure {
                    name: name.to_string(),
                    message: "Self address already populated".to_string(),
                });
            };

            *address_mut = new_address;

            // TODO: in the future, this probably needs to use `serialize_for_version`.
            compiled_module.serialize(&mut bytes).unwrap();
        } else {
            // TODO: in the future, this probably needs to use `serialize_for_version`.
            named_compiled_module.module.serialize(&mut bytes).unwrap();
        }

        Ok(((address, *package), (module, bytes)))
    }

    if include_deps {
        for (package, local_unit) in &compiled_package.deps_compiled_units {
            let CompiledUnitEnum::Module(m) = &local_unit.unit else {
                continue;
            };

            let (k, v) = make_map_entry(package, m, None)?;

            map.entry(k).or_default().insert(v.0, v.1);
        }
    }

    if source_mode == SourceMode::Skip {
        return Ok(map);
    }

    let subst_addr = if let SourceMode::VerifyAt(root_address) = source_mode {
        Some(root_address)
    } else {
        None
    };

    let root_package = compiled_package.compiled_package_info.package_name;
    for local_unit in &compiled_package.root_compiled_units {
        let CompiledUnitEnum::Module(m) = &local_unit.unit else {
            continue;
        };

        let (package, (module, bytes)) = make_map_entry(&root_package, m, subst_addr)?;
        map.entry(package).or_default().insert(module, bytes);
    }

    Ok(map)
}
