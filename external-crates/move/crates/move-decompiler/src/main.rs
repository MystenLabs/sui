// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use std::{
    fs,
    panic,
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Debug, Parser)]
#[clap(
    name = "move-decompiler",
    about = "Decompile Move bytecode packages from a corpus",
    author,
    version
)]
struct Args {
    /// Path to process: package directory, hex prefix directory, or corpus root
    #[clap(value_name = "PATH")]
    path: PathBuf,

    /// Output directory for decompiled modules (defaults to <package>/decompiled_output)
    #[clap(short = 'o', long = "output")]
    output_dir: Option<PathBuf>,

    /// Continue processing even if errors occur
    #[clap(short = 'c', long = "continue-on-error")]
    continue_on_error: bool,

    /// Show detailed progress for each package
    #[clap(short = 'v', long = "verbose")]
    verbose: bool,
}

#[derive(Debug)]
enum PathType {
    Package(PathBuf),
    HexPrefix(PathBuf),
    CorpusRoot(PathBuf),
}

#[derive(Debug)]
struct PackageInfo {
    package_id: String,
    path: PathBuf,
    mv_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResultStatus {
    Success,
    Failed,
    Skipped,
}

#[derive(Debug)]
struct DecompilationResult {
    package_id: String,
    status: ResultStatus,
    error: Option<String>,
    modules_count: usize,
}

#[derive(Debug)]
struct Summary {
    total_packages: usize,
    successful: usize,
    failed: usize,
    skipped: usize,
    total_modules: usize,
    errors: Vec<(String, String)>,
}

impl Summary {
    fn new(total_packages: usize) -> Self {
        Self {
            total_packages,
            successful: 0,
            failed: 0,
            skipped: 0,
            total_modules: 0,
            errors: Vec::new(),
        }
    }

    fn add_result(&mut self, result: &DecompilationResult) {
        match result.status {
            ResultStatus::Success => {
                self.successful += 1;
                self.total_modules += result.modules_count;
            }
            ResultStatus::Failed => {
                self.failed += 1;
                if let Some(error) = &result.error {
                    self.errors.push((result.package_id.clone(), error.clone()));
                }
            }
            ResultStatus::Skipped => {
                self.skipped += 1;
            }
        }
    }

    fn print(&self) {
        println!("\nSummary");
        println!("----------------------------------");
        println!("Total Packages:    {}", self.total_packages);
        println!("Successful:        {}", self.successful);
        println!("Failed:            {}", self.failed);
        println!("Skipped:           {}", self.skipped);
        println!("Total Modules:     {}", self.total_modules);

        if !self.errors.is_empty() {
            println!("\nErrors:");
            for (package_id, error) in &self.errors {
                println!("  - {}: {}", package_id, error);
            }
        }
    }
}

fn detect_path_type(path: &Path) -> Result<PathType> {
    if !path.exists() {
        return Err(anyhow!("Path does not exist: {}", path.display()));
    }

    if !path.is_dir() {
        return Err(anyhow!("Path is not a directory: {}", path.display()));
    }

    // Check if it's a package directory (contains bytecode_modules/)
    let bytecode_modules_path = path.join("bytecode_modules");
    if bytecode_modules_path.exists() && bytecode_modules_path.is_dir() {
        return Ok(PathType::Package(path.to_path_buf()));
    }

    // Check if any subdirectory contains bytecode_modules/
    let entries = fs::read_dir(path)
        .with_context(|| format!("Failed to read directory: {}", path.display()))?;

    let mut has_hex_prefix_subdirs = false;
    let mut has_package_subdirs = false;

    for entry in entries.flatten() {
        let subdir_path = entry.path();

        // Use metadata() to follow symlinks
        if let Ok(metadata) = fs::metadata(&subdir_path) {
            if metadata.is_dir() {
                let subdir_name = subdir_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");

                // Check if this is a hex prefix directory (0x00-0xff)
                if subdir_name.len() == 4
                    && subdir_name.starts_with("0x")
                    && subdir_name[2..].chars().all(|c| c.is_ascii_hexdigit())
                {
                    has_hex_prefix_subdirs = true;
                }

                // Check if subdirectory has bytecode_modules/
                if subdir_path.join("bytecode_modules").exists() {
                    has_package_subdirs = true;
                }
            }
        }
    }

    if has_hex_prefix_subdirs {
        Ok(PathType::CorpusRoot(path.to_path_buf()))
    } else if has_package_subdirs {
        Ok(PathType::HexPrefix(path.to_path_buf()))
    } else {
        Err(anyhow!(
            "Path does not appear to be a package, hex prefix directory, or corpus root: {}",
            path.display()
        ))
    }
}

fn collect_mv_files(bytecode_modules_path: &Path) -> Result<Vec<PathBuf>> {
    let mut mv_files = Vec::new();

    if !bytecode_modules_path.exists() {
        return Ok(mv_files);
    }

    let entries = fs::read_dir(bytecode_modules_path).with_context(|| {
        format!(
            "Failed to read bytecode_modules directory: {}",
            bytecode_modules_path.display()
        )
    })?;

    for entry in entries.flatten() {
        if let Ok(file_type) = entry.file_type() {
            if file_type.is_file() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("mv") {
                    mv_files.push(path);
                }
            }
        }
    }

    mv_files.sort();
    Ok(mv_files)
}

