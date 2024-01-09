use crate::{
    compilation::package_layout::CompiledPackageLayout,
    lock_file::{self, schema::ToolchainVersion},
    source_package::{layout::SourcePackageLayout, parsed_manifest::FileName},
};
use anyhow::{anyhow, bail, ensure, Result};
use colored::Colorize;
use move_binary_format::CompiledModule;
use move_bytecode_source_map::utils::source_map_from_file;
use move_command_line_common::{
    address::NumericalAddress,
    env::MOVE_HOME,
    files::{MOVE_EXTENSION, SOURCE_MAP_EXTENSION},
};
use move_compiler::{compiled_unit::NamedCompiledModule, shared::PackagePaths};
use move_symbol_pool::Symbol;

use std::process::Command;
use std::{
    ffi::OsStr,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};
use tracing::debug;

use super::compiled_package::CompiledUnitWithSource;

/// partitions `deps` by whether we need to compile dependent packages with a
/// prior toolchain (which we find by looking at Move.lock contents) or
/// whether we can compile them with the current binary.
pub fn partition_deps_by_toolchain<W: Write>(
    deps: Vec<PackagePaths>,
    current_compiler_version: Option<String>,
    w: &mut W,
) -> Result<(Vec<PackagePaths>, Vec<PackagePaths>)> {
    let current_compiler_version = current_compiler_version.unwrap_or_else(|| "0.0.0".into());
    debug!("current compiler: {current_compiler_version}");
    let mut deps_for_current_compiler = vec![];
    let mut deps_for_prior_compiler = vec![];
    for dep in deps {
        let a_source_path = dep.paths[0].as_str();
        let root = SourcePackageLayout::try_find_root(Path::new(a_source_path))?;
        let lock_file = root.join("Move.lock");
        if !lock_file.exists() {
            deps_for_current_compiler.push(dep);
            continue;
        }

        let mut lock_file = File::open(lock_file)?;
        let toolchain_version = lock_file::schema::ToolchainVersion::read(&mut lock_file)?;
        match toolchain_version {
            // No ToolchainVersion implies current compiler
            None => deps_for_current_compiler.push(dep),
            // This dependency uses the current compiler
            Some(ToolchainVersion {
                compiler_version, ..
            }) if compiler_version == current_compiler_version => {
                deps_for_current_compiler.push(dep)
            }
            // This dependency needs a prior compiler. Mark it and compile.
            Some(toolchain_version) => {
                let dep_name = match dep.name.clone() {
                    Some((name, _)) => name,
                    None => "unnamed dependency".into(),
                };
                writeln!(
                    w,
                    "{} {} compiler @ {}",
                    "REQUIRE".bold().green(),
                    dep_name,
                    toolchain_version.compiler_version.yellow(),
                )?;
                download_and_compile(root, toolchain_version, dep_name, w)?;
                deps_for_prior_compiler.push(dep)
            }
        }
    }
    Ok((deps_for_current_compiler, deps_for_prior_compiler))
}

fn detect_platform() -> Result<String> {
    let s = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "macos-arm64",
        ("macos", "x86_64") => "macos-x86_64",
        ("linux", "x86_64") => "ubuntu-x86_64",
        ("windows", "x86_64") => "windows-x86_64",
        (os, arch) => bail!("unsupported os {os} and arch {arch}"),
    };
    Ok(s.into())
}

fn download_and_compile<W: Write>(
    root: PathBuf,
    ToolchainVersion {
        compiler_version,
        edition,
        flavor,
    }: ToolchainVersion,
    dep_name: Symbol,
    w: &mut W,
) -> Result<()> {
    let binaries_path = &*MOVE_HOME; // E.g., ~/.move/binaries
    let mut dest_dir = PathBuf::from(binaries_path);
    dest_dir = dest_dir.join("binaries");
    let dest_version = dest_dir.join(compiler_version.clone());
    let platform = detect_platform()?;
    let dest_binary = dest_version.join(format!("target/release/sui-{}", platform));
    let dest_binary_os = OsStr::new(dest_binary.as_path());

    if !dest_binary.exists() {
        // Download if binary does not exist.
        let url = format!("https://github.com/MystenLabs/sui/releases/download/mainnet-v{}/sui-mainnet-v{}-{}.tgz", compiler_version, compiler_version, platform);
        let release_url = OsStr::new(url.as_str());
        let dest_tarball = dest_version.join(format!("{}.tgz", compiler_version));
        debug!(
            "curl -L --create-dirs -o {} {}",
            dest_tarball.display(),
            url
        );
        writeln!(
            w,
            "{} compiler @ {}",
            "DOWNLOADING".bold().green(),
            compiler_version.yellow()
        )?;
        Command::new("curl")
            .args([
                OsStr::new("-L"),
                OsStr::new("--create-dirs"),
                OsStr::new("-o"),
                OsStr::new(dest_tarball.as_path()),
                OsStr::new(release_url),
            ])
            .output()
            .map_err(|e| {
                anyhow!("failed to download compiler binary for {compiler_version}: {e}",)
            })?;

        debug!(
            "tar -xzf {} -C {}",
            dest_tarball.display(),
            dest_version.display()
        );
        Command::new("tar")
            .args([
                OsStr::new("-xzf"),
                OsStr::new(dest_tarball.as_path()),
                OsStr::new("-C"),
                OsStr::new(dest_version.as_path()),
            ])
            .output()
            .map_err(|e| anyhow!("failed to untar compiler binary: {e}"))?;

        set_executable_permission(dest_binary_os)?;
    }

    debug!(
        "sui move build --default-move-edition {} --default-move-flavor {} -p {}",
        edition.to_string().as_str(),
        flavor.to_string().as_str(),
        root.display()
    );
    writeln!(
        w,
        "{} {} (compiler @ {})",
        "BUILDING".bold().green(),
        dep_name.as_str(),
        compiler_version.yellow()
    )?;
    Command::new(dest_binary_os)
        .args([
            OsStr::new("move"),
            OsStr::new("build"),
            OsStr::new("--default-move-edition"),
            OsStr::new(edition.to_string().as_str()),
            OsStr::new("--default-move-flavor"),
            OsStr::new(flavor.to_string().as_str()),
            OsStr::new("-p"),
            OsStr::new(root.as_path()),
        ])
        .output()
        .map_err(|e| {
            anyhow!("failed to build package from compiler binary {compiler_version}: {e}",)
        })?;
    Ok(())
}

pub fn decode_bytecode_file(
    root_path: PathBuf,
    package_name: Symbol,
    bytecode_path_str: &str,
) -> Result<CompiledUnitWithSource> {
    let package_name_opt = Some(package_name);
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

#[cfg(unix)]
fn set_executable_permission(path: &OsStr) -> Result<()> {
    use std::fs;
    use std::os::unix::prelude::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable_permission(path: &OsStr) -> Result<()> {
    Command::new("icacls")
        .args([path, OsStr::new("/grant"), OsStr::new("Everyone:(RX)")])
        .status()?;
    Ok(())
}
