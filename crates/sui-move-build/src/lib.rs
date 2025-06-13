// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

extern crate move_ir_types;

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
};

use sui_package_alt::SuiFlavor;
use toml::Value as TV;

use anyhow::bail;
use anyhow::Context;
use fastcrypto::encoding::Base64;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_compiler::{
    compiled_unit::AnnotatedCompiledModule,
    diagnostics::{report_diagnostics_to_buffer, report_warnings, Diagnostics},
    linters::LINT_WARNING_PREFIX,
    shared::files::MappedFiles,
};
use move_core_types::{account_address::AccountAddress, language_storage::ModuleId};
// use move_package::{
//     compilation::{
//         build_plan::BuildPlan, compiled_package::CompiledPackage as MoveCompiledPackage,
//     },
//     // package_hooks::{PackageHooks, PackageIdentifier},
//     resolution::{dependency_graph::DependencyGraph, resolution_graph::ResolvedGraph},
//     source_package::parsed_manifest::{
//         Dependencies, Dependency, DependencyKind, GitInfo, InternalDependency, PackageName,
//     },
// };
// use move_package::{
//     source_package::parsed_manifest::OnChainInfo, source_package::parsed_manifest::SourceManifest,
// };
use move_package_alt::{
    compatibility::legacy_parser::{parse_package_info, LegacyPackageMetadata},
    flavor::MoveFlavor,
    package::RootPackage,
    schema::Environment,
};
use move_package_alt_compilation::compiled_package::{
    CompiledPackage as MoveCompiledPackage, // CompiledUnitWithSource,
};
use move_package_alt_compilation::{
    build_config::BuildConfig as MoveBuildConfig, build_plan::BuildPlan,
};
use move_symbol_pool::Symbol;
// use serde_reflection::Registry;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::{
    base_types::ObjectID,
    error::{SuiError, SuiResult},
    is_system_package,
    move_package::{FnInfo, FnInfoKey, FnInfoMap, MovePackage},
    BRIDGE_ADDRESS, DEEPBOOK_ADDRESS, MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS,
    SUI_SYSTEM_ADDRESS,
};
use sui_verifier::verifier as sui_bytecode_verifier;

const PACKAGE: &str = "package";

#[cfg(test)]
#[path = "unit_tests/build_tests.rs"]
mod build_tests;

pub mod test_utils {
    // use crate::{BuildConfig, CompiledPackage, SuiPackageHooks};
    // use std::path::PathBuf;

    // pub fn compile_basics_package() -> CompiledPackage {
    //     compile_example_package("../../examples/move/basics")
    // }
    //
    // pub fn compile_managed_coin_package() -> CompiledPackage {
    //     compile_example_package("../../crates/sui-core/src/unit_tests/data/managed_coin")
    // }
    //
    // pub fn compile_example_package(relative_path: &str) -> CompiledPackage {
    //     move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    //     let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    //     path.push(relative_path);
    //
    //     BuildConfig::new_for_testing().build(&path).unwrap()
    // }
}

/// Wrapper around the core Move `CompiledPackage` with some Sui-specific traits and info
#[derive(Debug, Clone)]
pub struct CompiledPackage {
    pub package: MoveCompiledPackage,
    /// Address the package is recorded as being published at.
    pub published_at: Option<ObjectID>,
    /// The dependency IDs of this package
    pub dependency_ids: Vec<ObjectID>,
    // Transitive dependency graph of a Move package
    // pub dependency_graph: Vec<PackageInfo<SuiFlavor>>,
}

