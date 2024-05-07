// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, process::Command};

use clap::Parser;

#[derive(Debug, Copy, Clone)]
pub enum Architecture {
    BpfEl,
    BpfEb,
}

impl std::str::FromStr for Architecture {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "bpfel-unknown-none" => Architecture::BpfEl,
            "bpfeb-unknown-none" => Architecture::BpfEb,
            _ => return Err("invalid target".to_owned()),
        })
    }
}

impl std::fmt::Display for Architecture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Architecture::BpfEl => "bpfel-unknown-none",
            Architecture::BpfEb => "bpfeb-unknown-none",
        })
    }
}

#[derive(Debug, Parser)]
pub struct Options {
    /// Set the endianness of the BPF target
    #[clap(default_value = "bpfel-unknown-none", long)]
    pub target: Architecture,
    /// Build the release target
    #[clap(long)]
    pub release: bool,
}

pub fn build_ebpf(opts: Options) -> Result<(), anyhow::Error> {
    let dir = PathBuf::from("nodefw-ebpf");
    let target = format!("--target={}", opts.target);
    let mut args = vec!["build", target.as_str(), "-Z", "build-std=core"];
    if opts.release {
        args.push("--release")
    }

    // Command::new creates a child process which inherits all env variables. This means env
    // vars set by the cargo xtask command are also inherited. RUSTUP_TOOLCHAIN is removed
    // so the rust-toolchain.toml file in the -ebpf folder is honored.

    let status = Command::new("cargo")
        .current_dir(dir)
        .env("RUSTC_BOOTSTRAP", "1")
        .env_remove("RUSTUP_TOOLCHAIN")
        .args(&args)
        .status()
        .expect("failed to build bpf program");
    assert!(status.success());
    Ok(())
}
