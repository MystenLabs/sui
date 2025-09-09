// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::build_config::BuildConfig;
use anyhow::Result;
use move_command_line_common::files::find_move_filenames;
use move_compiler::shared::files::FileName;
use move_package_alt::package::{layout::SourcePackageLayout, paths::PackagePath};
use std::path::{Path, PathBuf};

// Find all the source files for a package at the given path
pub fn get_sources(path: &PackagePath, config: &BuildConfig) -> Result<Vec<FileName>> {
    let places_to_look = source_paths_for_config(path.path(), config);
    Ok(find_move_filenames(&places_to_look, false)?
        .into_iter()
        .map(FileName::from)
        .collect())
}

/// Get the source paths to look for source files in a package at the given path, based on the
/// build config flags.
fn source_paths_for_config(package_path: &Path, config: &BuildConfig) -> Vec<PathBuf> {
    let mut places_to_look = Vec::new();
    let mut add_path = |layout_path: SourcePackageLayout| {
        let path = package_path.join(layout_path.path());
        if layout_path.is_optional() && !path.exists() {
            return;
        }
        places_to_look.push(path)
    };

    add_path(SourcePackageLayout::Sources);
    add_path(SourcePackageLayout::Scripts);

    if config.test_mode {
        add_path(SourcePackageLayout::Tests);
    }

    places_to_look
        .into_iter()
        .filter(|path| path.exists())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use move_package_alt::package::paths::PackagePath;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_package_structure(root: &Path) -> Result<()> {
        // Create Move.toml file (required for PackagePath)
        fs::write(
            root.join("Move.toml"),
            "[package]\nname = \"test\"\nversion = \"0.0.1\"",
        )?;

        // Create standard Move package directories
        fs::create_dir_all(root.join("sources"))?;
        fs::create_dir_all(root.join("scripts"))?;
        fs::create_dir_all(root.join("tests"))?;

        // Create some Move files
        fs::write(root.join("sources/module1.move"), "module test::module1 {}")?;
        fs::write(root.join("sources/module2.move"), "module test::module2 {}")?;
        fs::write(
            root.join("scripts/script1.move"),
            "script { fun main() {} }",
        )?;
        fs::write(root.join("tests/test1.move"), "module test::test1 {}")?;

        // Create a non-Move file that should be ignored
        fs::write(root.join("sources/README.md"), "# README")?;

        Ok(())
    }

    #[test]
    fn test_get_sources_normal_mode() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let package_path = temp_dir.path();

        create_test_package_structure(package_path).expect("Failed to create test structure");

        let config = BuildConfig {
            test_mode: false,
            ..Default::default()
        };

        let pkg_path =
            PackagePath::new(package_path.to_path_buf()).expect("Failed to create package path");
        let sources = get_sources(&pkg_path, &config).expect("Failed to get sources");

        // In normal mode, should only get files from sources and scripts directories
        assert_eq!(sources.len(), 3);

        let source_paths: Vec<String> = sources.iter().map(|f| f.as_str().to_string()).collect();

        assert!(source_paths.iter().any(|p| p.ends_with("module1.move")));
        assert!(source_paths.iter().any(|p| p.ends_with("module2.move")));
        assert!(source_paths.iter().any(|p| p.ends_with("script1.move")));
        assert!(!source_paths.iter().any(|p| p.ends_with("test1.move")));
        assert!(!source_paths.iter().any(|p| p.ends_with("README.md")));
    }

    #[test]
    fn test_get_sources_test_mode() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let package_path = temp_dir.path();

        create_test_package_structure(package_path).expect("Failed to create test structure");

        let config = BuildConfig {
            test_mode: true,
            ..Default::default()
        };

        let pkg_path =
            PackagePath::new(package_path.to_path_buf()).expect("Failed to create package path");
        let sources = get_sources(&pkg_path, &config).expect("Failed to get sources");

        // In test mode, should get files from sources, scripts, and tests directories
        assert_eq!(sources.len(), 4);

        let source_paths: Vec<String> = sources.iter().map(|f| f.as_str().to_string()).collect();

        assert!(source_paths.iter().any(|p| p.ends_with("module1.move")));
        assert!(source_paths.iter().any(|p| p.ends_with("module2.move")));
        assert!(source_paths.iter().any(|p| p.ends_with("script1.move")));
        assert!(source_paths.iter().any(|p| p.ends_with("test1.move")));
        assert!(!source_paths.iter().any(|p| p.ends_with("README.md")));
    }

    #[test]
    fn test_get_sources_missing_directories() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let package_path = temp_dir.path();

        // Create Move.toml file (required for PackagePath)
        fs::write(
            package_path.join("Move.toml"),
            "[package]\nname = \"test\"\nversion = \"0.0.1\"",
        )
        .expect("Failed to write Move.toml");

        // Only create sources directory
        fs::create_dir_all(package_path.join("sources")).expect("Failed to create sources dir");
        fs::write(
            package_path.join("sources/module.move"),
            "module test::module {}",
        )
        .expect("Failed to write module file");

        let config = BuildConfig::default();
        let pkg_path =
            PackagePath::new(package_path.to_path_buf()).expect("Failed to create package path");
        let sources = get_sources(&pkg_path, &config).expect("Failed to get sources");

        // Should only get the one file from sources
        assert_eq!(sources.len(), 1);
        assert!(sources[0].as_str().ends_with("module.move"));
    }

    #[test]
    fn test_get_sources_empty_directories() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let package_path = temp_dir.path();

        // Create directories but don't put any Move files in them
        fs::create_dir_all(package_path.join("sources")).expect("Failed to create sources dir");
        fs::create_dir_all(package_path.join("scripts")).expect("Failed to create scripts dir");
        fs::write(
            package_path.join("Move.toml"),
            "[package]\nname = \"test\"\nversion = \"0.0.1\"",
        )
        .unwrap();

        let config = BuildConfig::default();
        let pkg_path =
            PackagePath::new(package_path.to_path_buf()).expect("Failed to create package path");
        let sources = get_sources(&pkg_path, &config).expect("Failed to get sources");

        // Should return empty vector
        assert_eq!(sources.len(), 0);
    }

    #[test]
    fn test_get_sources_nested_move_files() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let package_path = temp_dir.path();

        // Create nested directory structure
        fs::create_dir_all(package_path.join("sources/subdir"))
            .expect("Failed to create nested dir");
        fs::write(
            package_path.join("sources/module.move"),
            "module test::module {}",
        )
        .expect("Failed to write module file");
        fs::write(
            package_path.join("sources/subdir/nested.move"),
            "module test::nested {}",
        )
        .expect("Failed to write nested file");
        fs::write(
            package_path.join("Move.toml"),
            "[package]\nname = \"test\"\nversion = \"0.0.1\"",
        )
        .unwrap();

        let config = BuildConfig::default();
        let pkg_path =
            PackagePath::new(package_path.to_path_buf()).expect("Failed to create package path");
        let sources = get_sources(&pkg_path, &config).expect("Failed to get sources");

        // Should get both the top-level and nested Move files
        assert_eq!(sources.len(), 2);

        let source_paths: Vec<String> = sources.iter().map(|f| f.as_str().to_string()).collect();

        assert!(source_paths.iter().any(|p| p.ends_with("module.move")));
        assert!(source_paths.iter().any(|p| p.ends_with("nested.move")));
    }

    #[test]
    fn test_source_paths_for_config_normal_mode() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let package_path = temp_dir.path();

        // Create all possible directories
        fs::create_dir_all(package_path.join("sources")).expect("Failed to create sources dir");
        fs::create_dir_all(package_path.join("scripts")).expect("Failed to create scripts dir");
        fs::create_dir_all(package_path.join("tests")).expect("Failed to create tests dir");

        let config = BuildConfig {
            test_mode: false,
            ..Default::default()
        };

        let paths = source_paths_for_config(package_path, &config);

        // In normal mode, should only include sources and scripts
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().any(|p| p.ends_with("sources")));
        assert!(paths.iter().any(|p| p.ends_with("scripts")));
        assert!(!paths.iter().any(|p| p.ends_with("tests")));
    }

    #[test]
    fn test_source_paths_for_config_test_mode() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let package_path = temp_dir.path();

        // Create all possible directories
        fs::create_dir_all(package_path.join("sources")).expect("Failed to create sources dir");
        fs::create_dir_all(package_path.join("scripts")).expect("Failed to create scripts dir");
        fs::create_dir_all(package_path.join("tests")).expect("Failed to create tests dir");

        let config = BuildConfig {
            test_mode: true,
            ..Default::default()
        };

        let paths = source_paths_for_config(package_path, &config);

        // In test mode, should include sources, scripts, and tests
        assert_eq!(paths.len(), 3);
        assert!(paths.iter().any(|p| p.ends_with("sources")));
        assert!(paths.iter().any(|p| p.ends_with("scripts")));
        assert!(paths.iter().any(|p| p.ends_with("tests")));
    }
}
