// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::{AggregateError, Error};
use futures::future;
use move_binary_format::CompiledModule;
use move_compiler::compiled_unit::NamedCompiledModule;
use move_core_types::account_address::AccountAddress;
use move_symbol_pool::Symbol;
use std::collections::{HashMap, HashSet};
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

/// Details of what to verify
pub enum ValidationMode {
    /// Validate only the dependencies
    Deps,

    /// Validate the root package, and its linkage.
    Root {
        /// Additionally validate the dependencies, and make sure the runtime and storage IDs in
        /// dependency source code matches the root package's on-chain linkage table.
        deps: bool,

        /// Look for the root package on-chain at the specified address, rather than the address in
        /// its manifest.
        at: Option<AccountAddress>,
    },
}

pub struct BytecodeSourceVerifier<'a> {
    rpc_client: &'a ReadApi,
}

/// Map package addresses and module names to package names and bytecode.
type LocalModules = HashMap<(AccountAddress, Symbol), (Symbol, CompiledModule)>;

#[derive(Default)]
struct OnChainRepresentation {
    /// Storage IDs from the root package's on-chain linkage table. This will only be present if
    /// root package verification was requested, in which case the keys from this mapping must
    /// match the source package's dependencies.
    on_chain_dependencies: Option<HashSet<AccountAddress>>,

    /// Map package addresses and module names to bytecode (package names are gone in the on-chain
    /// representation).
    modules: HashMap<(AccountAddress, Symbol), CompiledModule>,
}

impl ValidationMode {
    /// Only verify that source dependencies match their on-chain versions.
    pub fn deps() -> Self {
        Self::Deps
    }

    /// Only verify that the root package matches its on-chain version (requires that the root
    /// package is published with its address available in the manifest).
    pub fn root() -> Self {
        Self::Root {
            deps: false,
            at: None,
        }
    }

    /// Only verify that the root package matches its on-chain version, but override the location
    /// to look for the root package to `address`.
    pub fn root_at(address: AccountAddress) -> Self {
        Self::Root {
            deps: false,
            at: Some(address),
        }
    }

    /// Verify both the root package and its dependencies (requires that the root package is
    /// published with its address available in the manifest).
    pub fn root_and_deps() -> Self {
        Self::Root {
            deps: true,
            at: None,
        }
    }

    /// Verify both the root package and its dependencies, but override the location to look for
    /// the root package to `address`.
    pub fn root_and_deps_at(address: AccountAddress) -> Self {
        Self::Root {
            deps: true,
            at: Some(address),
        }
    }

    /// Should we verify dependencies?
    fn verify_deps(&self) -> bool {
        matches!(self, Self::Deps | Self::Root { deps: true, .. })
    }

    /// If the root package needs to be verified, what address should it be fetched from?
    fn root_address(&self, package: &CompiledPackage) -> Result<Option<AccountAddress>, Error> {
        match self {
            Self::Root { at: Some(addr), .. } => Ok(Some(*addr)),
            Self::Root { at: None, .. } => Ok(Some(*package.published_at.clone()?)),
            Self::Deps => Ok(None),
        }
    }

    /// All the on-chain addresses that we need to fetch to build on-chain addresses.
    fn on_chain_addresses(&self, package: &CompiledPackage) -> Result<Vec<AccountAddress>, Error> {
        let mut addrs = vec![];

        if let Some(addr) = self.root_address(package)? {
            addrs.push(addr);
        }

        if self.verify_deps() {
            addrs.extend(dependency_addresses(package));
        }

        Ok(addrs)
    }

    /// On-chain representation of the package and dependencies compiled to `package`, including
    /// linkage information.
    async fn on_chain(
        &self,
        package: &CompiledPackage,
        verifier: &BytecodeSourceVerifier<'_>,
    ) -> Result<OnChainRepresentation, AggregateError> {
        let mut on_chain = OnChainRepresentation::default();
        let mut errs: Vec<Error> = vec![];

        let root = self.root_address(package)?;
        let addrs = self.on_chain_addresses(package)?;

        let resps =
            future::join_all(addrs.iter().copied().map(|a| verifier.pkg_for_address(a))).await;

        for (storage_id, pkg) in addrs.into_iter().zip(resps) {
            let SuiRawMovePackage {
                module_map,
                linkage_table,
                ..
            } = pkg?;

            let mut modules = module_map
                .into_iter()
                .map(|(name, bytes)| {
                    let Ok(module) = CompiledModule::deserialize_with_defaults(&bytes) else {
                        return Err(Error::OnChainDependencyDeserializationError {
                            address: storage_id,
                            module: name.into(),
                        });
                    };

                    Ok::<_, Error>((Symbol::from(name), module))
                })
                .peekable();

            let runtime_id = match modules.peek() {
                Some(Ok((_, module))) => *module.self_id().address(),

                Some(Err(_)) => {
                    // SAFETY: The error type does not implement `Clone` so we need to take the
                    // error by value. We do that by calling `next` to take the value we just
                    // peeked, which we know is an error type.
                    errs.push(modules.next().unwrap().unwrap_err());
                    continue;
                }

                None => {
                    errs.push(Error::EmptyOnChainPackage(storage_id));
                    continue;
                }
            };

            for module in modules {
                match module {
                    Ok((name, module)) => {
                        on_chain.modules.insert((runtime_id, name), module);
                    }

                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                }
            }

            if root.is_some_and(|r| r == storage_id) {
                on_chain.on_chain_dependencies = Some(HashSet::from_iter(
                    linkage_table.into_values().map(|info| *info.upgraded_id),
                ));
            }
        }

        Ok(on_chain)
    }

