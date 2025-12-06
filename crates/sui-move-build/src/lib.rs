// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    io::Write,
    path::Path,
    str::FromStr,
};

use fastcrypto::encoding::Base64;
use serde_reflection::Registry;

use move_binary_format::{
    CompiledModule,
    normalized::{self, Type},
};
use move_bytecode_utils::{Modules, layout::SerdeLayoutBuilder, module_cache::GetModule};
use move_compiler::{
    compiled_unit::AnnotatedCompiledModule,
    diagnostics::{Diagnostics, report_diagnostics_to_buffer, report_warnings},
    linters::LINT_WARNING_PREFIX,
    shared::files::MappedFiles,
};
use move_core_types::{
    account_address::AccountAddress,
    language_storage::{ModuleId, StructTag},
};
use move_package_alt::{
    compatibility::{legacy_parser::LegacyPackageMetadata, parse_legacy_package_info},
    flavor::MoveFlavor,
    package::RootPackage,
    schema::Environment,
};
use move_package_alt_compilation::compiled_package::CompiledPackage as MoveCompiledPackage;
use move_package_alt_compilation::{
    build_config::BuildConfig as MoveBuildConfig, build_plan::BuildPlan,
};
use move_symbol_pool::Symbol;

use sui_package_alt::{SuiFlavor, testnet_environment};
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::{
    BRIDGE_ADDRESS, DEEPBOOK_ADDRESS, MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS,
    SUI_SYSTEM_ADDRESS, TypeTag,
    base_types::ObjectID,
    error::{SuiError, SuiErrorKind, SuiResult},
    is_system_package,
    move_package::{FnInfo, FnInfoKey, FnInfoMap, MovePackage},
};
use sui_verifier::verifier as sui_bytecode_verifier;

#[cfg(test)]
#[path = "unit_tests/build_tests.rs"]
mod build_tests;

pub mod test_utils {
    use crate::{BuildConfig, CompiledPackage};
    use std::path::PathBuf;

    pub async fn compile_basics_package() -> CompiledPackage {
        compile_example_package("../../examples/move/basics").await
    }

    pub async fn compile_managed_coin_package() -> CompiledPackage {
        compile_example_package("../../crates/sui-core/src/unit_tests/data/managed_coin").await
    }

    pub async fn compile_example_package(relative_path: &str) -> CompiledPackage {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push(relative_path);

        BuildConfig::new_for_testing()
            .build_async(&path)
            .await
            .unwrap()
    }
}

/// Wrapper around the core Move `CompiledPackage` with some Sui-specific traits and info
#[derive(Debug, Clone)]
pub struct CompiledPackage {
    pub package: MoveCompiledPackage,
    /// Address the package is recorded as being published at.
    pub published_at: Option<ObjectID>,
    /// The dependency IDs of this package
    pub dependency_ids: PackageDependencies,
}

