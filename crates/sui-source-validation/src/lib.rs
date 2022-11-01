// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, HashMap},
    fmt::{Debug, Display},
};

use move_compiler::compiled_unit::CompiledUnitEnum;
use move_core_types::account_address::AccountAddress;
use move_package::compilation::compiled_package::CompiledPackage;
use move_symbol_pool::Symbol;

use sui_sdk::{
    rpc_types::{SuiRawData, SuiRawMoveObject, SuiRawMovePackage},
    ReadApi,
};
use sui_types::{base_types::ObjectID, error::SuiError};

#[derive(Clone, Debug)]
pub struct DependencyVerificationResult {
    pub verified_dependencies: HashMap<AccountAddress, Dependency>,
}

#[derive(Debug)]
pub enum DependencyVerificationError {
    /// Could not read a dependencies' on-chain object
    DependencyObjectReadFailure(anyhow::Error),
    /// Dependency object does not exist or was deleted
    SuiObjectRefFailure(SuiError),
    /// Dependency address contains a Sui object, not a Move package
    ObjectFoundWhenPackageExpected(ObjectID, SuiRawMoveObject),
    /// A local dependency was not found
    LocalDependencyNotFound {
        package: Symbol,
        module: Option<Symbol>,
    },
    /// An on-chain version of a local dependency was not found
    ///
    /// params:  package, module
    OnChainDependencyNotFound(Symbol, Option<Symbol>),
    /// Local dependencies have a different number of modules than on-chain
    ///
    /// params:  expected count, on-chain count, missing
    ModuleCountMismatch(usize, usize, Vec<String>),
    /// A local dependency module did not match its on-chain version
    ///
    /// params:  package, module, address
    ModuleBytecodeMismatch(String, String, AccountAddress),
}

impl Display for DependencyVerificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self, f)
    }
}

#[derive(Debug)]
pub struct BytecodeSourceVerifier<'a> {
    pub verbose: bool,
    rpc_client: &'a ReadApi,
}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct Dependency {
    pub symbol: String,
    pub module_bytes: BTreeMap<String, Vec<u8>>,
}