fn collect_packages_from_dir(dir: &Path) -> Result<Vec<PackageInfo>> {
    let mut packages = Vec::new();

    let entries = fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?;

    for entry in entries.flatten() {
        let package_path = entry.path();

        // Use metadata() to follow symlinks
        if let Ok(metadata) = fs::metadata(&package_path) {
            if metadata.is_dir() {
                let bytecode_modules_path = package_path.join("bytecode_modules");

                if bytecode_modules_path.exists() {
                    let mv_files = collect_mv_files(&bytecode_modules_path)?;
                    let package_id = package_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();

                    packages.push(PackageInfo {
                        package_id,
                        path: package_path,
                        mv_files,
                    });
                }
            }
        }
    }

    packages.sort_by(|a, b| a.package_id.cmp(&b.package_id));
    Ok(packages)
}

fn collect_packages(path_type: PathType) -> Result<Vec<PackageInfo>> {
    match path_type {
        PathType::Package(path) => {
            let bytecode_modules_path = path.join("bytecode_modules");
            let mv_files = collect_mv_files(&bytecode_modules_path)?;
            let package_id = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            Ok(vec![PackageInfo {
                package_id,
                path,
                mv_files,
            }])
        }
        PathType::HexPrefix(path) => collect_packages_from_dir(&path),
        PathType::CorpusRoot(path) => {
            let mut all_packages = Vec::new();

            let entries = fs::read_dir(&path)
                .with_context(|| format!("Failed to read corpus root: {}", path.display()))?;

            for entry in entries.flatten() {
                let hex_prefix_path = entry.path();

                // Use metadata() to follow symlinks
                if let Ok(metadata) = fs::metadata(&hex_prefix_path) {
                    if metadata.is_dir() {
                        let packages = collect_packages_from_dir(&hex_prefix_path)?;
                        all_packages.extend(packages);
                    }
                }
            }

            all_packages.sort_by(|a, b| a.package_id.cmp(&b.package_id));
            Ok(all_packages)
        }
    }
}

fn decompile_package(
    package: &PackageInfo,
    output_dir: Option<&Path>,
    verbose: bool,
) -> DecompilationResult {
    let package_id = &package.package_id;

    if package.mv_files.is_empty() {
        if verbose {
            println!("  No .mv files found");
        }
        return DecompilationResult {
            package_id: package_id.clone(),
            status: ResultStatus::Skipped,
            error: Some("no .mv files".to_string()),
            modules_count: 0,
        };
    }

    if verbose {
        println!("  Found {} module(s)", package.mv_files.len());
    }

    let output_path = match output_dir {
        Some(dir) => dir.to_path_buf(),
        None => package.path.join("decompiled_output"),
    };

    if verbose {
        println!("  Output directory: {}", output_path.display());
    }

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        move_decompiler::generate_from_files(&package.mv_files, &output_path)
    }));

    match result {
        Ok(Ok(output_files)) => {
            if verbose {
                println!("  Generated {} file(s):", output_files.len());
                for file in &output_files {
                    println!("    - {}", file.display());
                }
            }
            DecompilationResult {
                package_id: package_id.clone(),
                status: ResultStatus::Success,
                error: None,
                modules_count: output_files.len(),
            }
        }
        Ok(Err(e)) => DecompilationResult {
            package_id: package_id.clone(),
            status: ResultStatus::Failed,
            error: Some(e.to_string()),
            modules_count: 0,
        },
        Err(panic_err) => {
            let panic_msg = if let Some(s) = panic_err.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_err.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            DecompilationResult {
                package_id: package_id.clone(),
                status: ResultStatus::Failed,
                error: Some(format!("Panic: {}", panic_msg)),
                modules_count: 0,
            }
        }
    }
}

fn truncate_package_id(package_id: &str) -> String {
    if package_id.len() > 16 {
        format!("{}...{}", &package_id[..8], &package_id[package_id.len() - 4..])
    } else {
        package_id.to_string()
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let start_time = Instant::now();

    // Detect path type
    let path_type = detect_path_type(&args.path)?;
    if args.verbose {
        println!("Detected path type: {:?}", path_type);
    }

    // Collect packages
    println!("Collecting packages...");
    let packages = collect_packages(path_type)?;
    println!("Found {} packages to process\n", packages.len());

    if packages.is_empty() {
        println!("No packages found to process.");
        return Ok(());
    }

    // Process packages
    let mut summary = Summary::new(packages.len());

    for (idx, package) in packages.iter().enumerate() {
        let package_display = truncate_package_id(&package.package_id);
        println!("[{}/{}] Processing: {}", idx + 1, packages.len(), package_display);

        let result = decompile_package(package, args.output_dir.as_deref(), args.verbose);

        match result.status {
            ResultStatus::Success => {
                println!("  Success: {} modules", result.modules_count);
            }
            ResultStatus::Failed => {
                println!("  Failed: {}", result.error.as_ref().unwrap());
                if !args.continue_on_error {
                    return Err(anyhow!(
                        "Decompilation failed for package: {}",
                        package.package_id
                    ));
                }
            }
            ResultStatus::Skipped => {
                println!("  Skipped: {}", result.error.as_ref().unwrap());
            }
        }

        summary.add_result(&result);
    }

    summary.print();

    let elapsed = start_time.elapsed();
    println!("\nTotal time: {:.2}s", elapsed.as_secs_f64());

    Ok(())
}