/// Wrapper around the core Move `BuildConfig` with some Sui-specific info
#[derive(Clone)]
pub struct BuildConfig {
    pub config: MoveBuildConfig,
    /// If true, run the Move bytecode verifier on the bytecode from a successful build
    pub run_bytecode_verifier: bool,
    /// If true, print build diagnostics to stderr--no printing if false
    pub print_diags_to_stderr: bool,
    /// The environment that compilation is with respect to (e.g., required to resolve
    /// published dependency IDs).
    pub environment: Environment,
}

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
            environment: testnet_environment(),
        }
    }

    pub fn new_for_testing_replace_addresses<I, S>(dep_original_addresses: I) -> Self
    where
        I: IntoIterator<Item = (S, ObjectID)>,
        S: Into<String>,
    {
        let mut build_config = Self::new_for_testing();
        for (addr_name, obj_id) in dep_original_addresses {
            build_config
                .config
                .additional_named_addresses
                .insert(addr_name.into(), AccountAddress::from(obj_id));
        }
        build_config
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

    fn compile_package<W: Write + Send, F: MoveFlavor>(
        &self,
        root_pkg: &RootPackage<F>,
        writer: &mut W,
    ) -> anyhow::Result<(MoveCompiledPackage, FnInfoMap)> {
        let mut config = self.config.clone();
        // set the default flavor to Sui if not already set by the user
        if config.default_flavor.is_none() {
            config.default_flavor = Some(move_compiler::editions::Flavor::Sui);
        }
        let build_plan = BuildPlan::create(root_pkg, &config)?;
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

    pub async fn build_async(self, path: &Path) -> anyhow::Result<CompiledPackage> {
        let mut root_pkg = RootPackage::<SuiFlavor>::load(
            path.to_path_buf(),
            self.environment.clone(),
            self.config.mode_set(),
        )
        .await?;

        self.internal_build(&mut root_pkg)
    }

    pub async fn build_async_from_root_pkg(
        self,
        root_pkg: &mut RootPackage<SuiFlavor>,
    ) -> anyhow::Result<CompiledPackage> {
        self.internal_build(root_pkg)
    }

    /// Given a `path` and a `build_config`, build the package in that path, including its dependencies.
    /// If we are building the Sui framework, we skip the check that the addresses should be 0
    pub fn build(self, path: &Path) -> anyhow::Result<CompiledPackage> {
        // we need to block here to compile the package, which requires to fetch dependencies
        let mut root_pkg = RootPackage::<SuiFlavor>::load_sync(
            path.to_path_buf(),
            self.environment.clone(),
            self.config.mode_set(),
        )?;

        self.internal_build(&mut root_pkg)
    }

    fn internal_build(
        self,
        root_pkg: &mut RootPackage<SuiFlavor>,
    ) -> anyhow::Result<CompiledPackage> {
        let result = if self.print_diags_to_stderr {
            self.compile_package(root_pkg, &mut std::io::stderr())
        } else {
            self.compile_package(root_pkg, &mut std::io::sink())
        };

        let (package, fn_info) = result.map_err(|error| {
            SuiError::from(SuiErrorKind::ModuleBuildFailure {
                // Use [Debug] formatting to capture [anyhow] error context
                error: format!("{:?}", error),
            })
        })?;

        if self.run_bytecode_verifier {
            verify_bytecode(&package, &fn_info)?;
        }

        let dependency_ids = PackageDependencies::new(root_pkg)?;
        let published_at = root_pkg
            .publication()
            .map(|p| ObjectID::from_address(p.addresses.published_at.0));

        root_pkg.save_lockfile_to_disk()?;

        Ok(CompiledPackage {
            package,
            dependency_ids,
            published_at,
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
        eprintln!(
            "Total number of linter warnings suppressed: {filtered_diags_num} (unique lints: {unique})"
        );
    }
}

/// Check that the compiled modules in `package` are valid
fn verify_bytecode(package: &MoveCompiledPackage, fn_info: &FnInfoMap) -> SuiResult<()> {
    let compiled_modules = package.root_modules_map();
    let verifier_config = ProtocolConfig::get_for_version(ProtocolVersion::MAX, Chain::Unknown)
        .verifier_config(/* signing_limits */ None);

    for m in compiled_modules.iter_modules() {
        move_bytecode_verifier::verify_module_unmetered(m).map_err(|err| {
            SuiErrorKind::ModuleVerificationFailure {
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

    /// Return the bytecode modules in this package, topologically sorted in dependency order.
    /// Optionally include dependencies that have not been published (are at address 0x0), if
    /// `with_unpublished_deps` is true. This is the function to call if you would like to publish
    /// or statically analyze the modules.
    pub fn get_dependency_sorted_modules(
        &self,
        with_unpublished_deps: bool,
    ) -> Vec<CompiledModule> {
        let all_modules = Modules::new(self.get_modules_and_deps());

        // SAFETY: package built successfully
        let modules = all_modules.compute_topological_order().unwrap();

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
    /// storage package IDs (where to load those packages on-chain).
    pub fn get_dependency_storage_package_ids(&self) -> Vec<ObjectID> {
        self.dependency_ids.published.values().cloned().collect()
    }

    /// Return a digest of the bytecode modules in this package.
    pub fn get_package_digest(&self, with_unpublished_deps: bool) -> [u8; 32] {
        let hash_modules = true;
        MovePackage::compute_digest_for_modules_and_deps(
            &self.get_package_bytes(with_unpublished_deps),
            &self.get_dependency_storage_package_ids(),
            hash_modules,
        )
    }

    /// Return a serialized representation of the bytecode modules in this package, topologically sorted in dependency order
    pub fn get_package_bytes(&self, with_unpublished_deps: bool) -> Vec<Vec<u8>> {
        self.get_dependency_sorted_modules(with_unpublished_deps)
            .iter()
            .map(|m| {
                let mut bytes = Vec::new();
                m.serialize_with_version(m.version, &mut bytes).unwrap(); // safe because package built successfully
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

    /// Generate layout schemas for all types declared by this package, as well as
    /// all struct types passed into `entry` functions declared by modules in this package
    /// (either directly or by reference).
    /// These layout schemas can be consumed by clients (e.g., the TypeScript SDK) to enable
    /// BCS serialization/deserialization of the package's objects, tx arguments, and events.
    pub fn generate_struct_layouts(&self) -> Registry {
        let pool = &mut normalized::RcPool::new();
        let mut package_types = BTreeSet::new();
        for m in self.get_modules() {
            let normalized_m = normalized::Module::new(pool, m, /* include code */ false);
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
                    name: name.as_ident_str().to_owned(),
                    type_params: dummy_type_parameters,
                });
            }
            // 2. generate struct layouts for all parameters of `entry` funs
            for (_name, f) in normalized_m.functions {
                if f.is_entry {
                    for t in &*f.parameters {
                        let tag_opt = match &**t {
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
                            Type::Reference(_, inner) => inner.to_struct_tag(pool),
                            Type::Datatype(_) => t.to_struct_tag(pool),
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
            layout_builder.build_data_layout(typ).unwrap();
        }
        layout_builder.into_registry()
    }

    /// Checks whether this package corresponds to a built-in framework
    pub fn is_system_package(&self) -> bool {
        // System packages always have "published-at" addresses
        let Some(published_at) = self.published_at else {
            return false;
        };

        is_system_package(published_at)
    }

    /// Checks for root modules with non-zero package addresses.  Returns an arbitrary one, if one
    /// can be found, otherwise returns `None`.
    pub fn published_root_module(&self) -> Option<&CompiledModule> {
        self.package.root_compiled_units.iter().find_map(|unit| {
            if unit.unit.module.self_id().address() != &AccountAddress::ZERO {
                Some(&unit.unit.module)
            } else {
                None
            }
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
                if !unpublished_deps.contains(p) || m.unit.module.address() == &AccountAddress::ZERO
                {
                    return None;
                }
                Some(format!(
                    " - {}::{} in dependency {}",
                    m.unit.module.address(),
                    m.unit.name,
                    p
                ))
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
            "If these packages really are unpublished, their self-addresses should not be \
            explicitly set when publishing. If they are already published, ensure they specify the \
            address in the `published-at` of their Published.toml file."
                .into(),
        );

        Err(SuiErrorKind::ModulePublishFailure {
            error: error_message.join("\n"),
        }
        .into())
    }

    pub fn get_published_dependencies_ids(&self) -> Vec<ObjectID> {
        self.dependency_ids.published.values().cloned().collect()
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

#[derive(thiserror::Error, Debug, Clone)]
pub enum PublishedAtError {
    #[error("The 'published-at' field in Move.toml or Move.lock is invalid: {0:?}")]
    Invalid(String),
    #[error("The 'published-at' field is not present in Move.toml or Move.lock")]
    NotPresent,
}

#[derive(Debug, Clone)]
pub struct PackageDependencies {
    /// Set of published dependencies (name and address).
    pub published: BTreeMap<Symbol, ObjectID>,
    /// Set of unpublished dependencies (name and address).
    pub unpublished: BTreeSet<Symbol>,
    /// Set of dependencies with invalid `published-at` addresses.
    pub invalid: BTreeMap<Symbol, String>,
    /// Set of dependencies that have conflicting `published-at` addresses. The key refers to
    /// the package, and the tuple refers to the address in the (Move.lock, Move.toml) respectively.
    pub conflicting: BTreeMap<Symbol, (ObjectID, ObjectID)>,
}

impl PackageDependencies {
    pub fn new<F: MoveFlavor>(root_pkg: &RootPackage<F>) -> anyhow::Result<Self> {
        let mut published = BTreeMap::new();
        let mut unpublished = BTreeSet::new();

        let packages = root_pkg.packages();

        for p in packages {
            if p.is_root() {
                continue;
            }
            if let Some(addresses) = p.published() {
                published.insert(
                    p.display_name().into(),
                    ObjectID::from_address(addresses.published_at.0),
                );
            } else {
                unpublished.insert(p.display_name().into());
            }
        }

        Ok(Self {
            published,
            unpublished,
            invalid: BTreeMap::new(),
            conflicting: BTreeMap::new(),
        })
    }
}

pub fn parse_legacy_pkg_info(package_path: &Path) -> Result<LegacyPackageMetadata, anyhow::Error> {
    parse_legacy_package_info(package_path)
}

pub fn published_at_property(package_path: &Path) -> Result<ObjectID, PublishedAtError> {
    let parsed_manifest =
        parse_legacy_package_info(package_path).expect("should read the manifest");

    let Some(value) = parsed_manifest.published_at else {
        return Err(PublishedAtError::NotPresent);
    };

    ObjectID::from_str(value.as_str())
        .map_err(|_| PublishedAtError::Invalid(value.as_str().to_owned()))
}