/// Wrapper around the core Move `BuildConfig` with some Sui-specific info
#[derive(Clone)]
pub struct BuildConfig {
    pub config: MoveBuildConfig,
    /// If true, run the Move bytecode verifier on the bytecode from a successful build
    pub run_bytecode_verifier: bool,
    /// If true, print build diagnostics to stderr--no printing if false
    pub print_diags_to_stderr: bool,
    /// The chain ID that compilation is with respect to (e.g., required to resolve
    /// published dependency IDs from the `Move.lock`).
    pub chain_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PackageDependencies {
    /// Set of published dependencies (name and address).
    pub published: BTreeMap<Symbol, ObjectID>,
    /// Set of unpublished dependencies (name).
    pub unpublished: BTreeSet<Symbol>,
    /// Set of dependencies with invalid `published-at` addresses.
    pub invalid: BTreeMap<Symbol, String>,
    /// Set of dependencies that have conflicting `published-at` addresses. The key refers to
    /// the package, and the tuple refers to the address in the (Move.lock, Move.toml) respectively.
    pub conflicting: BTreeMap<Symbol, (ObjectID, ObjectID)>,
}
//

impl BuildConfig {
    pub fn new_for_testing() -> Self {
        let install_dir = mysten_common::tempdir().unwrap().keep();
        let config = MoveBuildConfig {
            default_flavor: Some(move_compiler::editions::Flavor::Sui),

            lock_file: Some(install_dir.join("Move.lock")),
            install_dir: Some(install_dir),
            silence_warnings: true,
            lint_flag: move_package_alt_compilation::lint_flag::LintFlag::LEVEL_NONE,
            ..MoveBuildConfig::default()
        };
        BuildConfig {
            config,
            run_bytecode_verifier: true,
            print_diags_to_stderr: false,
            chain_id: None,
        }
    }

    pub fn new_for_testing_replace_addresses<I, S>(dep_original_addresses: I) -> Self
    where
        I: IntoIterator<Item = (S, ObjectID)>,
        S: Into<String>,
    {
        todo!()
        // let mut build_config = Self::new_for_testing();
        // for (addr_name, obj_id) in dep_original_addresses {
        //     build_config
        //         .config
        //         .additional_named_addresses
        //         .insert(addr_name.into(), AccountAddress::from(obj_id));
        // }
        // build_config
    }

    fn fn_info(units: &[AnnotatedCompiledModule]) -> FnInfoMap {
        let mut fn_info_map = BTreeMap::new();
        for u in units {
            let mod_addr = u.named_module.address.into_inner();
            let mod_is_test = u.attributes.is_test_or_test_only();
            for (_, s, info) in &u.function_infos {
                let fn_name = s.as_str().to_string();
                let is_test = mod_is_test || info.attributes.is_test_or_test_only();
                fn_info_map.insert(FnInfoKey { fn_name, mod_addr }, FnInfo { is_test });
            }
        }

        fn_info_map
    }

    fn compile_package<W: Write, F: MoveFlavor>(
        &self,
        root_pkg: RootPackage<F>,
        writer: &mut W,
    ) -> anyhow::Result<(MoveCompiledPackage, FnInfoMap)> {
        let build_plan = BuildPlan::create(root_pkg, &self.config)?;
        let mut fn_info = None;
        let compiled_pkg = build_plan.compile_with_driver(writer, |compiler| {
            let (files, units_res) = compiler.build()?;
            match units_res {
                Ok((units, warning_diags)) => {
                    decorate_warnings(warning_diags, Some(&files));
                    fn_info = Some(Self::fn_info(&units));
                    Ok((files, units))
                }
                Err(error_diags) => {
                    // with errors present don't even try decorating warnings output to avoid
                    // clutter
                    assert!(!error_diags.is_empty());
                    let diags_buf =
                        report_diagnostics_to_buffer(&files, error_diags, /* color */ true);
                    if let Err(err) = std::io::stderr().write_all(&diags_buf) {
                        anyhow::bail!("Cannot output compiler diagnostics: {}", err);
                    }
                    anyhow::bail!("Compilation error");
                }
            }
        })?;
        Ok((compiled_pkg, fn_info.unwrap()))
    }

