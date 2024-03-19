// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, bail, ensure};
use colored::Colorize;
use core::fmt;
use futures::future;
use move_binary_format::access::ModuleAccess;
use move_binary_format::CompiledModule;
use move_bytecode_source_map::utils::source_map_from_file;
use move_compiler::editions::{Edition, Flavor};
use move_compiler::shared::NumericalAddress;
use move_package::compilation::package_layout::CompiledPackageLayout;
use move_package::lock_file::schema::{Header, ToolchainVersion};
use move_package::source_package::layout::SourcePackageLayout;
use move_package::source_package::parsed_manifest::{FileName, PackageName};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Seek};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{collections::HashMap, fmt::Debug};
use sui_move_build::CompiledPackage;
use sui_types::error::SuiObjectResponseError;
use tar::Archive;
use tempfile::TempDir;
use thiserror::Error;
use tracing::{debug, info};

use move_command_line_common::env::MOVE_HOME;
use move_command_line_common::files::MOVE_COMPILED_EXTENSION;
use move_command_line_common::files::{
    extension_equals, find_filenames, MOVE_EXTENSION, SOURCE_MAP_EXTENSION,
};
use move_compiler::compiled_unit::NamedCompiledModule;
use move_core_types::account_address::AccountAddress;
use move_package::compilation::compiled_package::{
    CompiledPackage as MoveCompiledPackage, CompiledUnitWithSource,
};
use move_symbol_pool::Symbol;
use sui_sdk::apis::ReadApi;
use sui_sdk::error::Error;

use sui_sdk::rpc_types::{SuiObjectDataOptions, SuiRawData, SuiRawMoveObject, SuiRawMovePackage};
use sui_types::base_types::ObjectID;

#[cfg(test)]
mod tests;

const CURRENT_COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");
const LEGACY_COMPILER_VERSION: &str = CURRENT_COMPILER_VERSION; // TODO: update this when Move 2024 is released
const PRE_TOOLCHAIN_MOVE_LOCK_VERSION: u64 = 0; // Used to detect lockfiles pre-toolchain versioning support
const CANONICAL_UNIX_BINARY_NAME: &str = "sui";
const CANONICAL_WIN_BINARY_NAME: &str = "sui.exe";

#[derive(Debug, Error)]
pub enum SourceVerificationError {
    #[error("Could not read a dependency's on-chain object: {0:?}")]
    DependencyObjectReadFailure(Error),

    #[error("Dependency object does not exist or was deleted: {0:?}")]
    SuiObjectRefFailure(SuiObjectResponseError),

    #[error("Dependency ID contains a Sui object, not a Move package: {0}")]
    ObjectFoundWhenPackageExpected(ObjectID, SuiRawMoveObject),

    #[error("On-chain version of dependency {package}::{module} was not found.")]
    OnChainDependencyNotFound { package: Symbol, module: Symbol },

    #[error("Could not deserialize on-chain dependency {address}::{module}.")]
    OnChainDependencyDeserializationError {
        address: AccountAddress,
        module: Symbol,
    },

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

    #[error("Cannot check local module for {package}: {message}")]
    CannotCheckLocalModules { package: Symbol, message: String },

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
    rpc_client: &'a ReadApi,
}

/// Map package addresses and module names to package names and bytecode.
type LocalModules = HashMap<(AccountAddress, Symbol), (Symbol, CompiledModule)>;
/// Map package addresses and modules names to bytecode (package names are gone in the on-chain
/// representation).
type OnChainModules = HashMap<(AccountAddress, Symbol), CompiledModule>;

