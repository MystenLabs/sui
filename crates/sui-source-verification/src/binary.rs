// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::fs;
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

/// Download the `sui` release tarball for `version`, streaming out just the `sui` binary, and install
/// it atomically into `version_dir`. The binary is written under a temporary directory that is renamed
/// into place only once complete, so concurrent installs of the same version cannot observe a partial
/// tree, and nothing but the `sui` binary is ever written to disk.
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
        // Stream the release archive into the temp tree, writing only the `sui` binary. A release
        // archive carries every shipped binary (`sui-debug`, `sui-node`, ...), well over a gigabyte,
        // so streaming keeps everything but `sui` off disk. The completed tree is renamed into place
        // below, so the install stays atomic.
        let release_dir = tmp.join("target").join("release");
        fs::create_dir_all(&release_dir).context("creating release directory")?;
        let staged_binary = release_dir.join(binary_name);
        stream_sui_binary(version, platform, &staged_binary)?;
        set_executable_permission(staged_binary.as_os_str())?;
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

/// Download the `sui` release tarball for `version` and stream it, extracting only the `sui` binary
/// to `dest`.
fn stream_sui_binary(version: &str, platform: &str, dest: &Path) -> anyhow::Result<()> {
    let reader = download_reader(version, platform)?;
    extract_sui_from_stream(reader, version, platform, dest)
}

/// Open a streaming reader over the `sui` release tarball for `version`, trying the mainnet release
/// first and falling back to the testnet release on a 404.
fn download_reader(version: &str, platform: &str) -> anyhow::Result<impl io::Read> {
    let mainnet_url = format!(
        "https://github.com/MystenLabs/sui/releases/download/mainnet-v{version}/sui-mainnet-v{version}-{platform}.tgz",
    );

    // Progress goes to stderr so it does not corrupt a `--json` verification result on stdout.
    eprintln!(
        "{} sui compiler @ {} (this may take a while)",
        "DOWNLOADING".bold().green(),
        version.yellow()
    );

    let response = match ureq::get(&mainnet_url).call() {
        Ok(response) => response,
        Err(ureq::Error::Status(404, _)) => {
            debug!("no mainnet release for {version}, trying testnet");
            let testnet_url = format!(
                "https://github.com/MystenLabs/sui/releases/download/testnet-v{version}/sui-testnet-v{version}-{platform}.tgz",
            );
            ureq::get(&testnet_url).call()?
        }
        Err(e) => return Err(e.into()),
    };
    Ok(response.into_reader())
}

/// Read a gzipped tar archive from `reader` and unpack only the `sui` binary to `dest`, discarding
/// every other entry as it streams. Errors if the archive for `version` contains no `sui` binary.
fn extract_sui_from_stream(
    reader: impl io::Read,
    version: &str,
    platform: &str,
    dest: &Path,
) -> anyhow::Result<()> {
    let tar = flate2::read::GzDecoder::new(reader);
    let mut archive = Archive::new(tar);
    let entries = archive.entries().context("reading release archive")?;

    for entry in entries {
        let mut entry = entry.context("reading release archive entry")?;
        let is_sui = {
            let path = entry.path().context("reading archive entry path")?;
            matches_sui(&path, platform)
        };
        if is_sui {
            entry.unpack(dest).context("unpacking the sui binary")?;
            return Ok(());
        }
    }
    Err(anyhow!(
        "no sui binary found in the {version} release archive"
    ))
}

/// Whether `path`, the path of an entry in a release archive, is the `sui` executable — either the
/// modern root-level `sui` or the older `target/release/sui-<platform>`. Other shipped binaries
/// (`sui-node`, `sui-tool`, ...) do not match.
fn matches_sui(path: &Path, platform: &str) -> bool {
    let suffix = if platform == "windows-x86_64" {
        ".exe"
    } else {
        ""
    };
    let names = [format!("sui{suffix}"), format!("sui-{platform}{suffix}")];
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| names.iter().any(|candidate| candidate == n))
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
                 (OS: {os}, architecture: {arch})"
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
    use std::io::Write;

    use super::*;

    /// Build a gzipped tar archive from `(path, contents)` entries, as the release download stream
    /// would deliver it.
    fn make_tgz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut tar = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar);
            for (name, data) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_size(data.len() as u64);
                header.set_mode(0o755);
                builder.append_data(&mut header, name, *data).unwrap();
            }
            builder.finish().unwrap();
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar).unwrap();
        gz.finish().unwrap()
    }

    /// `matches_sui` accepts the modern root-level `sui` and the older `target/release/sui-<platform>`
    /// layouts, ignores other shipped binaries, and honours the windows `.exe` suffix.
    #[test]
    fn matches_sui_by_layout() {
        assert!(matches_sui(Path::new("sui"), "macos-arm64"));
        assert!(matches_sui(
            Path::new("target/release/sui-macos-arm64"),
            "macos-arm64"
        ));
        assert!(!matches_sui(Path::new("sui-node"), "macos-arm64"));
        assert!(!matches_sui(
            Path::new("target/release/sui-tool"),
            "macos-arm64"
        ));
        assert!(matches_sui(Path::new("sui.exe"), "windows-x86_64"));
        assert!(!matches_sui(Path::new("sui"), "windows-x86_64"));
    }

    /// Streaming extraction writes only the `sui` binary (with its exact bytes) and nothing else,
    /// even when other binaries precede and follow it in the archive.
    #[test]
    fn extract_takes_only_sui() {
        let tgz = make_tgz(&[
            ("sui-node", b"NODE"),
            ("sui", b"SUI-BINARY"),
            ("sui-tool", b"TOOL"),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("sui");

        extract_sui_from_stream(tgz.as_slice(), "1.0.0", "macos-arm64", &dest).unwrap();

        assert_eq!(fs::read(&dest).unwrap(), b"SUI-BINARY");
        // Nothing but `sui` landed on disk.
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    /// An archive with no `sui` binary is an error rather than a silent success.
    #[test]
    fn extract_errors_without_sui() {
        let tgz = make_tgz(&[("sui-node", b"NODE"), ("sui-tool", b"TOOL")]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("sui");

        let err =
            extract_sui_from_stream(tgz.as_slice(), "1.0.0", "macos-arm64", &dest).unwrap_err();
        assert!(err.to_string().contains("no sui binary"));
    }
}