    /// Given a `path` and a `build_config`, build the package in that path, including its dependencies.
    /// If we are building the Sui framework, we skip the check that the addresses should be 0
    pub fn build(self, path: &Path) -> anyhow::Result<CompiledPackage> {
        let envs = RootPackage::<SuiFlavor>::environments(path)?;
        let env = if let Some(ref e) = self.config.environment {
            if let Some(env) = envs.get(e) {
                Environment::new(e.to_string(), env.to_string())
            } else {
                bail!(
                    "No environment named `{e}` in the manifest. Available environments are {:?}",
                    envs.keys()
                );
            }
        } else {
            let (name, id) = envs.first_key_value().expect("At least one default env");
            Environment::new(name.to_string(), id.to_string())
        };

        // we need to block here to compile the package, which requires to fetch dependencies
        let root_pkg = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            // We're already in a tokio runtime
            match handle.runtime_flavor() {
                tokio::runtime::RuntimeFlavor::MultiThread => {
                    // Multi-threaded runtime, can use block_in_place
                    tokio::task::block_in_place(|| {
                        handle.block_on(RootPackage::<SuiFlavor>::load(path, env))
                    })
                }
                _ => {
                    // Single-threaded or current-thread runtime, use futures::executor
                    futures::executor::block_on(RootPackage::<SuiFlavor>::load(path, env))
                }
            }
        } else {
            // No runtime exists, create one
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(RootPackage::<SuiFlavor>::load(path, env))
        }?;

        root_pkg.save_to_disk()?;

        let result = if self.print_diags_to_stderr {
            self.compile_package(root_pkg, &mut std::io::stderr())
        } else {
            self.compile_package(root_pkg, &mut std::io::sink())
        };

        let (package, fn_info) = result.map_err(|error| SuiError::ModuleBuildFailure {
            // Use [Debug] formatting to capture [anyhow] error context
            error: format!("{:?}", error),
        })?;

        if self.run_bytecode_verifier {
            verify_bytecode(&package, &fn_info)?;
        }

        let dependency_ids = package
            .dependency_ids()
            .iter()
            .map(|x| ObjectID::from(x.0))
            .collect();

        Ok(CompiledPackage {
            package,
            dependency_ids,
            published_at: None, // TODO fix this once backward compatibility lands
                                // dependency_graph,
        })
    }
}

/// There may be additional information that needs to be displayed after diagnostics are reported
/// (optionally report diagnostics themselves if files argument is provided).
pub fn decorate_warnings(warning_diags: Diagnostics, files: Option<&MappedFiles>) {
    let any_linter_warnings = warning_diags.any_with_prefix(LINT_WARNING_PREFIX);
    let (filtered_diags_num, unique) =
        warning_diags.filtered_source_diags_with_prefix(LINT_WARNING_PREFIX);
    if let Some(f) = files {
        report_warnings(f, warning_diags);
    }
    if any_linter_warnings {
        eprintln!("Please report feedback on the linter warnings at https://forums.sui.io\n");
    }
    if filtered_diags_num > 0 {
        eprintln!("Total number of linter warnings suppressed: {filtered_diags_num} (unique lints: {unique})");
    }
}

/// Check that the compiled modules in `package` are valid
fn verify_bytecode(package: &MoveCompiledPackage, fn_info: &FnInfoMap) -> SuiResult<()> {
    let compiled_modules = package.root_modules_map();
    let verifier_config = ProtocolConfig::get_for_version(ProtocolVersion::MAX, Chain::Unknown)
        .verifier_config(/* signing_limits */ None);

    for m in compiled_modules.iter_modules() {
        move_bytecode_verifier::verify_module_unmetered(m).map_err(|err| {
            SuiError::ModuleVerificationFailure {
                error: err.to_string(),
            }
        })?;
        sui_bytecode_verifier::sui_verify_module_unmetered(m, fn_info, &verifier_config)?;
    }
    // TODO(https://github.com/MystenLabs/sui/issues/69): Run Move linker

    Ok(())
}

impl CompiledPackage {
    /// Return all of the bytecode modules in this package (not including direct or transitive deps)
    /// Note: these are not topologically sorted by dependency--use `get_dependency_sorted_modules` to produce a list of modules suitable
    /// for publishing or static analysis
    pub fn get_modules(&self) -> impl Iterator<Item = &CompiledModule> {
        self.package.root_modules().map(|m| &m.unit.module)
    }

