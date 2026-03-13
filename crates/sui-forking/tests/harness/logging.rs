// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

pub fn create_process_log_file(log_dir: &Path, name: &str) -> io::Result<(PathBuf, File, File)> {
    fs::create_dir_all(log_dir)?;

    let log_path = log_dir.join(format!("{name}.log"));
    let log_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)?;

    let stdout = log_file.try_clone()?;
    let stderr = log_file;

    Ok((log_path, stdout, stderr))
}

pub fn read_process_log(log_path: &Path) -> String {
    fs::read_to_string(log_path).unwrap_or_else(|error| {
        format!(
            "<failed to read process log at {}: {error}>",
            log_path.display()
        )
    })
}
