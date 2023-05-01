// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
};

use fastcrypto::encoding::Base64;
use move_binary_format::{
    access::ModuleAccess,
    normalized::{self, Type},
    CompiledModule,
};
use move_bytecode_utils::{layout::SerdeLayoutBuilder, module_cache::GetModule};
use move_compiler::{
    compiled_unit::{
        AnnotatedCompiledModule, AnnotatedCompiledScript, CompiledUnitEnum, NamedCompiledModule,
    },
    diagnostics::{report_diagnostics_to_color_buffer, report_warnings},
    expansion::ast::{AttributeName_, Attributes},
    shared::known_attributes::KnownAttribute,
};
use move_core_types::{
    account_address::AccountAddress,
    language_storage::{ModuleId, StructTag, TypeTag},
};
use move_package::{
    compilation::{
        build_plan::BuildPlan, compiled_package::CompiledPackage as MoveCompiledPackage,
    },
    package_hooks::PackageHooks,
    resolution::resolution_graph::ResolvedGraph,
    BuildConfig as MoveBuildConfig,
};
use move_package::{
    resolution::resolution_graph::Package, source_package::parsed_manifest::CustomDepInfo,
};
use move_symbol_pool::Symbol;
use serde_reflection::Registry;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion};
use sui_types::{
    base_types::ObjectID,
    error::{SuiError, SuiResult},
    is_system_package,
    move_package::{FnInfo, FnInfoKey, FnInfoMap, MovePackage},
    DEEPBOOK_ADDRESS, MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_ADDRESS,
};
use sui_verifier::verifier as sui_bytecode_verifier;

#[cfg(test)]
#[path = "unit_tests/build_tests.rs"]
mod build_tests;

/// Wrapper around the core Move `CompiledPackage` with some Sui-specific traits and info
#[derive(Debug)]
pub struct CompiledPackage {
    pub package: MoveCompiledPackage,
    /// Address the package is recorded as being published at.
    pub published_at: Result<ObjectID, PublishedAtError>,
    /// The dependency IDs of this package
    pub dependency_ids: PackageDependencies,
    /// Path to the Move package (i.e., where the Move.toml file is)
    pub path: PathBuf,
}

/// Wrapper around the core Move `BuildConfig` with some Sui-specific info
#[derive(Clone)]
pub struct BuildConfig {
    pub config: MoveBuildConfig,
    /// If true, run the Move bytecode verifier on the bytecode from a successful build
    pub run_bytecode_verifier: bool,
    /// If true, print build diagnostics to stderr--no printing if false
    pub print_diags_to_stderr: bool,
}

impl BuildConfig {
    pub fn new_for_testing() -> Self {
        move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
        let mut build_config: Self = Default::default();
        let install_dir = tempfile::tempdir().unwrap().into_path();
        let lock_file = install_dir.join("Move.lock");
        build_config.config.install_dir = Some(install_dir);
        build_config.config.lock_file = Some(lock_file);
        build_config
    }

    fn is_test(attributes: &Attributes) -> bool {
        attributes
            .iter()
            .any(|(_, name, _)| matches!(name, AttributeName_::Known(KnownAttribute::Testing(_))))
    }

    fn fn_info(
        units: &[CompiledUnitEnum<AnnotatedCompiledModule, AnnotatedCompiledScript>],
    ) -> FnInfoMap {
        let mut fn_info_map = BTreeMap::new();
        for u in units {
            match u {
                CompiledUnitEnum::Module(m) => {
                    let mod_addr = m.named_module.address.into_inner();
                    for (_, s, info) in &m.function_infos {
                        let fn_name = s.as_str().to_string();
                        let is_test = Self::is_test(&info.attributes);
                        fn_info_map.insert(FnInfoKey { fn_name, mod_addr }, FnInfo { is_test });
                    }
                }
                CompiledUnitEnum::Script(_) => continue,
            }
        }

        fn_info_map
    }

