// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeSet, HashSet},
    path::PathBuf,
};

use fastcrypto::encoding::Base64;
use move_binary_format::{
    access::ModuleAccess,
    normalized::{self, Type},
    CompiledModule,
};
use move_bytecode_utils::{layout::SerdeLayoutBuilder, module_cache::GetModule, Modules};
use move_compiler::compiled_unit::CompiledUnitEnum;
use move_core_types::{
    account_address::AccountAddress,
    language_storage::{ModuleId, StructTag, TypeTag},
};
use move_package::{
    compilation::compiled_package::CompiledPackage as MoveCompiledPackage,
    BuildConfig as MoveBuildConfig,
};
use serde_reflection::Registry;
use sui_types::{
    error::{SuiError, SuiResult},
    MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS,
};
use sui_verifier::verifier as sui_bytecode_verifier;

use crate::{MOVE_STDLIB_PACKAGE_NAME, SUI_PACKAGE_NAME};

/// Wrapper around the core Move `CompiledPackage` with some Sui-specific traits and info
pub struct CompiledPackage {
    pub package: MoveCompiledPackage,
    /// Path to the Move package (i.e., where the Move.toml file is)
    pub path: PathBuf,
}

/// Wrapper around the core Move `BuildConfig` with some Sui-specific info
pub struct BuildConfig {
    pub config: MoveBuildConfig,
    /// If true, run the Move bytecode verifier on the bytecode from a successful build
    pub run_bytecode_verifier: bool,
    /// If true, print build diagnostics to stderr--no printing if false
    pub print_diags_to_stderr: bool,
}

impl BuildConfig {
    /// Given a `path` and a `build_config`, build the package in that path, including its dependencies.
    /// If we are building the Sui framework, we skip the check that the addresses should be 0
    pub fn build(self, path: PathBuf) -> SuiResult<CompiledPackage> {
        let res = if self.print_diags_to_stderr {
            self.config
                .compile_package_no_exit(&path, &mut std::io::stderr())
        } else {
            self.config.compile_package_no_exit(&path, &mut Vec::new())
        };

        // write build failure diagnostics to stderr, convert `error` to `String` using `Debug`
        // format to include anyhow's error context chain.
        let package = match res {
            Err(error) => {
                return Err(SuiError::ModuleBuildFailure {
                    error: format!("{:?}", error),
                })
            }
            Ok(package) => package,
        };
        let compiled_modules = package.root_modules_map();
        let package_name = package.compiled_package_info.package_name.as_str();
        let is_framework =
            package_name == SUI_PACKAGE_NAME || package_name == MOVE_STDLIB_PACKAGE_NAME;
        if !is_framework {
            if let Some(m) = compiled_modules
                .iter_modules()
                .iter()
                .find(|m| m.self_id().address() != &AccountAddress::ZERO)
            {
                // TODO: this should be a generic build failure, not a Sui module publish failure
                return Err(SuiError::ModulePublishFailure {
                    error: format!(
                        "Modules must all have 0x0 as their addresses. Violated by module {:?}",
                        m.self_id()
                    ),
                });
            }
        }
        if self.run_bytecode_verifier {
            for m in compiled_modules.iter_modules() {
                move_bytecode_verifier::verify_module(m).map_err(|err| {
                    SuiError::ModuleVerificationFailure {
                        error: err.to_string(),
                    }
                })?;
                sui_bytecode_verifier::verify_module(m)?;
            }
            // TODO(https://github.com/MystenLabs/sui/issues/69): Run Move linker
        }
        Ok(CompiledPackage { package, path })
    }
}

impl CompiledPackage {
    /// Return all of the bytecode modules in this package (not including direct or transitive deps)
    /// Note: these are not topologically sorted by dependency--use `get_dependency_sorted_modules` to produce a list of modules suitable
    /// for publishing or static analysis
    pub fn get_modules(&self) -> impl Iterator<Item = &CompiledModule> {
        self.package.root_modules().map(|m| match &m.unit {
            CompiledUnitEnum::Module(m) => &m.module,
            CompiledUnitEnum::Script(_) => unimplemented!("Scripts not supported in Sui Move"),
        })
    }

    /// Return all of the bytecode modules in this package (not including direct or transitive deps)
    /// Note: these are not topologically sorted by dependency--use `get_dependency_sorted_modules` to produce a list of modules suitable
    /// for publishing or static analysis
    pub fn into_modules(self) -> Vec<CompiledModule> {
        self.package
            .root_compiled_units
            .into_iter()
            .map(|m| match m.unit {
                CompiledUnitEnum::Module(m) => m.module,
                CompiledUnitEnum::Script(_) => unimplemented!("Scripts not supported in Sui Move"),
            })
            .collect()
    }

