// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::Path;

use move_binary_format::CompiledModule;
use serde::Deserialize;

use crate::error::Error;

/// Read the root package's already-compiled modules from `package_path/build/<name>/bytecode_modules`
/// without invoking the compiler, where `<name>` is the package's name from its `Move.toml`. Only the
/// root package's modules are read; the `dependencies` subdirectory is skipped. Returns an error if
/// the build directory is missing or empty (the package has not been built).
pub(crate) fn read_modules(package_path: &Path) -> Result<Vec<CompiledModule>, Error> {
    let dir = package_path
        .join("build")
        .join(package_name(package_path)?)
        .join("bytecode_modules");

    let entries = fs::read_dir(&dir).map_err(|e| Error::BuildOutputParse {
        message: format!("could not read build directory {}: {e}", dir.display()),
    })?;

    let mut modules = vec![];
    for entry in entries.flatten() {
        let path = entry.path();
        // Skips the `dependencies` subdirectory and any non-module files.
        if path.extension().and_then(|e| e.to_str()) != Some("mv") {
            continue;
        }
        let bytes = fs::read(&path).map_err(|e| Error::BuildOutputParse {
            message: format!("could not read {}: {e}", path.display()),
        })?;
        let module = CompiledModule::deserialize_with_defaults(&bytes).map_err(|e| {
            Error::BuildOutputParse {
                message: format!("could not deserialize {}: {e}", path.display()),
            }
        })?;
        modules.push(module);
    }

    if modules.is_empty() {
        return Err(Error::BuildOutputParse {
            message: format!(
                "no compiled modules in {}; build the package first",
                dir.display()
            ),
        });
    }
    Ok(modules)
}

/// The root package's name, read from `Move.toml`; it names the `build/<name>` subdirectory.
fn package_name(package_path: &Path) -> Result<String, Error> {
    let manifest_path = package_path.join("Move.toml");
    let contents = fs::read_to_string(&manifest_path).map_err(|e| Error::BuildOutputParse {
        message: format!("could not read {}: {e}", manifest_path.display()),
    })?;

    #[derive(Deserialize)]
    struct Package {
        name: String,
    }
    #[derive(Deserialize)]
    struct Manifest {
        package: Package,
    }

    let manifest: Manifest = toml::from_str(&contents).map_err(|e| Error::BuildOutputParse {
        message: format!("could not parse {}: {e}", manifest_path.display()),
    })?;
    Ok(manifest.package.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use move_binary_format::file_format::empty_module;
    use move_binary_format::file_format_common::VERSION_MAX;

    fn write_module(path: &Path) {
        let mut bytes = vec![];
        empty_module()
            .serialize_with_version(VERSION_MAX, &mut bytes)
            .unwrap();
        fs::write(path, bytes).unwrap();
    }

    /// Reads the root package's `.mv` modules and ignores the `dependencies` subdirectory.
    #[test]
    fn reads_root_modules_only() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg = tmp.path();
        fs::write(pkg.join("Move.toml"), "[package]\nname = \"demo\"\n").unwrap();

        let modules = pkg.join("build").join("demo").join("bytecode_modules");
        fs::create_dir_all(modules.join("dependencies")).unwrap();
        write_module(&modules.join("demo.mv"));
        write_module(&modules.join("dependencies").join("dep.mv"));

        assert_eq!(read_modules(pkg).unwrap().len(), 1);
    }

    /// Errors when the package has not been built.
    #[test]
    fn errors_when_unbuilt() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Move.toml"), "[package]\nname = \"demo\"\n").unwrap();
        assert!(read_modules(tmp.path()).is_err());
    }
}