    fn compile_package<W: Write>(
        resolution_graph: ResolvedGraph,
        writer: &mut W,
    ) -> anyhow::Result<(MoveCompiledPackage, FnInfoMap)> {
        let build_plan = BuildPlan::create(resolution_graph)?;
        let mut fn_info = None;
        let compiled_pkg = build_plan.compile_with_driver(writer, |compiler| {
            let (files, units_res) = compiler.build()?;
            match units_res {
                Ok((units, warning_diags)) => {
                    report_warnings(&files, warning_diags);
                    fn_info = Some(Self::fn_info(&units));
                    Ok((files, units))
                }
                Err(error_diags) => {
                    assert!(!error_diags.is_empty());
                    let diags_buf = report_diagnostics_to_color_buffer(&files, error_diags);
                    if let Err(err) = std::io::stdout().write_all(&diags_buf) {
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
    pub fn build(self, path: PathBuf) -> SuiResult<CompiledPackage> {
        let print_diags_to_stderr = self.print_diags_to_stderr;
        let run_bytecode_verifier = self.run_bytecode_verifier;
        let resolution_graph = self.resolution_graph(&path)?;
        build_from_resolution_graph(
            path,
            resolution_graph,
            run_bytecode_verifier,
            print_diags_to_stderr,
        )
    }

    pub fn resolution_graph(self, path: &Path) -> SuiResult<ResolvedGraph> {
        if self.print_diags_to_stderr {
            self.config
                .resolution_graph_for_package(path, &mut std::io::stderr())
        } else {
            self.config
                .resolution_graph_for_package(path, &mut std::io::sink())
        }
        .map_err(|err| SuiError::ModuleBuildFailure {
            error: format!("{:?}", err),
        })
    }
}

pub fn build_from_resolution_graph(
    path: PathBuf,
    resolution_graph: ResolvedGraph,
    run_bytecode_verifier: bool,
    print_diags_to_stderr: bool,
) -> SuiResult<CompiledPackage> {
    let (published_at, dependency_ids) = gather_published_ids(&resolution_graph);

    let result = if print_diags_to_stderr {
        BuildConfig::compile_package(resolution_graph, &mut std::io::stderr())
    } else {
        BuildConfig::compile_package(resolution_graph, &mut std::io::sink())
    };
    // write build failure diagnostics to stderr, convert `error` to `String` using `Debug`
    // format to include anyhow's error context chain.
    let (package, fn_info) = match result {
        Err(error) => {
            return Err(SuiError::ModuleBuildFailure {
                error: format!("{:?}", error),
            })
        }
        Ok((package, fn_info)) => (package, fn_info),
    };
    let compiled_modules = package.root_modules_map();
    if run_bytecode_verifier {
        for m in compiled_modules.iter_modules() {
            move_bytecode_verifier::verify_module_unmetered(m).map_err(|err| {
                SuiError::ModuleVerificationFailure {
                    error: err.to_string(),
                }
            })?;
            // TODO make this configurable
            sui_bytecode_verifier::sui_verify_module_unmetered(
                &ProtocolConfig::get_for_version(ProtocolVersion::MAX),
                m,
                &fn_info,
            )?;
        }
        // TODO(https://github.com/MystenLabs/sui/issues/69): Run Move linker
    }
    Ok(CompiledPackage {
        package,
        published_at,
        dependency_ids,
        path,
    })
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

    /// Return the bytecode modules in this package, topologically sorted in dependency order.
    /// Optionally include dependencies that have not been published (are at address 0x0), if
    /// `with_unpublished_deps` is true. This is the function to call if you would like to publish
    /// or statically analyze the modules.
    pub fn get_dependency_sorted_modules(
        &self,
        with_unpublished_deps: bool,
    ) -> Vec<CompiledModule> {
        let all_modules = self.package.all_modules_map();
        let graph = all_modules.compute_dependency_graph();

        // SAFETY: package built successfully
        let modules = graph.compute_topological_order().unwrap();

        if with_unpublished_deps {
            // For each transitive dependent module, if they are not to be published, they must have
            // a non-zero address (meaning they are already published on-chain).
            modules
                .filter(|module| module.address() == &AccountAddress::ZERO)
                .cloned()
                .collect()
        } else {
            // Collect all module IDs from the current package to be published (module names are not
            // sufficient as we may have modules with the same names in user code and in Sui
            // framework which would result in the latter being pulled into a set of modules to be
            // published).
            let self_modules: HashSet<_> = self
                .package
                .root_modules_map()
                .iter_modules()
                .iter()
                .map(|m| m.self_id())
                .collect();

            modules
                .filter(|module| self_modules.contains(&module.self_id()))
                .cloned()
                .collect()
        }
    }

    /// Return the set of Object IDs corresponding to this package's transitive dependencies'
    /// original package IDs.
    pub fn get_dependency_original_package_ids(&self) -> Vec<ObjectID> {
        let mut ids: BTreeSet<_> = self
            .package
            .deps_compiled_units
            .iter()
            .map(|(_, m)| match &m.unit {
                CompiledUnitEnum::Module(m) => ObjectID::from(*m.module.address()),
                CompiledUnitEnum::Script(_) => unimplemented!("Scripts not supported in Sui Move"),
            })
            .collect();

        // `0x0` is not a real dependency ID -- it means that the package has unpublished
        // dependencies.
        ids.remove(&ObjectID::ZERO);
        ids.into_iter().collect()
    }

    pub fn get_package_digest(&self, with_unpublished_deps: bool, hash_modules: bool) -> [u8; 32] {
        MovePackage::compute_digest_for_modules_and_deps(
            &self.get_package_bytes(with_unpublished_deps),
            self.dependency_ids.published.values(),
            hash_modules,
        )
    }

    /// Return a serialized representation of the bytecode modules in this package, topologically sorted in dependency order
    pub fn get_package_bytes(&self, with_unpublished_deps: bool) -> Vec<Vec<u8>> {
        self.get_dependency_sorted_modules(with_unpublished_deps)
            .iter()
            .map(|m| {
                let mut bytes = Vec::new();
                m.serialize(&mut bytes).unwrap(); // safe because package built successfully
                bytes
            })
            .collect()
    }

    /// Return the base64-encoded representation of the bytecode modules in this package, topologically sorted in dependency order
    pub fn get_package_base64(&self, with_unpublished_deps: bool) -> Vec<Base64> {
        self.get_package_bytes(with_unpublished_deps)
            .iter()
            .map(|b| Base64::from_bytes(b))
            .collect()
    }

    pub fn get_package_dependencies_hex(&self) -> Vec<String> {
        self.dependency_ids
            .published
            .values()
            .map(|object_id| object_id.to_hex_uncompressed())
            .collect()
    }

    /// Get bytecode modules from DeepBook that are used by this package
    pub fn get_deepbook_modules(&self) -> impl Iterator<Item = &CompiledModule> {
        self.get_modules_and_deps()
            .filter(|m| *m.self_id().address() == DEEPBOOK_ADDRESS)
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
            for (_name, f) in normalized_m.functions {
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

    /// Checks whether this package corresponds to a built-in framework
    pub fn is_system_package(&self) -> bool {
        // System packages always have "published-at" addresses
        let Ok(published_at) = self.published_at else {
            return false
        };

        is_system_package(published_at)
    }

    /// Checks for root modules with non-zero package addresses.  Returns an arbitrary one, if one
    /// can can be found, otherwise returns `None`.
    pub fn published_root_module(&self) -> Option<&CompiledModule> {
        self.package
            .root_compiled_units
            .iter()
            .find_map(|unit| match &unit.unit {
                CompiledUnitEnum::Module(NamedCompiledModule { module, .. })
                    if module.self_id().address() != &AccountAddress::ZERO =>
                {
                    Some(module)
                }
                _ => None,
            })
    }

    pub fn verify_unpublished_dependencies(
        &self,
        unpublished_deps: &BTreeSet<Symbol>,
    ) -> SuiResult<()> {
        if unpublished_deps.is_empty() {
            return Ok(());
        }

        let errors = self
            .package
            .deps_compiled_units
            .iter()
            .filter_map(|(p, m)| {
                if !unpublished_deps.contains(p) {
                    return None;
                }
                match &m.unit {
                    CompiledUnitEnum::Module(m) if m.module.address() != &AccountAddress::ZERO => {
                        Some(format!(
                            " - {}::{} in dependency {}",
                            m.module.address(),
                            m.name,
                            p
                        ))
                    }
                    CompiledUnitEnum::Module(_) => None,
                    CompiledUnitEnum::Script(_) => {
                        unimplemented!("Scripts are not supported in Sui Move")
                    }
                }
            })
            .collect::<Vec<String>>();

        if errors.is_empty() {
            return Ok(());
        }

        let mut error_message = vec![];
        error_message.push(
            "The following modules in package dependencies set a non-zero self-address:".into(),
        );
        error_message.extend(errors);
        error_message.push(
            "If these packages really are unpublished, their self-addresses should be set \
	     to \"0x0\" in the [addresses] section of the manifest when publishing. If they \
	     are already published, ensure they specify the address in the `published-at` of \
	     their Move.toml manifest."
                .into(),
        );

        Err(SuiError::ModulePublishFailure {
            error: error_message.join("\n"),
        })
    }

    pub fn published_dependency_ids(&self) -> Vec<ObjectID> {
        self.dependency_ids.published.values().cloned().collect()
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

pub const PUBLISHED_AT_MANIFEST_FIELD: &str = "published-at";

pub struct SuiPackageHooks;

impl PackageHooks for SuiPackageHooks {
    fn custom_package_info_fields(&self) -> Vec<String> {
        vec![PUBLISHED_AT_MANIFEST_FIELD.to_string()]
    }

    fn custom_dependency_key(&self) -> Option<String> {
        None
    }

    fn resolve_custom_dependency(
        &self,
        _dep_name: move_symbol_pool::Symbol,
        _info: &CustomDepInfo,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct PackageDependencies {
    /// Set of published dependencies (name and address).
    pub published: BTreeMap<Symbol, ObjectID>,
    /// Set of unpublished dependencies (name).
    pub unpublished: BTreeSet<Symbol>,
    /// Set of dependencies with invalid `published-at` addresses.
    pub invalid: BTreeMap<Symbol, String>,
}

#[derive(Debug)]
pub enum PublishedAtError {
    Invalid(String),
    NotPresent,
}

/// Partition packages in `resolution_graph` into one of four groups:
/// - The ID that the package itself is published at (if it is published)
/// - The IDs of dependencies that have been published
/// - The names of packages that have not been published on chain.
/// - The names of packages that have a `published-at` field that isn't filled with a valid address.
pub fn gather_published_ids(
    resolution_graph: &ResolvedGraph,
) -> (Result<ObjectID, PublishedAtError>, PackageDependencies) {
    let root = resolution_graph.root_package();

    let mut published = BTreeMap::new();
    let mut unpublished = BTreeSet::new();
    let mut invalid = BTreeMap::new();
    let mut published_at = Err(PublishedAtError::NotPresent);

    for (name, package) in &resolution_graph.package_table {
        let property = published_at_property(package);
        if name == &root {
            // Separate out the root package as a special case
            published_at = property;
            continue;
        }

        match property {
            Ok(id) => {
                published.insert(*name, id);
            }
            Err(PublishedAtError::NotPresent) => {
                unpublished.insert(*name);
            }
            Err(PublishedAtError::Invalid(value)) => {
                invalid.insert(*name, value);
            }
        };
    }

    (
        published_at,
        PackageDependencies {
            published,
            unpublished,
            invalid,
        },
    )
}

pub fn published_at_property(package: &Package) -> Result<ObjectID, PublishedAtError> {
    let Some(value) = package
        .source_package
        .package
        .custom_properties
        .get(&Symbol::from(PUBLISHED_AT_MANIFEST_FIELD))
    else {
        return Err(PublishedAtError::NotPresent);
    };

    ObjectID::from_str(value.as_str()).map_err(|_| PublishedAtError::Invalid(value.to_owned()))
}

pub fn check_unpublished_dependencies(unpublished: &BTreeSet<Symbol>) -> Result<(), SuiError> {
    if unpublished.is_empty() {
        return Ok(());
    };

    let mut error_messages = unpublished
        .iter()
        .map(|name| {
            format!(
                "Package dependency \"{name}\" does not specify a published address \
		 (the Move.toml manifest for \"{name}\" does not contain a published-at field).",
            )
        })
        .collect::<Vec<_>>();

    error_messages.push(
        "If this is intentional, you may use the --with-unpublished-dependencies flag to \
             continue publishing these dependencies as part of your package (they won't be \
             linked against existing packages on-chain)."
            .into(),
    );

    Err(SuiError::ModulePublishFailure {
        error: error_messages.join("\n"),
    })
}

pub fn check_invalid_dependencies(invalid: &BTreeMap<Symbol, String>) -> Result<(), SuiError> {
    if invalid.is_empty() {
        return Ok(());
    }

    let error_messages = invalid
        .iter()
        .map(|(name, value)| {
            format!(
                "Package dependency \"{name}\" does not specify a valid published \
		 address: could not parse value \"{value}\" for published-at field."
            )
        })
        .collect::<Vec<_>>();

    Err(SuiError::ModulePublishFailure {
        error: error_messages.join("\n"),
    })
}
