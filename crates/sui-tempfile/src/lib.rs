// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::{
    env,
    fs::File,
    path::{Path, PathBuf},
};

use once_cell::sync::Lazy;
pub use tempfile::TempDir;
use tracing::{error, warn};

const RAMDISK_FLAG: &str = "SUI_TEMPFILE_USE_RAMDISK";
const RAMDISK_VOLUME_PATH: &str = "/Volumes/sui_ramdisk";

pub static USE_RAMDISK: Lazy<bool> = Lazy::new(|| read_ramdisk_flag_env().unwrap_or(false));

pub fn read_ramdisk_flag_env() -> Option<bool> {
    env::var(RAMDISK_FLAG)
        .ok()?
        .parse::<bool>()
        .map_err(|e| {
            println!(
                "Env var {} does not contain valid usize integer: {}",
                RAMDISK_FLAG, e
            )
        })
        .ok()
}

pub struct SuiTempFile {}

impl SuiTempFile {
    pub fn tempdir_disk() -> PathBuf {
        env::temp_dir()
    }

    pub fn tempdir_ramdisk() -> Result<PathBuf, anyhow::Error> {
        Ok(PathBuf::from(RAMDISK_VOLUME_PATH))
    }

    pub fn base_dir() -> PathBuf {
        println!("USE_RAMDISK: {:?}", *USE_RAMDISK);
        if *USE_RAMDISK && cfg!(target_os = "macos") {
            println!("Using ramdisk");
            match Self::tempdir_ramdisk() {
                Ok(path) => path,
                Err(e) => {
                    println!("Unable to use ramdisk: {:?}", e);
                    println!("Falling back to normal disk: {:?}", e);
                    Self::tempdir_disk()
                }
            }
        } else {
            println!("Using normal disk");
            Self::tempdir_disk()
        }
    }

    pub fn temp_file() -> io::Result<tempfile::NamedTempFile> {
        let base_dir = Self::base_dir();
        println!("base_dir: {:?}", base_dir);
        tempfile::Builder::new().tempfile_in(base_dir)
    }

    pub fn temp_dir() -> io::Result<tempfile::TempDir> {
        let base_dir = Self::base_dir();
        println!("base_dir: {:?}", base_dir);

        tempfile::Builder::new().tempdir_in(base_dir)
    }
}

pub fn tempfile() -> io::Result<tempfile::NamedTempFile> {
    SuiTempFile::temp_file()
}

pub fn tempdir() -> io::Result<tempfile::TempDir> {
    SuiTempFile::temp_dir()
}