impl<'a> BytecodeSourceVerifier<'a> {
    pub fn new(rpc_client: &'a ReadApi) -> Self {
        BytecodeSourceVerifier { rpc_client }
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
        let mut on_chain_pkgs = vec![];
        match &source_mode {
            SourceMode::Skip => (),
            // On-chain address for matching root package cannot be zero
            SourceMode::VerifyAt(AccountAddress::ZERO) => {
                return Err(SourceVerificationError::ZeroOnChainAddresSpecifiedFailure.into())
            }
            SourceMode::VerifyAt(root_address) => on_chain_pkgs.push(*root_address),
            SourceMode::Verify => {
                on_chain_pkgs.extend(compiled_package.published_at.as_ref().map(|id| **id))
            }
        };

        if verify_deps {
            on_chain_pkgs.extend(
                compiled_package
                    .dependency_ids
                    .published
                    .values()
                    .map(|id| **id),
            );
        }

        let local_modules = local_modules(&compiled_package.package, verify_deps, source_mode)?;
        let mut on_chain_modules = self.on_chain_modules(on_chain_pkgs.into_iter()).await?;

        let mut errors = Vec::new();
        for ((address, module), (package, local_module)) in local_modules {
            let Some(on_chain_module) = on_chain_modules.remove(&(address, module)) else {
                errors.push(SourceVerificationError::OnChainDependencyNotFound { package, module });
                continue;
            };

            // compare local bytecode to on-chain bytecode to ensure integrity of our
            // dependencies
            if local_module != on_chain_module {
                errors.push(SourceVerificationError::ModuleBytecodeMismatch {
                    address,
                    package,
                    module,
                });
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

    async fn on_chain_modules(
        &self,
        addresses: impl Iterator<Item = AccountAddress> + Clone,
    ) -> Result<OnChainModules, AggregateSourceVerificationError> {
        let resp = future::join_all(addresses.clone().map(|addr| self.pkg_for_address(addr))).await;
        let mut map = OnChainModules::new();
        let mut err = vec![];

        for (storage_id, pkg) in addresses.zip(resp) {
            let SuiRawMovePackage { module_map, .. } = pkg?;
            for (name, bytes) in module_map {
                let Ok(module) = CompiledModule::deserialize_with_defaults(&bytes) else {
                    err.push(
                        SourceVerificationError::OnChainDependencyDeserializationError {
                            address: storage_id,
                            module: name.into(),
                        },
                    );
                    continue;
                };

                let runtime_id = *module.self_id().address();
                map.insert((runtime_id, Symbol::from(name)), module);
            }
        }

        if !err.is_empty() {
            return Err(AggregateSourceVerificationError(err));
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

fn local_modules(
    compiled_package: &MoveCompiledPackage,
    include_deps: bool,
    source_mode: SourceMode,
) -> Result<LocalModules, SourceVerificationError> {
    let mut map = LocalModules::new();

    if include_deps {
        // Compile dependencies with prior compilers if needed.
        let deps_compiled_units = units_for_toolchain(&compiled_package.deps_compiled_units)
            .map_err(|e| SourceVerificationError::CannotCheckLocalModules {
                package: compiled_package.compiled_package_info.package_name,
                message: e.to_string(),
            })?;

        for (package, local_unit) in deps_compiled_units {
            let m = &local_unit.unit;
            let module = m.name;
            let address = m.address.into_inner();
            if address == AccountAddress::ZERO {
                continue;
            }

            map.insert((address, module), (package, m.module.clone()));
        }
    }

    let root_package = compiled_package.compiled_package_info.package_name;
    match source_mode {
        SourceMode::Skip => { /* nop */ }

        // Include the root compiled units, at their current addresses.
        SourceMode::Verify => {
            // Compile root modules with prior compiler if needed.
            let root_compiled_units = {
                let root_compiled_units = compiled_package
                    .root_compiled_units
                    .iter()
                    .map(|u| ("root".into(), u.clone()))
                    .collect::<Vec<_>>();

                units_for_toolchain(&root_compiled_units).map_err(|e| {
                    SourceVerificationError::CannotCheckLocalModules {
                        package: compiled_package.compiled_package_info.package_name,
                        message: e.to_string(),
                    }
                })?
            };

            for (_, local_unit) in root_compiled_units {
                let m = &local_unit.unit;

                let module = m.name;
                let address = m.address.into_inner();
                if address == AccountAddress::ZERO {
                    return Err(SourceVerificationError::InvalidModuleFailure {
                        name: module.to_string(),
                        message: "Can't verify unpublished source".to_string(),
                    });
                }

                map.insert((address, module), (root_package, m.module.clone()));
            }
        }

        // Include the root compiled units, and any unpublished dependencies with their
        // addresses substituted
        SourceMode::VerifyAt(root_address) => {
            // Compile root modules with prior compiler if needed.
            let root_compiled_units = {
                let root_compiled_units = compiled_package
                    .root_compiled_units
                    .iter()
                    .map(|u| ("root".into(), u.clone()))
                    .collect::<Vec<_>>();

                units_for_toolchain(&root_compiled_units).map_err(|e| {
                    SourceVerificationError::CannotCheckLocalModules {
                        package: compiled_package.compiled_package_info.package_name,
                        message: e.to_string(),
                    }
                })?
            };

            for (_, local_unit) in root_compiled_units {
                let m = &local_unit.unit;

                let module = m.name;
                map.insert(
                    (root_address, module),
                    (root_package, substitute_root_address(m, root_address)?),
                );
            }

            for (package, local_unit) in &compiled_package.deps_compiled_units {
                let m = &local_unit.unit;
                let module = m.name;
                let address = m.address.into_inner();
                if address != AccountAddress::ZERO {
                    continue;
                }

                map.insert(
                    (root_address, module),
                    (*package, substitute_root_address(m, root_address)?),
                );
            }
        }
    }

    Ok(map)
}

fn current_toolchain() -> ToolchainVersion {
    ToolchainVersion {
        compiler_version: CURRENT_COMPILER_VERSION.into(),
        edition: Edition::LEGACY, /* does not matter, unused for current_toolchain */
        flavor: Flavor::Sui,      /* does not matter, unused for current_toolchain */
    }
}

fn legacy_toolchain() -> ToolchainVersion {
    ToolchainVersion {
        compiler_version: LEGACY_COMPILER_VERSION.into(),
        edition: Edition::LEGACY,
        flavor: Flavor::Sui,
    }
}

/// Ensures `compiled_units` are compiled with the right compiler version, based on
/// Move.lock contents. This works by detecting if a compiled unit requires a prior compiler version:
/// - If so, download the compiler, recompile the unit, and return that unit in the result.
/// - If not, simply keep the current compiled unit.
fn units_for_toolchain(
    compiled_units: &Vec<(PackageName, CompiledUnitWithSource)>,
) -> anyhow::Result<Vec<(PackageName, CompiledUnitWithSource)>> {
    if std::env::var("SUI_RUN_TOOLCHAIN_BUILD").is_err() {
        return Ok(compiled_units.clone());
    }
    let mut package_version_map: HashMap<Symbol, (ToolchainVersion, Vec<CompiledUnitWithSource>)> =
        HashMap::new();
    // First iterate over packages, mapping the required version for each package in `package_version_map`.
    for (package, local_unit) in compiled_units {
        if let Some((_, units)) = package_version_map.get_mut(package) {
            // We've processed this package's required version.
            units.push(local_unit.clone());
            continue;
        }

        if sui_types::is_system_package(local_unit.unit.address.into_inner()) {
            // System packages are always compiled with the current compiler.
            package_version_map.insert(*package, (current_toolchain(), vec![local_unit.clone()]));
            continue;
        }

        let package_root = SourcePackageLayout::try_find_root(&local_unit.source_path)?;
        let lock_file = package_root.join(SourcePackageLayout::Lock.path());
        if !lock_file.exists() {
            // No lock file implies current compiler for this package.
            package_version_map.insert(*package, (current_toolchain(), vec![local_unit.clone()]));
            continue;
        }

        let mut lock_file = File::open(lock_file)?;
        let lock_version = Header::read(&mut lock_file)?.version;
        if lock_version == PRE_TOOLCHAIN_MOVE_LOCK_VERSION {
            // No need to attempt reading lock file toolchain
            debug!("{package} on legacy compiler",);
            package_version_map.insert(*package, (legacy_toolchain(), vec![local_unit.clone()]));
            continue;
        }

        // Read lock file toolchain info
        lock_file.rewind()?;
        let toolchain_version = ToolchainVersion::read(&mut lock_file)?;
        match toolchain_version {
            // No ToolchainVersion and new Move.lock version implies current compiler.
            None => {
                debug!("{package} on current compiler @ {CURRENT_COMPILER_VERSION}",);
                package_version_map
                    .insert(*package, (current_toolchain(), vec![local_unit.clone()]));
            }
            // This dependency uses the current compiler.
            Some(ToolchainVersion {
                compiler_version, ..
            }) if compiler_version == CURRENT_COMPILER_VERSION => {
                debug!("{package} on current compiler @ {CURRENT_COMPILER_VERSION}",);
                package_version_map
                    .insert(*package, (current_toolchain(), vec![local_unit.clone()]));
            }
            // This dependency needs a prior compiler. Mark it and compile.
            Some(toolchain_version) => {
                println!(
                    "{} {package} compiler @ {}",
                    "REQUIRE".bold().green(),
                    toolchain_version.compiler_version.yellow(),
                );
                package_version_map.insert(*package, (toolchain_version, vec![local_unit.clone()]));
            }
        }
    }

    let mut units = vec![];
    // Iterate over compiled units, and check if they need to be recompiled and replaced by a prior compiler's output.
    for (package, (toolchain_version, local_units)) in package_version_map {
        if toolchain_version.compiler_version == CURRENT_COMPILER_VERSION {
            let local_units: Vec<_> = local_units.iter().map(|u| (package, u.clone())).collect();
            units.extend(local_units);
            continue;
        }

        if local_units.is_empty() {
            bail!("Expected one or more modules, but none found");
        }
        let package_root = SourcePackageLayout::try_find_root(&local_units[0].source_path)?;
        let install_dir = tempfile::tempdir()?; // place compiled packages in this temp dir, don't pollute this packages build dir
        download_and_compile(
            package_root.clone(),
            &install_dir,
            &toolchain_version,
            &package,
        )?;

        let compiled_unit_paths = vec![package_root.clone()];
        let compiled_units = find_filenames(&compiled_unit_paths, |path| {
            extension_equals(path, MOVE_COMPILED_EXTENSION)
        })?;
        let build_path = install_dir
            .path()
            .join(CompiledPackageLayout::path(&CompiledPackageLayout::Root))
            .join(package.as_str());
        debug!("build path is {}", build_path.display());

        // Add all units compiled with the previous compiler.
        for bytecode_path in compiled_units {
            info!("bytecode path {bytecode_path}, {package}");
            let local_unit = decode_bytecode_file(build_path.clone(), &package, &bytecode_path)?;
            units.push((package, local_unit))
        }
    }
    Ok(units)
}

fn download_and_compile(
    root: PathBuf,
    install_dir: &TempDir,
    ToolchainVersion {
        compiler_version,
        edition,
        flavor,
    }: &ToolchainVersion,
    dep_name: &Symbol,
) -> anyhow::Result<()> {
    let dest_dir = PathBuf::from_iter([&*MOVE_HOME, "binaries"]); // E.g., ~/.move/binaries
    let dest_version = dest_dir.join(compiler_version);
    let mut dest_canonical_path = dest_version.clone();
    dest_canonical_path.extend(["target", "release"]);
    let mut dest_canonical_binary = dest_canonical_path.clone();

    let platform = detect_platform(&root, compiler_version, &dest_canonical_path)?;
    if platform == "windows-x86_64" {
        dest_canonical_binary.push(CANONICAL_WIN_BINARY_NAME);
    } else {
        dest_canonical_binary.push(CANONICAL_UNIX_BINARY_NAME);
    }

    if !dest_canonical_binary.exists() {
        // Check the platform and proceed if we can download a binary. If not, the user should follow error instructions to sideload the binary.
        // Download if binary does not exist.
        let mainnet_url = format!(
            "https://github.com/MystenLabs/sui/releases/download/mainnet-v{compiler_version}/sui-mainnet-v{compiler_version}-{platform}.tgz",
        );

        println!(
            "{} mainnet compiler @ {} (this may take a while)",
            "DOWNLOADING".bold().green(),
            compiler_version.yellow()
        );

        let mut response = match ureq::get(&mainnet_url).call() {
            Ok(response) => response,
            Err(ureq::Error::Status(404, _)) => {
                println!(
                    "{} sui mainnet compiler {} not available, attempting to download testnet compiler release...",
                    "WARNING".bold().yellow(),
                    compiler_version.yellow()
                );
                println!(
                    "{} testnet compiler @ {} (this may take a while)",
                    "DOWNLOADING".bold().green(),
                    compiler_version.yellow()
                );
                let testnet_url = format!("https://github.com/MystenLabs/sui/releases/download/testnet-v{compiler_version}/sui-testnet-v{compiler_version}-{platform}.tgz");
                ureq::get(&testnet_url).call()?
            }
            Err(e) => return Err(e.into()),
        }.into_reader();

        let dest_tarball = dest_version.join(format!("{}.tgz", compiler_version));
        debug!("tarball destination: {} ", dest_tarball.display());
        if let Some(parent) = dest_tarball.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow!("failed to create directory for tarball: {e}"))?;
        }
        let mut dest_file = File::create(&dest_tarball)?;
        io::copy(&mut response, &mut dest_file)?;

        // Extract the tarball using the tar crate
        let tar_gz = File::open(&dest_tarball)?;
        let tar = flate2::read::GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        archive
            .unpack(&dest_version)
            .map_err(|e| anyhow!("failed to untar compiler binary: {e}"))?;

        let mut dest_binary = dest_version.clone();
        dest_binary.extend(["target", "release"]);
        if platform == "windows-x86_64" {
            dest_binary.push(&format!("sui-{platform}.exe"));
        } else {
            dest_binary.push(&format!("sui-{platform}"));
        }
        let dest_binary_os = OsStr::new(dest_binary.as_path());
        set_executable_permission(dest_binary_os)?;
        std::fs::rename(dest_binary_os, dest_canonical_binary.clone())?;
    }

    debug!(
        "{} move build --default-move-edition {} --default-move-flavor {} -p {} --install-dir {}",
        dest_canonical_binary.display(),
        edition.to_string().as_str(),
        flavor.to_string().as_str(),
        root.display(),
        install_dir.path().display(),
    );
    info!(
        "{} {} (compiler @ {})",
        "BUILDING".bold().green(),
        dep_name.as_str(),
        compiler_version.yellow()
    );
    Command::new(dest_canonical_binary)
        .args([
            OsStr::new("move"),
            OsStr::new("build"),
            OsStr::new("--default-move-edition"),
            OsStr::new(edition.to_string().as_str()),
            OsStr::new("--default-move-flavor"),
            OsStr::new(flavor.to_string().as_str()),
            OsStr::new("-p"),
            OsStr::new(root.as_path()),
            OsStr::new("--install-dir"),
            OsStr::new(install_dir.path()),
        ])
        .output()
        .map_err(|e| {
            anyhow!("failed to build package from compiler binary {compiler_version}: {e}",)
        })?;
    Ok(())
}

fn detect_platform(
    package_path: &Path,
    compiler_version: &String,
    dest_dir: &Path,
) -> anyhow::Result<String> {
    let s = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "macos-arm64",
        ("macos", "x86_64") => "macos-x86_64",
        ("linux", "x86_64") => "ubuntu-x86_64",
        ("windows", "x86_64") => "windows-x86_64",
        (os, arch) => {
            let mut binary_name = CANONICAL_UNIX_BINARY_NAME;
            if os == "windows" {
                binary_name = CANONICAL_WIN_BINARY_NAME;
            };
            bail!(
                "The package {} needs to be built with sui compiler version {compiler_version} but there \
                 is no binary release available to download for your platform:\n\
                 Operating System: {os}\n\
                 Architecture: {arch}\n\
                 You can manually put a {binary_name} binary for your platform in {} and rerun your command to continue.",
                package_path.display(),
                dest_dir.display(),
            )
        }
    };
    Ok(s.into())
}

#[cfg(unix)]
fn set_executable_permission(path: &OsStr) -> anyhow::Result<()> {
    use std::fs;
    use std::os::unix::prelude::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable_permission(path: &OsStr) -> anyhow::Result<()> {
    Command::new("icacls")
        .args([path, OsStr::new("/grant"), OsStr::new("Everyone:(RX)")])
        .status()?;
    Ok(())
}

fn decode_bytecode_file(
    root_path: PathBuf,
    package_name: &Symbol,
    bytecode_path_str: &str,
) -> anyhow::Result<CompiledUnitWithSource> {
    let package_name_opt = Some(*package_name);
    let bytecode_path = Path::new(bytecode_path_str);
    let path_to_file = CompiledPackageLayout::path_to_file_after_category(bytecode_path);
    let bytecode_bytes = std::fs::read(bytecode_path)?;
    let source_map = source_map_from_file(
        &root_path
            .join(CompiledPackageLayout::SourceMaps.path())
            .join(&path_to_file)
            .with_extension(SOURCE_MAP_EXTENSION),
    )?;
    let source_path = &root_path
        .join(CompiledPackageLayout::Sources.path())
        .join(path_to_file)
        .with_extension(MOVE_EXTENSION);
    ensure!(
        source_path.is_file(),
        "Error decoding package: Unable to find corresponding source file for '{bytecode_path_str}' in package {package_name}"
    );
    let module = CompiledModule::deserialize_with_defaults(&bytecode_bytes)?;
    let (address_bytes, module_name) = {
        let id = module.self_id();
        let parsed_addr = NumericalAddress::new(
            id.address().into_bytes(),
            move_compiler::shared::NumberFormat::Hex,
        );
        let module_name = FileName::from(id.name().as_str());
        (parsed_addr, module_name)
    };
    let unit = NamedCompiledModule {
        package_name: package_name_opt,
        address: address_bytes,
        name: module_name,
        module,
        source_map,
    };
    Ok(CompiledUnitWithSource {
        unit,
        source_path: source_path.clone(),
    })
}