    /// Return all of the bytecode modules in this package (not including direct or transitive deps)
    /// Note: these are not topologically sorted by dependency--use `get_dependency_sorted_modules` to produce a list of modules suitable
    /// for publishing or static analysis
    pub fn into_modules(self) -> Vec<CompiledModule> {
        self.package
            .root_compiled_units
            .into_iter()
            .map(|m| m.unit.module)
            .collect()
    }

    /// Return all of the bytecode modules that this package depends on (both directly and transitively)
    /// Note: these are not topologically sorted by dependency.
    pub fn get_dependent_modules(&self) -> impl Iterator<Item = &CompiledModule> {
        self.package
            .deps_compiled_units
            .iter()
            .map(|(_, m)| &m.unit.module)
    }

    /// Return all of the bytecode modules in this package and the modules of its direct and transitive dependencies.
    /// Note: these are not topologically sorted by dependency.
    pub fn get_modules_and_deps(&self) -> impl Iterator<Item = &CompiledModule> {
        self.package
            .all_compiled_units_with_source()
            .map(|m| &m.unit.module)
    }
    //
    //     /// Return the bytecode modules in this package, topologically sorted in dependency order.
    //     /// Optionally include dependencies that have not been published (are at address 0x0), if
    //     /// `with_unpublished_deps` is true. This is the function to call if you would like to publish
    //     /// or statically analyze the modules.
    //     pub fn get_dependency_sorted_modules(
    //         &self,
    //         with_unpublished_deps: bool,
    //     ) -> Vec<CompiledModule> {
    //         let all_modules = Modules::new(self.get_modules_and_deps());
    //
    //         // SAFETY: package built successfully
    //         let modules = all_modules.compute_topological_order().unwrap();
    //
    //         if with_unpublished_deps {
    //             // For each transitive dependent module, if they are not to be published, they must have
    //             // a non-zero address (meaning they are already published on-chain).
    //             modules
    //                 .filter(|module| module.address() == &AccountAddress::ZERO)
    //                 .cloned()
    //                 .collect()
    //         } else {
    //             // Collect all module IDs from the current package to be published (module names are not
    //             // sufficient as we may have modules with the same names in user code and in Sui
    //             // framework which would result in the latter being pulled into a set of modules to be
    //             // published).
    //             let self_modules: HashSet<_> = self
    //                 .package
    //                 .root_modules_map()
    //                 .iter_modules()
    //                 .iter()
    //                 .map(|m| m.self_id())
    //                 .collect();
    //
    //             modules
    //                 .filter(|module| self_modules.contains(&module.self_id()))
    //                 .cloned()
    //                 .collect()
    //         }
    //     }
    //
    /// Return the set of Object IDs corresponding to this package's transitive dependencies'
    /// storage package IDs (where to load those packages on-chain).
    pub fn get_dependency_storage_package_ids(&self) -> Vec<ObjectID> {
        self.dependency_ids.clone()
    }

    /// Return a digest of the bytecode modules in this package.
    pub fn get_package_digest(&self) -> [u8; 32] {
        let hash_modules = true;
        MovePackage::compute_digest_for_modules_and_deps(
            &self.get_package_bytes(),
            &self.dependency_ids,
            hash_modules,
        )
    }

    /// Return a serialized representation of the bytecode modules in this package, topologically sorted in dependency order
    pub fn get_package_bytes(&self) -> Vec<Vec<u8>> {
        self.package.get_package_bytes()
    }

    /// Return the base64-encoded representation of the bytecode modules in this package, topologically sorted in dependency order
    pub fn get_package_base64(&self) -> Vec<Base64> {
        self.get_package_bytes()
            .iter()
            .map(|b| Base64::from_bytes(b))
            .collect()
    }

