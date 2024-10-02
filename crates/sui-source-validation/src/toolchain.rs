// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    ffi::OsStr,
    fs::File,
    io::{self, Seek},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, bail, ensure};
use colored::Colorize;
use move_binary_format::CompiledModule;
use move_bytecode_source_map::utils::source_map_from_file;
use move_command_line_common::{
    env::MOVE_HOME,
    files::{
        extension_equals, find_filenames, MOVE_COMPILED_EXTENSION, MOVE_EXTENSION,
        SOURCE_MAP_EXTENSION,
    },
};
use move_compiler::{
    compiled_unit::NamedCompiledModule,
    editions::{Edition, Flavor},
    shared::{files::FileName, NumericalAddress},
};
use move_package::{
    compilation::{
        compiled_package::CompiledUnitWithSource, package_layout::CompiledPackageLayout,
    },
    lock_file::schema::{Header, ToolchainVersion},
    source_package::{layout::SourcePackageLayout, parsed_manifest::PackageName},
};
use move_symbol_pool::Symbol;
use tar::Archive;
use tempfile::TempDir;
use tracing::{debug, info};

pub(crate) const CURRENT_COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");
const LEGACY_COMPILER_VERSION: &str = CURRENT_COMPILER_VERSION; // TODO: update this when Move 2024 is released
const PRE_TOOLCHAIN_MOVE_LOCK_VERSION: u16 = 0; // Used to detect lockfiles pre-toolchain versioning support
const CANONICAL_UNIX_BINARY_NAME: &str = "sui";
const CANONICAL_WIN_BINARY_NAME: &str = "sui.exe";

pub(crate) fn current_toolchain() -> ToolchainVersion {
    ToolchainVersion {
        compiler_version: CURRENT_COMPILER_VERSION.into(),
        edition: Edition::LEGACY, /* does not matter, unused for current_toolchain */
        flavor: Flavor::Sui,      /* does not matter, unused for current_toolchain */
    }
}

pub(crate) fn legacy_toolchain() -> ToolchainVersion {
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
pub(crate) fn units_for_toolchain(
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
            dest_binary.push(format!("sui-{platform}.exe"));
        } else {
            dest_binary.push(format!("sui-{platform}"));
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
        address_name: None,
    };
    Ok(CompiledUnitWithSource {
        unit,
        source_path: source_path.clone(),
    })
}
