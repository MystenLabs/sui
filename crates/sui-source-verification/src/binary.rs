// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use colored::Colorize;
use move_command_line_common::env::MOVE_HOME;
use tar::Archive;
use tracing::debug;

use crate::error::Error;

const CURRENT_COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Resolve the path to a `sui` binary for `version`, downloading and caching it under the user's
/// cache directory (one subdirectory per version) if necessary.
///
/// If `version` is the version of the running binary, the running executable is used directly
/// (avoiding a redundant download of the version already in hand). This is also the precache /
/// warm entry point: calling it ahead of time populates the cache so later runs need no network.
pub fn ensure_binary(version: &str) -> Result<PathBuf, Error> {
    if version == CURRENT_COMPILER_VERSION {
        return std::env::current_exe().map_err(|e| Error::BinaryDownload {
            version: version.to_string(),
            message: format!("could not locate the running executable: {e}"),
        });
    }

    let platform = detect_platform(version)?;
    let binary_name = platform.binary_name();

    let cache_root = binary_cache_dir();
    let version_dir = cache_root.join(version);
    let canonical = version_dir.join("target").join("release").join(binary_name);

    if canonical.exists() {
        return Ok(canonical);
    }

    download_and_install(
        version,
        platform.artifact_str(),
        binary_name,
        &cache_root,
        &version_dir,
    )
    .map_err(|e| Error::BinaryDownload {
        version: version.to_string(),
        message: e.to_string(),
    })?;

    Ok(canonical)
}

/// Download the `sui` release tarball for `version`, extract the `sui` binary from it, and install it
/// atomically into `version_dir`. The download and extraction happen in a temporary directory that is
/// renamed into place only once complete, so concurrent installs of the same version cannot observe a
/// partial tree.
fn download_and_install(
    version: &str,
    platform: &str,
    binary_name: &str,
    cache_root: &Path,
    version_dir: &Path,
) -> anyhow::Result<()> {
    fs::create_dir_all(cache_root).context("creating binary cache directory")?;

    // Temp dir on the same filesystem as the cache so the final rename is atomic.
    let tmp = cache_root.join(format!(".tmp-{version}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).context("creating temporary install directory")?;

    let result = (|| {
        // Unpack into a scratch subdirectory: a release archive carries every shipped binary
        // (`sui-debug`, `sui-node`, ...), which is well over a gigabyte, and only `sui` is wanted.
        let extract = tmp.join("extract");
        fs::create_dir_all(&extract).context("creating extraction directory")?;
        download_and_extract(version, platform, &extract)?;

        // Locate the `sui` binary in the extracted archive (its layout varies by release) and
        // install it under the canonical cache path.
        let found = find_binary(&extract, platform)
            .ok_or_else(|| anyhow!("no sui binary found in the {version} release archive"))?;

        let release_dir = tmp.join("target").join("release");
        fs::create_dir_all(&release_dir).context("creating release directory")?;
        let canonical = release_dir.join(binary_name);
        fs::rename(&found, &canonical).context("installing extracted binary")?;
        set_executable_permission(canonical.as_os_str())?;

        // Drop the archive and the binaries we do not use.
        fs::remove_dir_all(&extract).context("cleaning up extracted archive")?;
        Ok(())
    })();

    if let Err(e) = result {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }

    match fs::rename(&tmp, version_dir) {
        Ok(()) => Ok(()),
        // Another process installed the same version first; use theirs and drop ours.
        Err(_)
            if version_dir
                .join("target")
                .join("release")
                .join(binary_name)
                .exists() =>
        {
            let _ = fs::remove_dir_all(&tmp);
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_dir_all(&tmp);
            Err(anyhow!("installing downloaded binary: {e}"))
        }
    }
}

/// Download the release tarball for `version` (trying the mainnet release, then the testnet
/// release) and extract it into `dest`.
fn download_and_extract(version: &str, platform: &str, dest: &Path) -> anyhow::Result<()> {
    let mainnet_url = format!(
        "https://github.com/MystenLabs/sui/releases/download/mainnet-v{version}/sui-mainnet-v{version}-{platform}.tgz",
    );

    println!(
        "{} sui compiler @ {} (this may take a while)",
        "DOWNLOADING".bold().green(),
        version.yellow()
    );

    let reader = match ureq::get(&mainnet_url).call() {
        Ok(response) => response,
        Err(ureq::Error::Status(404, _)) => {
            debug!("no mainnet release for {version}, trying testnet");
            let testnet_url = format!(
                "https://github.com/MystenLabs/sui/releases/download/testnet-v{version}/sui-testnet-v{version}-{platform}.tgz",
            );
            ureq::get(&testnet_url).call()?
        }
        Err(e) => return Err(e.into()),
    }
    .into_reader();

    let tarball = dest.join("sui.tgz");
    let mut file = File::create(&tarball).context("creating tarball file")?;
    io::copy(&mut { reader }, &mut file).context("downloading tarball")?;

    let tar_gz = File::open(&tarball).context("reopening tarball")?;
    let tar = flate2::read::GzDecoder::new(tar_gz);
    Archive::new(tar)
        .unpack(dest)
        .map_err(|e| anyhow!("failed to untar compiler binary: {e}"))?;
    Ok(())
}

/// Locate the `sui` executable within an extracted release archive, accepting either the modern
/// root-level `sui` or the older `target/release/sui-<platform>`. Other shipped binaries
/// (`sui-node`, `sui-tool`, ...) are not matched.
fn find_binary(root: &Path, platform: &str) -> Option<PathBuf> {
    let suffix = if platform == "windows-x86_64" {
        ".exe"
    } else {
        ""
    };
    let names = [format!("sui{suffix}"), format!("sui-{platform}{suffix}")];

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| names.iter().any(|candidate| candidate == n))
            {
                return Some(path);
            }
        }
    }
    None
}