    /// Get bytecode modules from DeepBook that are used by this package
    pub fn get_deepbook_modules(&self) -> impl Iterator<Item = &CompiledModule> {
        self.get_modules_and_deps()
            .filter(|m| *m.self_id().address() == DEEPBOOK_ADDRESS)
    }

    /// Get bytecode modules from DeepBook that are used by this package
    pub fn get_bridge_modules(&self) -> impl Iterator<Item = &CompiledModule> {
        self.get_modules_and_deps()
            .filter(|m| *m.self_id().address() == BRIDGE_ADDRESS)
    }

    /// Get bytecode modules from the Sui System that are used by this package
    pub fn get_sui_system_modules(&self) -> impl Iterator<Item = &CompiledModule> {
        self.get_modules_and_deps()
            .filter(|m| *m.self_id().address() == SUI_SYSTEM_ADDRESS)
    }

    /// Get bytecode modules from the Sui Framework that are used by this package
    pub fn get_sui_framework_modules(&self) -> impl Iterator<Item = &CompiledModule> {
        self.get_modules_and_deps()
            .filter(|m| *m.self_id().address() == SUI_FRAMEWORK_ADDRESS)
    }

    /// Get bytecode modules from the Move stdlib that are used by this package
    pub fn get_stdlib_modules(&self) -> impl Iterator<Item = &CompiledModule> {
        self.get_modules_and_deps()
            .filter(|m| *m.self_id().address() == MOVE_STDLIB_ADDRESS)
    }
    //
    //     /// Generate layout schemas for all types declared by this package, as well as
    //     /// all struct types passed into `entry` functions declared by modules in this package
    //     /// (either directly or by reference).
    //     /// These layout schemas can be consumed by clients (e.g., the TypeScript SDK) to enable
    //     /// BCS serialization/deserialization of the package's objects, tx arguments, and events.
    //     pub fn generate_struct_layouts(&self) -> Registry {
    //         let pool = &mut normalized::RcPool::new();
    //         let mut package_types = BTreeSet::new();
    //         for m in self.get_modules() {
    //             let normalized_m = normalized::Module::new(pool, m, /* include code */ false);
    //             // 1. generate struct layouts for all declared types
    //             'structs: for (name, s) in normalized_m.structs {
    //                 let mut dummy_type_parameters = Vec::new();
    //                 for t in &s.type_parameters {
    //                     if t.is_phantom {
    //                         // if all of t's type parameters are phantom, we can generate a type layout
    //                         // we make this happen by creating a StructTag with dummy `type_params`, since the layout generator won't look at them.
    //                         // we need to do this because SerdeLayoutBuilder will refuse to generate a layout for any open StructTag, but phantom types
    //                         // cannot affect the layout of a struct, so we just use dummy values
    //                         dummy_type_parameters.push(TypeTag::Signer)
    //                     } else {
    //                         // open type--do not attempt to generate a layout
    //                         // TODO: handle generating layouts for open types?
    //                         continue 'structs;
    //                     }
    //                 }
    //                 debug_assert!(dummy_type_parameters.len() == s.type_parameters.len());
    //                 package_types.insert(StructTag {
    //                     address: *m.address(),
    //                     module: m.name().to_owned(),
    //                     name: name.as_ident_str().to_owned(),
    //                     type_params: dummy_type_parameters,
    //                 });
    //             }
    //             // 2. generate struct layouts for all parameters of `entry` funs
    //             for (_name, f) in normalized_m.functions {
    //                 if f.is_entry {
    //                     for t in &*f.parameters {
    //                         let tag_opt = match &**t {
    //                             Type::Address
    //                             | Type::Bool
    //                             | Type::Signer
    //                             | Type::TypeParameter(_)
    //                             | Type::U8
    //                             | Type::U16
    //                             | Type::U32
    //                             | Type::U64
    //                             | Type::U128
    //                             | Type::U256
    //                             | Type::Vector(_) => continue,
    //                             Type::Reference(_, inner) => inner.to_struct_tag(pool),
    //                             Type::Datatype(_) => t.to_struct_tag(pool),
    //                         };
    //                         if let Some(tag) = tag_opt {
    //                             package_types.insert(tag);
    //                         }
    //                     }
    //                 }
    //             }
    //         }
    //         let mut layout_builder = SerdeLayoutBuilder::new(self);
    //         for typ in &package_types {
    //             layout_builder.build_data_layout(typ).unwrap();
    //         }
    //         layout_builder.into_registry()
    //     }
    //
    /// Checks whether this package corresponds to a built-in framework
    pub fn is_system_package(&self) -> bool {
        // System packages always have "published-at" addresses
        let Some(published_at) = self.published_at else {
            return false;
        };

        is_system_package(published_at)
    }