/// map the package's direct dependency symbols to their addresses and nested dependencies
type ModuleBytesMap = HashMap<Symbol, (AccountAddress, HashMap<Symbol, Vec<u8>>)>;

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
        &mut self,
        compiled_package: CompiledPackage,
    ) -> Result<DependencyVerificationResult, DependencyVerificationError> {
        let compiled_dep_map = get_module_bytes_map(&compiled_package);

        let mut on_chain_module_count = 0usize;
        let mut verified_dependencies: HashMap<AccountAddress, Dependency> = HashMap::new();

        for (pkg_symbol, (addr, local_pkg_bytes)) in compiled_dep_map {
            if pkg_symbol == compiled_package.compiled_package_info.package_name {
                continue;
            };

            // fetch the Sui object at the address specified for the package in the local resolution table
            let on_chain_package = self.pkg_for_address(&addr).await?;

            for (module_symbol, local_bytes) in local_pkg_bytes {
                // package addresses may show up many times, but we only need to verify them once
                // zero address is the package we're checking dependencies for
                if verified_dependencies.contains_key(&addr) || addr == AccountAddress::ZERO {
                    continue;
                }

                let mod_str = module_symbol.to_string();
                let on_chain_bytes = match on_chain_package.module_map.get(&mod_str) {
                    Some(oc_bytes) => oc_bytes,
                    None => {
                        return Err(DependencyVerificationError::OnChainDependencyNotFound(
                            pkg_symbol,
                            Some(module_symbol),
                        ))
                    }
                };

                // compare local bytecode to on-chain bytecode to ensure integrity of our dependencies
                if &local_bytes != on_chain_bytes {
                    return Err(DependencyVerificationError::ModuleBytecodeMismatch(
                        pkg_symbol.to_string(),
                        module_symbol.to_string(),
                        addr,
                    ));
                }

                on_chain_module_count += 1;

                if self.verbose {
                    println!(
                        "{}::{} - {} bytes, code matches",
                        pkg_symbol,
                        module_symbol,
                        on_chain_bytes.len()
                    );
                }
            }

            verified_dependencies.insert(
                addr,
                Dependency {
                    symbol: pkg_symbol.to_string(),
                    module_bytes: on_chain_package.module_map.clone(),
                },
            );
        }

        // total number of modules in packages must match, in addition to each individual module matching
        let len = compiled_package.deps_compiled_units.len();
        // only need to check for less than, because if local modules are missing on-chain we've already errored out
        if len < on_chain_module_count {
            let missing_modules = get_missing_modules(&compiled_package, &verified_dependencies);
            return Err(DependencyVerificationError::ModuleCountMismatch(
                len,
                on_chain_module_count,
                missing_modules,
            ));
        }

        Ok(DependencyVerificationResult {
            verified_dependencies,
        })
    }

    async fn pkg_for_address(
        &mut self,
        addr: &AccountAddress,
    ) -> Result<SuiRawMovePackage, DependencyVerificationError> {
        // Move packages are specified with an AccountAddress, but are
        // fetched from a sui network via sui_getObject, which takes an object ID
        let obj_id = ObjectID::from(*addr);

        // fetch the Sui object at the address specified for the package in the local resolution table
        // if future packages with a large set of dependency packages prove too slow to verify,
        // batched object fetching should be added to the ReadApi & used here
        let obj_read = match self.rpc_client.get_object(obj_id).await {
            Ok(raw) => raw,
            Err(err) => {
                return Err(DependencyVerificationError::DependencyObjectReadFailure(
                    err,
                ))
            }
        };
        let obj = match obj_read.object() {
            Ok(sui_obj) => sui_obj,
            Err(err) => return Err(DependencyVerificationError::SuiObjectRefFailure(err)),
        };

        match obj.data.clone() {
            SuiRawData::Package(pkg) => Ok(pkg),
            SuiRawData::MoveObject(move_obj) => Err(
                DependencyVerificationError::ObjectFoundWhenPackageExpected(obj_id, move_obj),
            ),
        }
    }
}

fn get_missing_modules(
    package: &CompiledPackage,
    verified_dependencies: &HashMap<AccountAddress, Dependency>,
) -> Vec<String> {
    let mut missing_modules: Vec<String> = vec![];
    for (local_pkg_symbol, local_unit) in &package.deps_compiled_units {
        let local_pkg_symbol_str = local_pkg_symbol.to_string();
        let local_mod_name = local_unit.unit.name().to_string();
        let mod_str = local_mod_name.as_str();

        if !verified_dependencies.iter().any(|(_, dep)| {
            dep.symbol == local_pkg_symbol_str && dep.module_bytes.contains_key(mod_str)
        }) {
            missing_modules.push(format!("{}::{}", local_pkg_symbol_str, mod_str))
        }
    }
    missing_modules
}

fn get_module_bytes_map(compiled_package: &CompiledPackage) -> ModuleBytesMap {
    let mut map: ModuleBytesMap = HashMap::new();
    compiled_package
        .deps_compiled_units
        .iter()
        .for_each(|(symbol, unit_src)| {
            let name = unit_src.unit.name();
            // in the future, this probably needs to specify the compiler version instead of None
            let bytes = unit_src.unit.serialize(None);

            if let CompiledUnitEnum::Module(m) = unit_src.unit.clone() {
                let module_addr: AccountAddress = m.address.into_inner();

                match map.get_mut(symbol) {
                    Some((_addr, existing_modules)) => {
                        existing_modules.insert(name, bytes);
                    }
                    None => {
                        let mut new_map = HashMap::new();
                        new_map.insert(name, bytes);
                        map.insert(*symbol, (module_addr, new_map));
                    }
                }
            }
        });

    map
}
