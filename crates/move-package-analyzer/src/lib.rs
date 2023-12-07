// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::PackageAnalyzerError;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::{
    collections::HashSet,
    {fs, path::Path},
};
use sui_types::base_types::ObjectID;

pub mod errors;
pub mod load_from_csv;
pub mod load_from_dir;
pub mod model;
pub mod passes;
pub mod query_indexer;

// Global constants
const DEFAULT_CAPACITY: usize = 16 * 1024;
const PACKAGE_BCS: &str = "package.bcs";

/// Known framework packages
pub static FRAMEWORK: Lazy<HashSet<ObjectID>> = Lazy::new(|| {
    let mut framework = HashSet::new();
    // move std lib
    framework.insert(
        ObjectID::from_hex_literal(
            "0x0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap(),
    );
    // sui framework
    framework.insert(
        ObjectID::from_hex_literal(
            "0x0000000000000000000000000000000000000000000000000000000000000002",
        )
        .unwrap(),
    );
    // sui system
    framework.insert(
        ObjectID::from_hex_literal(
            "0x0000000000000000000000000000000000000000000000000000000000000003",
        )
        .unwrap(),
    );
    // deepbook
    framework.insert(
        ObjectID::from_hex_literal(
            "0x000000000000000000000000000000000000000000000000000000000000dee9",
        )
        .unwrap(),
    );
    // bridge
    framework.insert(
        ObjectID::from_hex_literal(
            "0x000000000000000000000000000000000000000000000000000000000000000b",
        )
        .unwrap(),
    );
    framework
});

/// Passes as loaded from `passes.yaml`.
#[derive(Debug, Deserialize)]
pub struct PassesConfig {
    /// Passes to run. May have duplicates.
    pub passes: Vec<Pass>,
    // Directory where output files are written.
    pub output_dir: Option<String>,
}

/// Passes available.
/// Refer to the file of the pass for detailed info on the pass itself.
/// Annoyance: when adding a pass one has to come here, add a variant, and add it in
/// `pass_manager.rs` as well. It can then be called from `passes.yaml`.
/// We may review how a pass is exported but for now we'll survive.
#[derive(Debug, Deserialize)]
pub enum Pass {
    /// No pass, just to have something in the `passes.yaml` file.
    Noop,
    /// Passes available.
    /// Refer to the file of the pass for detailed info on the pass itself.
    /// Annoyance: when adding a pass one has to come here, add a variant, and add it in
    /// `pass_manager.rs` as well. It can then be called from `passes.yaml`.
    /// We may review how a pass is exported but for now we'll survive.
    /// Write summary information of the environment in a `summary.txt` file.
    Summary,
    /// Write out packages in a compact, convenient format.
    DumpEnv,
    /// Write out `csv` files for all expected vm/language entities in the system:
    /// packages, modules, functions, structs, ...
    CsvEntities,
    /// Report (`versions.txt`) version information for packages that went through
    /// upgrades.
    Versions,
    /// Report all public/entry calls into specified functions.
    FindCallers(Vec<CallInfo>),
    /// Report all calls to specified modules.
    /// A module has the form <package_id>::<module_name>
    /// e.g. 0x0000000000000000000000000000000000000000000000000000000000000002::dynamic_field
    CallsToModule(Vec<String>),
}

#[derive(Debug, Deserialize)]
pub struct CallInfo {
    /// The function to look for, with the fomat `0xpackage_id::module_name::function_name`
    pub function: String,
    /// The instantiation in case of generic functions.
    /// When empty, generic parameters are not considered, or the function is not
    /// generic.
    pub instantiation: Vec<String>,
}

/// Load the passes to run.
pub fn load_config(path: &Path) -> Result<PassesConfig, PackageAnalyzerError> {
    let reader = fs::File::open(path).map_err(|e| {
        PackageAnalyzerError::BadConfig(format!(
            "Cannot open config file {}: {}",
            path.display(),
            e
        ))
    })?;
    let config: PassesConfig = serde_yaml::from_reader(reader).map_err(|e| {
        PackageAnalyzerError::BadConfig(format!(
            "Cannot parse config file {}: {}",
            path.display(),
            e
        ))
    })?;
    Ok(config)
}

/// Idiotic helper to write out to a file and report the error on failures.
#[macro_export]
macro_rules! write_to {
    ($file:expr, $($arg:tt)*) => {{
        writeln!($file, $($arg)*).unwrap_or_else(|e| error!(
            "Unable to write to file: {}",
            e.to_string()
        ))
    }};
}