    /// Checks for root modules with non-zero package addresses.  Returns an arbitrary one, if one
    /// can can be found, otherwise returns `None`.
    pub fn published_root_module(&self) -> Option<&CompiledModule> {
        self.package.root_compiled_units.iter().find_map(|unit| {
            if unit.unit.module.self_id().address() != &AccountAddress::ZERO {
                Some(&unit.unit.module)
            } else {
                None
            }
        })
    }
    //
    //     pub fn verify_unpublished_dependencies(
    //         &self,
    //         unpublished_deps: &BTreeSet<Symbol>,
    //     ) -> SuiResult<()> {
    //         if unpublished_deps.is_empty() {
    //             return Ok(());
    //         }
    //
    //         let errors = self
    //             .package
    //             .deps_compiled_units
    //             .iter()
    //             .filter_map(|(p, m)| {
    //                 if !unpublished_deps.contains(p) || m.unit.module.address() == &AccountAddress::ZERO
    //                 {
    //                     return None;
    //                 }
    //                 Some(format!(
    //                     " - {}::{} in dependency {}",
    //                     m.unit.module.address(),
    //                     m.unit.name,
    //                     p
    //                 ))
    //             })
    //             .collect::<Vec<String>>();
    //
    //         if errors.is_empty() {
    //             return Ok(());
    //         }
    //
    //         let mut error_message = vec![];
    //         error_message.push(
    //             "The following modules in package dependencies set a non-zero self-address:".into(),
    //         );
    //         error_message.extend(errors);
    //         error_message.push(
    //             "If these packages really are unpublished, their self-addresses should be set \
    // 	     to \"0x0\" in the [addresses] section of the manifest when publishing. If they \
    // 	     are already published, ensure they specify the address in the `published-at` of \
    // 	     their Move.toml manifest."
    //                 .into(),
    //         );
    //
    //         Err(SuiError::ModulePublishFailure {
    //             error: error_message.join("\n"),
    //         })
    //     }
    //
    pub fn get_published_dependencies_ids(&self) -> Vec<ObjectID> {
        self.dependency_ids.clone()
    }
    //
    //     /// Find the map of packages that are immediate dependencies of the root modules, joined with
    //     /// the set of bytecode dependencies.
    //     pub fn find_immediate_deps_pkgs_to_keep(
    //         &self,
    //         with_unpublished_deps: bool,
    //     ) -> Result<BTreeMap<Symbol, ObjectID>, anyhow::Error> {
    //         // Start from the root modules (or all modules if with_unpublished_deps is true as we
    //         // need to include modules with 0x0 address)
    //         let root_modules: Vec<_> = if with_unpublished_deps {
    //             self.package
    //                 .all_compiled_units_with_source()
    //                 .filter(|m| m.unit.address.into_inner() == AccountAddress::ZERO)
    //                 .map(|x| x.unit.clone())
    //                 .collect()
    //         } else {
    //             self.package
    //                 .root_modules()
    //                 .map(|x| x.unit.clone())
    //                 .collect()
    //         };
    //
    //         // Find the immediate dependencies for each root module and store the package name
    //         // in the pkgs_to_keep set. This basically prunes the packages that are not used
    //         // based on the modules information.
    //         let mut pkgs_to_keep: BTreeSet<Symbol> = BTreeSet::new();
    //         let module_to_pkg_name: BTreeMap<_, _> = self
    //             .package
    //             .all_modules()
    //             .map(|m| (m.unit.module.self_id(), m.unit.package_name))
    //             .collect();
    //
    //         for module in &root_modules {
    //             let immediate_deps = module.module.immediate_dependencies();
    //             for dep in immediate_deps {
    //                 if let Some(pkg_name) = module_to_pkg_name.get(&dep) {
    //                     let Some(pkg_name) = pkg_name else {
    //                         bail!("Expected a package name but it's None")
    //                     };
    //                     pkgs_to_keep.insert(*pkg_name);
    //                 }
    //             }
    //         }
    //
    //         // If a package depends on another published package that has only bytecode without source
    //         // code available, we need to include also that package as dep.
    //         pkgs_to_keep.extend(self.bytecode_deps.iter().map(|(name, _)| *name));
    //
    //         // Finally, filter out packages that are published and exist in the manifest at the
    //         // compilation time but are not referenced in the source code.
    //         Ok(self
    //             .dependency_ids
    //             .clone()
    //             .published
    //             .into_iter()
    //             .filter(|(pkg_name, _)| pkgs_to_keep.contains(pkg_name))
    //             .collect())
    //     }
}

