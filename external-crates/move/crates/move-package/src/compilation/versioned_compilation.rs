use crate::{
    lock_file::{self, schema::ToolchainVersion},
    source_package::layout::SourcePackageLayout,
};
use anyhow::{anyhow, bail, Result};
use colored::Colorize;
use move_command_line_common::env::MOVE_HOME;
use move_compiler::shared::PackagePaths;
use move_symbol_pool::Symbol;

use std::process::Command;
use std::{
    ffi::OsStr,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};
use tracing::debug;

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
