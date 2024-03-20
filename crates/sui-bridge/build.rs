// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{exit, Command, ExitStatus};

fn main() -> Result<(), ExitStatus> {
    #[cfg(windows)]
    {
        eprintln!("bridge tests are not supported on Windows.");
        return Ok(());
    }

    println!("cargo:rerun-if-changed=build.rs");
    let base = env!("CARGO_MANIFEST_DIR");
    let bridge_path = format!("{base}/../../bridge/evm");
    let bridge_lib_path = format!("{base}/../../bridge/evm/lib");
    let mut forge_path = "forge".to_owned();
    // Check if Forge is installed
    let forge_installed = Command::new("which")
        .arg("forge")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !forge_installed {
        eprintln!("Installing forge");
        // Also print the path where foundryup is installed
        let install_cmd = "curl -L https://foundry.paradigm.xyz | { cat; echo 'echo foundryup-path=\"$FOUNDRY_BIN_DIR/foundryup\"'; } | bash";

        let output = Command::new("sh")
            .arg("-c")
            .arg(install_cmd)
            .output()
            .expect("Failed to install Forge");

        // extract foundryup path
        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut foundryup_path = None;
        for line in output_str.lines() {
            if line.starts_with("foundryup-path=") {
                foundryup_path = Some(line.trim_start_matches("foundryup-path="));
                break;
            }
        }
        if foundryup_path.is_none() {
            eprintln!("Error installing forge: expect a foundry path in output");
            exit(1);
        }
        let foundryup_path = foundryup_path.unwrap();
        eprintln!("foundryup path: {foundryup_path}");
        // Run foundryup
        let output = Command::new(foundryup_path)
            .output()
            .expect("Failed to run foundryup");

        if !output.status.success() {
            eprintln!("Error running foundryup: {:?}", output);
            exit(1);
        }
        // Update forge path
        let mut forge = PathBuf::from(foundryup_path);
        forge.pop();
        forge.push("forge");
        forge_path = forge.to_str().unwrap().to_owned();
    }

    // check if should install dependencies
    if should_install_dependencies(&bridge_lib_path) {
        // Run Foundry CLI command to install dependencies
        Command::new(forge_path)
            .current_dir(bridge_path)
            .arg("install")
            .arg("https://github.com/OpenZeppelin/openzeppelin-contracts-upgradeable@v5.0.1")
            .arg("https://github.com/foundry-rs/forge-std@v1.3.0")
            .arg("https://github.com/OpenZeppelin/openzeppelin-foundry-upgrades")
            .arg("--no-git")
            .arg("--no-commit")
            .status()
            .expect("Failed to execute Foundry CLI command");
    }

    Ok(())
}

fn should_install_dependencies(dir_path: &str) -> bool {
    let dependencies = vec![
        "forge-std",
        "openzeppelin-contracts-upgradeable",
        "openzeppelin-foundry-upgrades",
    ];
    let mut missing_dependencies = false;
    for d in &dependencies {
        let path = format!("{}/{}", dir_path, d);
        let path = Path::new(&path);
        if !path.exists() {
            missing_dependencies = true;
            break;
        }
    }
    // found all dependencies, no need to install
    if !missing_dependencies {
        return false;
    }
    // if any dependencies are missing, recreate an empty directory and then reinstall
    eprintln!(
        "cargo:warning={:?} does not have all the dependnecies, re-creating",
        dir_path
    );
    if Path::new(&dir_path).exists() {
        fs::remove_dir_all(dir_path).unwrap();
    }
    fs::create_dir_all(dir_path).expect("Failed to create directory");
    true
}