    /// Return all of the bytecode modules that this package depends on (both directly and transitively)
    /// Note: these are not topologically sorted by dependency.
    pub fn get_dependent_modules(&self) -> impl Iterator<Item = &CompiledModule> {
        self.package
            .deps_compiled_units
            .iter()
            .map(|(_, m)| match &m.unit {
                CompiledUnitEnum::Module(m) => &m.module,
                CompiledUnitEnum::Script(_) => unimplemented!("Scripts not supported in Sui Move"),
            })
    }

    /// Return all of the bytecode modules in this package and the modules of its direct and transitive dependencies.
    /// Note: these are not topologically sorted by dependency.
    pub fn get_modules_and_deps(&self) -> impl Iterator<Item = &CompiledModule> {
        self.package.all_modules().map(|m| match &m.unit {
            CompiledUnitEnum::Module(m) => &m.module,
            CompiledUnitEnum::Script(_) => unimplemented!("Scripts not supported in Sui Move"),
        })
    }

    /// Return the bytecode modules in this package, topologically sorted in dependency order
    /// This is the function to call if you would like to publish or statically analyze the modules
    pub fn get_dependency_sorted_modules(&self) -> Vec<CompiledModule> {
        let compiled_modules = self.package.root_modules_map();
        // Collect all module IDs from the current package to be
        // published (module names are not sufficient as we may
        // have modules with the same names in user code and in
        // Sui framework which would result in the latter being
        // pulled into a set of modules to be published).
        // For each transitive dependent module, if they are not to be published,
        // they must have a non-zero address (meaning they are already published on-chain).
        let self_modules: HashSet<ModuleId> = compiled_modules
            .iter_modules()
            .iter()
            .map(|m| m.self_id())
            .collect();
        self.package
            .all_modules_map()
            .compute_dependency_graph()
            .compute_topological_order()
            .unwrap() // safe because package built successfully
            .filter(|m| self_modules.contains(&m.self_id()))
            .cloned()
            .collect()
    }

    /// Return a serialized representation of the bytecode modules in this package, topologically sorted in dependency order
    pub fn get_package_bytes(&self) -> Vec<Vec<u8>> {
        self.get_dependency_sorted_modules()
            .iter()
            .map(|m| {
                let mut bytes = Vec::new();
                m.serialize(&mut bytes).unwrap(); // safe because package built successfully
                bytes
            })
            .collect()
    }

    /// Return the base64-encoded representation of the bytecode modules in this package, topologically sorted in dependency order
    pub fn get_package_base64(&self) -> Vec<Base64> {
        self.get_package_bytes()
            .iter()
            .map(|b| Base64::from_bytes(b))
            .collect()
    }

    /// Get bytecode modules from the Sui Framework that are used by this package
    pub fn get_framework_modules(&self) -> impl Iterator<Item = &CompiledModule> {
        self.get_modules_and_deps()
            .filter(|m| *m.self_id().address() == SUI_FRAMEWORK_ADDRESS)
    }

    /// Get bytecode modules from the Move stdlib that are used by this package
    pub fn get_stdlib_modules(&self) -> impl Iterator<Item = &CompiledModule> {
        self.get_modules_and_deps()
            .filter(|m| *m.self_id().address() == MOVE_STDLIB_ADDRESS)
    }

