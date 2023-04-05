// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, bail, format_err, Result};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::{self},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

// Unfortunately, solc can run the Yul compiler only to stdout, not supporting the -o option.
// It uses some markers in stdout which we match on to find the relevant parts. This is fragile
// and may need to be fixed if solc starts changing its output.
const OPTIMIZED_YUL_MARKER: &str = "\nPretty printed source:";
const HEX_OUTPUT_MARKER: &str = "\nBinary representation:";

fn solc_path() -> Result<PathBuf> {
    let solc_exe = move_command_line_common::env::read_env_var("SOLC_EXE");

    if solc_exe.is_empty() {
        bail!(
            "failed to resolve path to solc (Solidity compiler).
            Is the environment variable SOLC_EXE set?
            Did you run `./scripts/dev_setup.sh -d`?"
        )
    }

    Ok(PathBuf::from(&solc_exe))
}

fn solc_impl(
    source_paths: impl IntoIterator<Item = impl AsRef<OsStr>>,
    output_dir: &Path,
) -> Result<BTreeMap<String, Vec<u8>>> {
    Command::new(&solc_path()?)
        .args(source_paths)
        .arg("--bin")
        .arg("-o")
        .arg(output_dir)
        .output()
        .map_err(|err| format_err!("failed to call solc (solidity compiler): {:?}", err))?;

    let mut compiled_contracts = BTreeMap::new();

    for entry in fs::read_dir(output_dir)? {
        let entry = entry?;

        if entry.file_type()?.is_file() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "bin" {
                    let data = fs::read(&path)?;
                    let data = hex::decode(&data)?;

                    compiled_contracts.insert(
                        path.file_stem()
                            .ok_or_else(|| format_err!("failed to extract file name"))?
                            .to_string_lossy()
                            .to_string(),
                        data,
                    );
                }
            }
        }
    }

    Ok(compiled_contracts)
}

/// Compile the solidity sources using solc.
/// Return a mapping with keys being contract names and values being compiled bytecode.
///
/// The environment variable SOLC_EXE must point to solc.
pub fn solc(
    source_paths: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let temp = tempfile::tempdir()?;

    solc_impl(source_paths, temp.path())
}

/// Compile the Yul source, given as a string, and return the binary representation of the
/// compiled bytecode. If `return_optimized_yul` is true, also return the textual representation
/// of optimized Yul.
pub fn solc_yul(source: &str, return_optimized_yul: bool) -> Result<(Vec<u8>, Option<String>)> {
    let mut prog = Command::new(&solc_path()?);
    prog.arg("--optimize").arg("--strict-assembly").arg("--bin");
    if return_optimized_yul {
        prog.arg("--ir-optimized");
    }
    let mut child = prog
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let pipe = child.stdin.as_mut().ok_or(anyhow!("cannot create pipe"))?;
    pipe.write_all(source.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        return Err(anyhow!(String::from_utf8_lossy(&out.stderr).to_string()));
    }
    let out_str = String::from_utf8_lossy(&out.stdout).to_string();
    let start_of_yul = out_str.find(OPTIMIZED_YUL_MARKER);
    let start_of_hex = out_str.find(HEX_OUTPUT_MARKER);
    if return_optimized_yul && start_of_yul.is_none() || start_of_hex.is_none() {
        return Err(anyhow!(
            "Internal error: unexpected output of solc during Yul compilation"
        ));
    }
    let yul = if return_optimized_yul {
        Some(
            out_str[(start_of_yul.unwrap() + OPTIMIZED_YUL_MARKER.len())..start_of_hex.unwrap()]
                .trim()
                .to_string(),
        )
    } else {
        None
    };
    let bin = hex::decode(out_str[(start_of_hex.unwrap() + HEX_OUTPUT_MARKER.len())..].trim())?;
    Ok((bin, yul))
}