    /// Local representation of the modules in `package`. If the validation mode requires verifying
    /// dependencies, then the dependencies' modules are also included in the output.
    ///
    /// For the purposes of this function, a module is considered a dependency if it is from a
    /// different source package, and that source package has already been published. Conversely, a
    /// module that is from a different source package, but that has not been published is
    /// considered part of the root package.
    ///
    /// If the validation mode requires verifying the root package at a specific address, then the
    /// modules from the root package will be expected at address `0x0` and this address will be
    /// substituted with the specified address.
    fn local(&self, package: &CompiledPackage) -> Result<LocalModules, Error> {
        let sui_package = package;
        let package = &package.package;
        let root_package = package.compiled_package_info.package_name;
        let mut map = LocalModules::new();

        if self.verify_deps() {
            let deps_compiled_units =
                units_for_toolchain(&package.deps_compiled_units).map_err(|e| {
                    Error::CannotCheckLocalModules {
                        package: package.compiled_package_info.package_name,
                        message: e.to_string(),
                    }
                })?;

            for (package, local_unit) in deps_compiled_units {
                let m = &local_unit.unit;
                let module = m.name;
                let address = m.address.into_inner();

                // Skip modules with on 0x0 because they are treated as part of the root package,
                // even if they are a source dependency.
                if address == AccountAddress::ZERO {
                    continue;
                }

                map.insert((address, module), (package, m.module.clone()));
            }

            // Include bytecode dependencies.
            for (package, module) in sui_package.bytecode_deps.iter() {
                let address = *module.address();
                if address == AccountAddress::ZERO {
                    continue;
                }

                map.insert(
                    (address, Symbol::from(module.name().as_str())),
                    (*package, module.clone()),
                );
            }
        }

        let Self::Root { at, .. } = self else {
            return Ok(map);
        };

        // Potentially rebuild according to the toolchain that the package was originally built
        // with.
        let root_compiled_units = units_for_toolchain(
            &package
                .root_compiled_units
                .iter()
                .map(|u| ("root".into(), u.clone()))
                .collect(),
        )
        .map_err(|e| Error::CannotCheckLocalModules {
            package: package.compiled_package_info.package_name,
            message: e.to_string(),
        })?;

        // Add the root modules, potentially remapping 0x0 if we have been supplied an address to
        // substitute with.
        for (_, local_unit) in root_compiled_units {
            let m = &local_unit.unit;
            let module = m.name;
            let address = m.address.into_inner();

            let (address, compiled_module) = if let Some(root_address) = at {
                (*root_address, substitute_root_address(m, *root_address)?)
            } else if address == AccountAddress::ZERO {
                return Err(Error::InvalidModuleFailure {
                    name: module.to_string(),
                    message: "Can't verify unpublished source".to_string(),
                });
            } else {
                (address, m.module.clone())
            };

            map.insert((address, module), (root_package, compiled_module));
        }

        // If we have a root address to substitute, we need to find unpublished dependencies that
        // would have gone into the root package as well.
        if let Some(root_address) = at {
            for (package, local_unit) in &package.deps_compiled_units {
                let m = &local_unit.unit;
                let module = m.name;
                let address = m.address.into_inner();

                if address != AccountAddress::ZERO {
                    continue;
                }

                map.insert(
                    (*root_address, module),
                    (*package, substitute_root_address(m, *root_address)?),
                );
            }
        }

        Ok(map)
    }
}

impl<'a> BytecodeSourceVerifier<'a> {
    pub fn new(rpc_client: &'a ReadApi) -> Self {
        BytecodeSourceVerifier { rpc_client }
    }

    /// Verify that the `compiled_package` matches its on-chain representation.
    ///
    /// See [`ValidationMode`] for more details on what is verified.
    pub async fn verify(
        &self,
        package: &CompiledPackage,
        mode: ValidationMode,
    ) -> Result<(), AggregateError> {
        if matches!(
            mode,
            ValidationMode::Root {
                at: Some(AccountAddress::ZERO),
                ..
            }
        ) {
            return Err(Error::ZeroOnChainAddresSpecifiedFailure.into());
        }

        let local = mode.local(package)?;
        let mut chain = mode.on_chain(package, self).await?;
        let mut errs = vec![];

        // Check that the transitive dependencies listed on chain match the dependencies listed in
        // source code. Ignore 0x0 becaus this signifies an unpublished dependency.
        if let Some(on_chain_deps) = &mut chain.on_chain_dependencies {
            for dependency_id in dependency_addresses(package) {
                if dependency_id != AccountAddress::ZERO && !on_chain_deps.remove(&dependency_id) {
                    errs.push(Error::MissingDependencyInLinkageTable(dependency_id));
                }
            }
        }

        for on_chain_dep_id in chain.on_chain_dependencies.take().into_iter().flatten() {
            errs.push(Error::MissingDependencyInSourcePackage(on_chain_dep_id));
        }

        // Check that the contents of bytecode matches between modules.
        for ((address, module), (package, local_module)) in local {
            let Some(on_chain_module) = chain.modules.remove(&(address, module)) else {
                errs.push(Error::OnChainDependencyNotFound { package, module });
                continue;
            };

            if local_module != on_chain_module {
                errs.push(Error::ModuleBytecodeMismatch {
                    address,
                    package,
                    module,
                })
            }
        }

        for (address, module) in chain.modules.into_keys() {
            errs.push(Error::LocalDependencyNotFound { address, module });
        }

        if !errs.is_empty() {
            return Err(AggregateError(errs));
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

/// The on-chain addresses for a source package's dependencies
fn dependency_addresses(package: &CompiledPackage) -> impl Iterator<Item = AccountAddress> + '_ {
    package.dependency_ids.published.values().map(|id| **id)
}
