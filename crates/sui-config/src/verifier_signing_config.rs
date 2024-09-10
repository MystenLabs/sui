// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_vm_config::verifier::MeterConfig;
use serde::{Deserialize, Serialize};

// Default values for verifier signing config.
pub const DEFAULT_MAX_PER_FUN_METER_UNITS: usize = 2_200_000;
pub const DEFAULT_MAX_PER_MOD_METER_UNITS: usize = 2_200_000;
pub const DEFAULT_MAX_PER_PKG_METER_UNITS: usize = 2_200_000;

pub const DEFAULT_MAX_BACK_EDGES_PER_FUNCTION: usize = 10_000;
pub const DEFAULT_MAX_BACK_EDGES_PER_MODULE: usize = 10_000;

/// This holds limits that are only set and used by the verifier during signing _only_. There are
/// additional limits in the `MeterConfig` and `VerifierConfig` that are used during both signing
/// and execution, however those limits cannot be set here and must be protocol versioned.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct VerifierSigningConfig {
    #[serde(default)]
    max_per_fun_meter_units: Option<usize>,
    #[serde(default)]
    max_per_mod_meter_units: Option<usize>,
    #[serde(default)]
    max_per_pkg_meter_units: Option<usize>,

    #[serde(default)]
    max_back_edges_per_function: Option<usize>,
    #[serde(default)]
    max_back_edges_per_module: Option<usize>,
}

impl VerifierSigningConfig {
    pub fn max_per_fun_meter_units(&self) -> usize {
        self.max_per_fun_meter_units
            .unwrap_or(DEFAULT_MAX_PER_FUN_METER_UNITS)
    }

    pub fn max_per_mod_meter_units(&self) -> usize {
        self.max_per_mod_meter_units
            .unwrap_or(DEFAULT_MAX_PER_MOD_METER_UNITS)
    }

    pub fn max_per_pkg_meter_units(&self) -> usize {
        self.max_per_pkg_meter_units
            .unwrap_or(DEFAULT_MAX_PER_PKG_METER_UNITS)
    }

    pub fn max_back_edges_per_function(&self) -> usize {
        self.max_back_edges_per_function
            .unwrap_or(DEFAULT_MAX_BACK_EDGES_PER_FUNCTION)
    }

    pub fn max_back_edges_per_module(&self) -> usize {
        self.max_back_edges_per_module
            .unwrap_or(DEFAULT_MAX_BACK_EDGES_PER_MODULE)
    }

    /// Return sign-time only limit for back edges for the verifier.
    pub fn limits_for_signing(&self) -> (usize, usize) {
        (
            self.max_back_edges_per_function(),
            self.max_back_edges_per_module(),
        )
    }

    /// MeterConfig for metering packages during signing. It is NOT stable between binaries and
    /// cannot used during execution.
    pub fn meter_config_for_signing(&self) -> MeterConfig {
        MeterConfig {
            max_per_fun_meter_units: Some(self.max_per_fun_meter_units() as u128),
            max_per_mod_meter_units: Some(self.max_per_mod_meter_units() as u128),
            max_per_pkg_meter_units: Some(self.max_per_pkg_meter_units() as u128),
        }
    }
}
