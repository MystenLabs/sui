// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    errors::{FileHandle, PackageResult},
    flavor::Vanilla,
    package::{lockfile::Lockfiles, manifest::Manifest},
    schema::ParsedLockfile,
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
        let (manifest_path, lockfile_path) = match self {
            Parse {
                path,
                manifest: None,
                lockfile: None,
                ..
            } => (
                Some(path.clone().unwrap_or_default().join("Move.toml")),
                Some(path.clone().unwrap_or_default().join("Move.lock")),
            ),
            Parse {
                manifest, lockfile, ..
            } => (manifest.clone(), lockfile.clone()),
        };

        if let Some(manifest_path) = manifest_path {
            if !manifest_path.exists() {
                eprintln!("No manifest file at {:?}", manifest_path);
            } else {
                println!("Manifest file found at: {:?}", manifest_path);

                let manifest = Manifest::<Vanilla>::read_from_file(&manifest_path);
                match manifest {
                    Ok(manifest) => {
                        println!("{:?}", manifest);
                    }
                    Err(e) => {
                        eprintln!("Error reading manifest: {}", e);
                    }
                }
            }
        }

        if let Some(lockfile_path) = lockfile_path {
            if !lockfile_path.exists() {
                eprintln!("No lockfile at {:?}", lockfile_path);
            } else {
                println!("Lockfile found at: {:?}", lockfile_path);
                let file = FileHandle::new(&lockfile_path)?;
                let lockfile: ParsedLockfile<Vanilla> = toml_edit::de::from_str(file.source())?;

                println!("{:?}", lockfile);
            }
        }

        Ok(())
    }
}