// /// Create a set of [Dependencies] from a [SystemPackagesVersion]; the dependencies are override git
// /// dependencies to the specific revision given by the [SystemPackagesVersion]
// ///
// /// Skips "Deepbook" dependency.
// pub fn implicit_deps(packages: &SystemPackagesVersion) -> Dependencies {
//     let deps_to_skip = ["DeepBook".to_string()];
//     packages
//         .packages
//         .iter()
//         .filter(|package| !deps_to_skip.contains(&package.package_name))
//         .map(|package| {
//             (
//                 package.package_name.clone().into(),
//                 Dependency::Internal(InternalDependency {
//                     kind: DependencyKind::Git(GitInfo {
//                         git_url: SYSTEM_GIT_REPO.into(),
//                         git_rev: packages.git_revision.clone().into(),
//                         subdir: package.repo_path.clone().into(),
//                     }),
//                     subst: None,
//                     digest: None,
//                     dep_override: true,
//                 }),
//             )
//         })
//         .collect()
// }

impl GetModule for CompiledPackage {
    type Error = anyhow::Error;
    // TODO: return ref here for better efficiency? Borrow checker + all_modules_map() make it hard to do this
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<Self::Item>, Self::Error> {
        Ok(self.package.all_modules_map().get_module(id).ok().cloned())
    }
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum PublishedAtError {
    #[error("The 'published-at' field in Move.toml or Move.lock is invalid: {0:?}")]
    Invalid(String),
    #[error("The 'published-at' field is not present in Move.toml or Move.lock")]
    NotPresent,
}

pub fn parse_legacy_package_info(
    package_path: &PathBuf,
) -> Result<LegacyPackageMetadata, anyhow::Error> {
    let manifest_string = std::fs::read_to_string(package_path.join("Move.toml"))?;
    let tv =
        toml::from_str::<TV>(&manifest_string).context("Unable to parse Move package manifest")?;

    match tv {
        TV::Table(mut table) => {
            let metadata = table
                .remove(PACKAGE)
                .map(parse_package_info)
                .transpose()
                .context("Error parsing '[package]' section of manifest")?
                .unwrap();
            return Ok(metadata);
        }
        _ => bail!("Expected a table from the manifest file"),
    }
}

pub fn published_at_property(package_path: &PathBuf) -> Result<ObjectID, PublishedAtError> {
    let parsed_manifest =
        parse_legacy_package_info(package_path).expect("should read the manifest");

    let Some(value) = parsed_manifest.published_at else {
        return Err(PublishedAtError::NotPresent);
    };

    ObjectID::from_str(value.as_str())
        .map_err(|_| PublishedAtError::Invalid(value.as_str().to_owned()))
}
