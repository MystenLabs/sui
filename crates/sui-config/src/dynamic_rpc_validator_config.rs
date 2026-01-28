// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Configuration for the dynamic RPC validator system
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DynamicRpcValidatorConfig {
    /// Path to the shared object file containing validation functions
    /// If None, dynamic validation is disabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library_path: Option<PathBuf>,

    /// How often to check for library file modifications (in seconds)
    /// Default: 60 seconds
    #[serde(default = "default_check_interval_secs")]
    pub check_interval_secs: u64,
}

fn default_check_interval_secs() -> u64 {
    60
}

impl DynamicRpcValidatorConfig {
    pub fn check_interval(&self) -> Duration {
        Duration::from_secs(self.check_interval_secs)
    }
}
