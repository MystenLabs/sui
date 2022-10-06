// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, HashMap},
    fmt::{Debug, Display},
    path::Path,
    str::FromStr,
};

use move_core_types::account_address::AccountAddress;
use move_package::{compilation::compiled_package::CompiledPackage, BuildConfig};
use move_symbol_pool::Symbol;

use sui_sdk::{
    rpc_types::{SuiRawData, SuiRawMoveObject, SuiRawMovePackage},
    ReadApi,
};
use sui_types::{
    base_types::{ObjectID, ObjectIDParseError},
    error::SuiError,
};

#[derive(Clone, Debug)]
pub struct DependencyVerificationResult {
    pub verified_dependencies: HashMap<AccountAddress, Dependency>,
}

#[derive(Debug)]
pub enum DependencyVerificationError {
    /// Could not resolve Sui addresses for package dependencies
    ResolutionGraphNotResolved(anyhow::Error),
    /// Could not convert a dependencies' resolved Sui address to a Sui object ID
    ObjectIdFromAddressFailure(ObjectIDParseError),
    /// Could not read a dependencies' on-chain object
    DependencyObjectReadFailure(anyhow::Error),
    /// Dependency object does not exist or was deleted
    SuiObjectRefFailure(SuiError),
    /// Dependency address contains a Sui object, not a Move package
    ObjectFoundWhenPackageExpected(ObjectID, SuiRawMoveObject),
    /// A local dependency was not found
    ///
    /// params:  package, module
    LocalDependencyNotFound(Symbol, Option<Symbol>),
    /// Local dependencies have a different number of modules than on-chain
    ///
    /// params:  expected count, on-chain count
    ModuleCountMismatch(usize, usize),
    /// A local dependency module did not match its on-chain version
    ///
    /// params:  package, module, address, expected, found
    ModuleBytecodeMismatch(String, String, AccountAddress, Vec<u8>, Vec<u8>),
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
    pub address: AccountAddress,
    pub module_bytes: BTreeMap<String, Vec<u8>>,
}

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
        build_config: &BuildConfig,
        path: &Path,
        compiled_package: CompiledPackage,
    ) -> Result<DependencyVerificationResult, DependencyVerificationError> {
        let resolution_graph = match build_config.clone().resolution_graph_for_package(path) {
            Ok(graph) => graph,
            Err(err) => {
                return Err(DependencyVerificationError::ResolutionGraphNotResolved(err));
            }
        };

        let compiled_dep_map = Self::get_module_bytes_map(&compiled_package);

        let mut on_chain_module_count = 0usize;
        let mut verified_dependencies: HashMap<AccountAddress, Dependency> = HashMap::new();

        for (pkg_symbol, resolution_package) in resolution_graph.package_table {
            if pkg_symbol == compiled_package.compiled_package_info.package_name {
                continue;
            };

            let local_pkg_bytes = match compiled_dep_map.get(&pkg_symbol) {
                Some(module_bytes) => {
                    if self.verbose {
                        println!(
                            "\nlocal package dependency {} : {} modules",
                            pkg_symbol,
                            module_bytes.len()
                        );
                    }
                    module_bytes
                }
                None => {
                    return Err(DependencyVerificationError::LocalDependencyNotFound(
                        pkg_symbol, None,
                    ));
                }
            };

            for (symbol, addr) in resolution_package.resolution_table {
                // package addresses may show up many times, but we only need to verify them once
                // zero address is the package we're checking dependencies for
                if verified_dependencies.contains_key(&addr) || addr.eq(&AccountAddress::ZERO) {
                    continue;
                }

                // fetch the Sui object at the address specified for the package in the local resolution table
                let on_chain_package = self.pkg_for_address(&addr).await?;

                for (oc_name, on_chain_bytes) in &on_chain_package.module_map {
                    let on_chain_module_symbol = Symbol::from(oc_name.as_str());
                    let local_bytes = match local_pkg_bytes.get(&on_chain_module_symbol) {
                        Some(bytes) => bytes,
                        None => {
                            return Err(DependencyVerificationError::LocalDependencyNotFound(
                                pkg_symbol,
                                Some(on_chain_module_symbol),
                            ))
                        }
                    };

                    // compare local bytecode to on-chain bytecode to ensure integrity of our dependencies
                    if local_bytes != on_chain_bytes {
                        return Err(DependencyVerificationError::ModuleBytecodeMismatch(
                            pkg_symbol.to_string(),
                            on_chain_module_symbol.to_string(),
                            addr,
                            local_bytes.clone(),
                            on_chain_bytes.clone(),
                        ));
                    }

                    if self.verbose {
                        println!(
                            "{}::{} - {} bytes, code matches",
                            pkg_symbol,
                            oc_name,
                            on_chain_bytes.len()
                        );
                    }
                }

                on_chain_module_count += on_chain_package.module_map.len();

                verified_dependencies.insert(
                    addr,
                    Dependency {
                        symbol: symbol.to_string(),
                        address: addr,
                        module_bytes: on_chain_package.module_map.clone(),
                    },
                );
            }
        }

        // total number of modules in packages must match, in addition to each individual module matching
        if compiled_package.deps_compiled_units.len() != on_chain_module_count {
            let len = compiled_package.deps_compiled_units.len();
            return Err(DependencyVerificationError::ModuleCountMismatch(
                len,
                on_chain_module_count,
            ));
        }

        Ok(DependencyVerificationResult {
            verified_dependencies,
        })
    }

    fn get_module_bytes_map(
        compiled_package: &CompiledPackage,
    ) -> HashMap<Symbol, HashMap<Symbol, Vec<u8>>> {
        let mut map: HashMap<Symbol, HashMap<Symbol, Vec<u8>>> = HashMap::new();
        compiled_package
            .deps_compiled_units
            .iter()
            .for_each(|(symbol, unit_src)| {
                let name = unit_src.unit.name();
                // in the future, this probably needs to specify the compiler version instead of None
                let bytes = unit_src.unit.serialize(None);

                match map.get_mut(symbol) {
                    Some(existing_modules) => {
                        existing_modules.insert(name, bytes);
                    }
                    None => {
                        let mut new_map = HashMap::new();
                        new_map.insert(name, bytes);
                        map.insert(*symbol, new_map);
                    }
                }
            });

        map
    }

    async fn pkg_for_address(
        &self,
        addr: &AccountAddress,
    ) -> Result<SuiRawMovePackage, DependencyVerificationError> {
        // Move packages are specified with an AccountAddress, but are
        // fetched from a sui network via sui_getObject, which takes an object ID
        let obj_id = match ObjectID::from_str(addr.to_string().as_str()) {
            Ok(id) => id,
            Err(err) => return Err(DependencyVerificationError::ObjectIdFromAddressFailure(err)),
        };

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
