// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    errors::PackageResult,
    flavor::Vanilla,
    package::{lockfile::Lockfile, manifest::Manifest},
};

use std::path::PathBuf;

#[derive(Debug, Clone, clap::Parser)]
pub struct Parse {
    /// The path to the project
    path: Option<PathBuf>,
    /// The path to the manifest file
    #[clap(
        name = "manifest",
        short = 'm',
        long = "manifest",
        conflicts_with("path")
    )]
    manifest: Option<PathBuf>,
    /// The path to the lockfile
    #[clap(
        name = "lockfile",
        short = 'l',
        long = "lockfile",
        conflicts_with("path")
    )]
    lockfile: Option<PathBuf>,
}

impl Parse {
    pub fn execute(&self) -> PackageResult<()> {
        let manifest = &self.manifest;
        let lockfile = &self.lockfile;
        let path = &self.path;
        if manifest.is_none() && lockfile.is_none() {
            let default_path = PathBuf::from(".");
            let path = path.as_ref().unwrap_or(&default_path);
            let manifest_path = path.join("Move.toml");
            let lockfile_path = path.join("Move.lock");
            if manifest_path.exists() {
                println!("Manifest file found at: {:?}", manifest_path);
                let manifest = Manifest::<Vanilla>::read_from(&manifest_path);
                match manifest {
                    Ok(manifest) => {
                        println!("{:?}", manifest);
                    }
                    Err(e) => {
                        eprintln!("Error reading manifest: {}", e);
                    }
                }
            }
            if lockfile_path.exists() {
                println!("Lockfile found at: {:?}", lockfile_path);
                let lockfile = Lockfile::<Vanilla>::read_from(&lockfile_path);
                match lockfile {
                    Ok(lockfile) => {
                        println!("{:?}", lockfile);
                    }
                    Err(e) => {
                        eprintln!("Error reading lockfile: {}", e);
                    }
                }
            }
        }

        if let Some(manifest_path) = manifest {
            let m = Manifest::<Vanilla>::read_from(manifest_path);
            match m {
                Ok(manifest) => {
                    println!("{:?}", manifest);
                }
                Err(e) => {
                    eprintln!("Error reading manifest: {}", e);
                }
            }
        }

        if let Some(lockfile_path) = lockfile {
            let lockfile = Lockfile::<Vanilla>::read_from(lockfile_path);
            match lockfile {
                Ok(lockfile) => {
                    println!("{:?}", lockfile);
                }
                Err(e) => {
                    eprintln!("Error reading lockfile: {}", e);
                }
            }
        }
        Ok(())
    }
}