    /// Version of the framework code that the binary used for compilation expects should be the same as
    /// version of the framework code bundled as compiled package's dependency and this function
    /// verifies this.
    // TODO: replace this with actual versioning checks instead of hacky byte-for-byte comparisons
    pub fn verify_framework_version(
        &self,
        ext_sui_framework: Vec<CompiledModule>,
        ext_move_stdlib: Vec<CompiledModule>,
    ) -> SuiResult<()> {
        // We stash compiled modules in the Modules map which is sorted so that we can compare sets of
        // compiled modules directly.
        let ext_framework_modules = Modules::new(ext_sui_framework.iter());
        let pkg_framework_modules: Vec<&CompiledModule> = self.get_framework_modules().collect();

        // compare framework modules pulled as dependencies (if any - a developer may choose to use only
        // stdlib) with framework modules bundled with the distribution
        if !pkg_framework_modules.is_empty()
            && ext_framework_modules != Modules::new(pkg_framework_modules)
        {
            // note: this advice is overfitted to the most common failure modes we see:
            // user is trying to publish to testnet, but has a `sui` binary and Sui Framework
            // sources that are not in sync. the first part of the advice ensures that the
            // user's project is always pointing at the devnet copy of the `Sui` Framework.
            // the second ensures that the `sui` binary matches the devnet framework
            return Err(SuiError::ModuleVerificationFailure {
            error: "Sui framework version mismatch detected.\
		    Make sure that you are using a GitHub dep in your Move.toml:\
		    \
                    [dependencies]
                    Sui = { git = \"https://github.com/MystenLabs/sui.git\", subdir = \"crates/sui-framework\", rev = \"devnet\" }
`                   \
                    If that does not fix the issue, your `sui` binary is likely out of date--try \
                    cargo install --locked --git https://github.com/MystenLabs/sui.git --branch devnet sui"
                .to_string(),
        });
        }

        let ext_stdlib_modules = Modules::new(ext_move_stdlib.iter());
        let pkg_stdlib_modules: Vec<&CompiledModule> = self.get_stdlib_modules().collect();

        // compare stdlib modules pulled as dependencies (if any) with stdlib modules bundled with the
        // distribution
        if !pkg_stdlib_modules.is_empty() && ext_stdlib_modules != Modules::new(pkg_stdlib_modules)
        {
            return Err(SuiError::ModuleVerificationFailure {
                error: "Move stdlib version mismatch detected.\
                    Make sure that the sui command line tool and the Move standard library code\
                    used as a dependency correspond to the same git commit"
                    .to_string(),
            });
        }

        Ok(())
    }

    /// Generate layout schemas for all types declared by this package, as well as
    /// all struct types passed into `entry` functions declared by modules in this package
    /// (either directly or by reference).
    /// These layout schemas can be consumed by clients (e.g., the TypeScript SDK) to enable
    /// BCS serialization/deserialization of the package's objects, tx arguments, and events.
    pub fn generate_struct_layouts(&self) -> Registry {
        let mut package_types = BTreeSet::new();
        for m in self.get_modules() {
            let normalized_m = normalized::Module::new(m);
            // 1. generate struct layouts for all declared types
            'structs: for (name, s) in normalized_m.structs {
                let mut dummy_type_parameters = Vec::new();
                for t in &s.type_parameters {
                    if t.is_phantom {
                        // if all of t's type parameters are phantom, we can generate a type layout
                        // we make this happen by creating a StructTag with dummy `type_params`, since the layout generator won't look at them.
                        // we need to do this because SerdeLayoutBuilder will refuse to generate a layout for any open StructTag, but phantom types
                        // cannot affect the layout of a struct, so we just use dummy values
                        dummy_type_parameters.push(TypeTag::Signer)
                    } else {
                        // open type--do not attempt to generate a layout
                        // TODO: handle generating layouts for open types?
                        continue 'structs;
                    }
                }
                debug_assert!(dummy_type_parameters.len() == s.type_parameters.len());
                package_types.insert(StructTag {
                    address: *m.address(),
                    module: m.name().to_owned(),
                    name,
                    type_params: dummy_type_parameters,
                });
            }
            // 2. generate struct layouts for all parameters of `entry` funs
            for (_name, f) in normalized_m.exposed_functions {
                if f.is_entry {
                    for t in f.parameters {
                        let tag_opt = match t.clone() {
                            Type::Address
                            | Type::Bool
                            | Type::Signer
                            | Type::TypeParameter(_)
                            | Type::U8
                            | Type::U16
                            | Type::U32
                            | Type::U64
                            | Type::U128
                            | Type::U256
                            | Type::Vector(_) => continue,
                            Type::Reference(t) | Type::MutableReference(t) => t.into_struct_tag(),
                            s @ Type::Struct { .. } => s.into_struct_tag(),
                        };
                        if let Some(tag) = tag_opt {
                            package_types.insert(tag);
                        }
                    }
                }
            }
        }
        let mut layout_builder = SerdeLayoutBuilder::new(self);
        for typ in &package_types {
            layout_builder.build_struct_layout(typ).unwrap();
        }
        layout_builder.into_registry()
    }
}

impl Default for BuildConfig {
    fn default() -> Self {
        BuildConfig {
            config: MoveBuildConfig::default(),
            run_bytecode_verifier: true,
            print_diags_to_stderr: false,
        }
    }
}

impl GetModule for CompiledPackage {
    type Error = anyhow::Error;
    // TODO: return ref here for better efficiency? Borrow checker + all_modules_map() make it hard to do this
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<Self::Item>, Self::Error> {
        Ok(self.package.all_modules_map().get_module(id).ok().cloned())
    }
}
