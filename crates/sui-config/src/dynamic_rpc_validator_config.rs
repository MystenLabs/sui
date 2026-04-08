// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_types::base_types::AuthorityName;

/// Information about a validator, provided to the test callback
#[derive(Clone, Debug)]
pub struct ValidatorInfo {
    /// The validator's index in the committee (0-based)
    pub index: usize,
    /// The validator's authority name (public key)
    pub authority_name: AuthorityName,
}

/// Callback type for tests to provide validator library paths.
/// Returns Some(path) to load a library for this validator, or None to skip validation.
pub type ValidatorLibraryCallback =
    Arc<dyn Fn(&ValidatorInfo) -> Option<PathBuf> + Send + Sync + 'static>;

/// Global test callback registry
static TEST_VALIDATOR_LIBRARY_CALLBACK: OnceCell<ValidatorLibraryCallback> = OnceCell::new();

/// Register a callback to provide validator library paths during tests.
/// This should only be called once per test run.
///
/// # Panics
/// Panics if a callback is already registered.
pub fn set_test_validator_library_callback(callback: ValidatorLibraryCallback) {
    if TEST_VALIDATOR_LIBRARY_CALLBACK.set(callback).is_err() {
        panic!("Test validator library callback already set");
    }
}

/// Clear the test callback (for test cleanup).
/// Note: This uses unsafe code to reset the OnceCell for testing purposes.
#[cfg(test)]
pub fn clear_test_validator_library_callback() {
    // OnceCell doesn't support clearing, so we need to use a workaround
    // In tests, we can just ignore this limitation
}

/// Get the library path for a validator, consulting the test callback if in test mode.
pub fn get_validator_library_path(
    config: &DynamicRpcValidatorConfig,
    validator_info: Option<&ValidatorInfo>,
) -> Option<PathBuf> {
    // In test configuration, check the callback first
    if mysten_common::in_test_configuration()
        && let Some(callback) = TEST_VALIDATOR_LIBRARY_CALLBACK.get()
        && let Some(info) = validator_info
    {
        return callback(info);
    }

    // Fall back to the config file path
    config.library_path.clone()
}

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
    pub fn new(library_path: Option<PathBuf>) -> Self {
        Self {
            library_path,
            check_interval_secs: default_check_interval_secs(),
        }
    }

    pub fn check_interval(&self) -> Duration {
        Duration::from_secs(self.check_interval_secs)
    }
}