/// A platform for which `sui` release binaries are published.
enum Platform {
    MacosArm64,
    MacosX86_64,
    UbuntuX86_64,
    WindowsX86_64,
}

impl Platform {
    /// The platform string used in release download URLs.
    fn artifact_str(&self) -> &'static str {
        match self {
            Platform::MacosArm64 => "macos-arm64",
            Platform::MacosX86_64 => "macos-x86_64",
            Platform::UbuntuX86_64 => "ubuntu-x86_64",
            Platform::WindowsX86_64 => "windows-x86_64",
        }
    }

    /// The name of the `sui` executable on this platform.
    fn binary_name(&self) -> &'static str {
        match self {
            Platform::WindowsX86_64 => "sui.exe",
            _ => "sui",
        }
    }
}

/// The [`Platform`] for the current OS/architecture, or an error explaining how to sideload a binary
/// if there is no downloadable release for this platform.
fn detect_platform(version: &str) -> Result<Platform, Error> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok(Platform::MacosArm64),
        ("macos", "x86_64") => Ok(Platform::MacosX86_64),
        ("linux", "x86_64") => Ok(Platform::UbuntuX86_64),
        ("windows", "x86_64") => Ok(Platform::WindowsX86_64),
        (os, arch) => Err(Error::BinaryDownload {
            version: version.to_string(),
            message: format!(
                "no downloadable sui {version} release for your platform \
                 (OS: {os}, architecture: {arch}); place a matching binary in {}",
                binary_cache_dir()
                    .join(version)
                    .join("target")
                    .join("release")
                    .display(),
            ),
        }),
    }
}

/// Directory under which downloaded `sui` binaries are cached, one subdirectory per version. Uses the
/// platform cache directory (as other Sui tooling does), falling back to `$MOVE_HOME` if it cannot be
/// determined.
fn binary_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(&*MOVE_HOME))
        .join("sui")
        .join("source-verification")
        .join("binaries")
}

#[cfg(unix)]
fn set_executable_permission(path: &OsStr) -> anyhow::Result<()> {
    use std::os::unix::prelude::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable_permission(_path: &OsStr) -> anyhow::Result<()> {
    // On Windows an executable is runnable by virtue of its extension, and the freshly-written file
    // is already owned by the current user, so there is no permission bit to set.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `find_binary` locates the `sui` binary in both the modern (root-level) and older
    /// (`target/release/sui-<platform>`) archive layouts, and ignores other shipped binaries.
    #[test]
    fn find_binary_handles_both_layouts() {
        let platform = "macos-arm64";

        // Modern layout: `sui` at the archive root, alongside binaries we do not want.
        let modern = tempfile::tempdir().unwrap();
        fs::write(modern.path().join("sui-node"), b"").unwrap();
        fs::write(modern.path().join("sui"), b"").unwrap();
        let found = find_binary(modern.path(), platform).expect("modern layout");
        assert_eq!(found.file_name().unwrap(), "sui");

        // Older layout: nested `target/release/sui-<platform>`.
        let old = tempfile::tempdir().unwrap();
        let nested = old.path().join("target").join("release");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("sui-macos-arm64"), b"").unwrap();
        let found = find_binary(old.path(), platform).expect("older layout");
        assert_eq!(found.file_name().unwrap(), "sui-macos-arm64");

        // Only other artifacts present: no match.
        let none = tempfile::tempdir().unwrap();
        fs::write(none.path().join("sui-tool"), b"").unwrap();
        assert!(find_binary(none.path(), platform).is_none());
    }
}
