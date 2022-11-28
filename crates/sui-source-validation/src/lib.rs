// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, fmt::Debug};
use thiserror::Error;

use move_compiler::compiled_unit::CompiledUnitEnum;
use move_core_types::account_address::AccountAddress;
use move_package::compilation::compiled_package::CompiledPackage;
use move_symbol_pool::Symbol;

use sui_sdk::rpc_types::{SuiRawData, SuiRawMoveObject, SuiRawMovePackage};
use sui_types::{base_types::ObjectID, error::SuiError};

#[cfg(test)]
mod tests;

#[cfg(not(msim))]
type ReadApi = sui_sdk::ReadApi;
#[cfg(msim)]
type ReadApi = sui_sdk::embedded_gateway::ReadApi;

#[derive(Debug, Error)]
pub enum DependencyVerificationError {
    #[error("Could not read a dependency's on-chain object: {0:?}")]
    DependencyObjectReadFailure(anyhow::Error),

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

    /// Verify that all local Move package dependencies' bytecode matches
    /// the bytecode at the address specified on the Sui network we are publishing to.
    pub async fn verify_deployed_dependencies(
        &self,
        compiled_package: &CompiledPackage,
    ) -> Result<(), DependencyVerificationError> {
        let compiled_dep_map = get_module_bytes_map(compiled_package);

        for ((address, package), local_modules) in compiled_dep_map {
            // Zero address is the package we're checking dependencies for, it does not need to (and
            // cannot) be verified.
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
                    return Err(DependencyVerificationError::OnChainDependencyNotFound {
                        package, module,
                    })
                };

                // compare local bytecode to on-chain bytecode to ensure integrity of our
                // dependencies
                if local_bytes != on_chain_bytes {
                    return Err(DependencyVerificationError::ModuleBytecodeMismatch {
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
                return Err(DependencyVerificationError::LocalDependencyNotFound {
                    package,
                    module,
                });
            }
        }

        Ok(())
    }

    async fn pkg_for_address(
        &self,
        addr: &AccountAddress,
    ) -> Result<SuiRawMovePackage, DependencyVerificationError> {
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
            .map_err(DependencyVerificationError::DependencyObjectReadFailure)?;

        let obj = obj_read
            .object()
            .map_err(DependencyVerificationError::SuiObjectRefFailure)?;

        match obj.data.clone() {
            SuiRawData::Package(pkg) => Ok(pkg),
            SuiRawData::MoveObject(move_obj) => Err(
                DependencyVerificationError::ObjectFoundWhenPackageExpected(obj_id, move_obj),
            ),
        }
    }
}

fn get_module_bytes_map(compiled_package: &CompiledPackage) -> ModuleBytesMap {
    let mut map: ModuleBytesMap = HashMap::new();
    for (package, local_unit) in &compiled_package.deps_compiled_units {
        let CompiledUnitEnum::Module(m) = &local_unit.unit else {
            continue;
        };

        let module = m.name;
        let address = m.address.into_inner();

        // in the future, this probably needs to use `serialize_for_version`.
        let mut bytes = vec![];
        m.module.serialize(&mut bytes).unwrap();

        map.entry((address, *package))
            .or_default()
            .insert(module, bytes);
    }
    map
}
