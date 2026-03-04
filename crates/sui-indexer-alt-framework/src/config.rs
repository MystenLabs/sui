// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;
use serde::Serialize;

use sui_futures::stream::ConcurrencyConfig as RuntimeConcurrencyConfig;

/// Serde-friendly concurrency configuration used by both the ingestion and processor stages.
///
/// Use `{ kind = "fixed", value = 10 }` for constant concurrency, or
/// `{ kind = "adaptive", initial = 5, min = 1, max = 20 }` for adaptive concurrency that adjusts
/// based on downstream channel fill fraction.
///
/// Adaptive mode accepts optional tuning overrides:
/// - `dead_band`: `[low, high]` fill fraction thresholds (default: [0.6, 0.85])
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ConcurrencyConfig {
    Fixed {
        value: usize,
    },
    Adaptive {
        initial: usize,
        min: usize,
        max: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dead_band: Option<[f64; 2]>,
    },
}

impl ConcurrencyConfig {
    pub fn initial(&self) -> usize {
        match self {
            Self::Fixed { value } => *value,
            Self::Adaptive { initial, .. } => *initial,
        }
    }

    pub fn min(&self) -> usize {
        let v = match self {
            Self::Fixed { value } => *value,
            Self::Adaptive { min, .. } => *min,
        };
        assert!(v >= 1, "min concurrency must be >= 1");
        v
    }

    pub fn max(&self) -> usize {
        match self {
            Self::Fixed { value } => *value,
            Self::Adaptive { max, .. } => *max,
        }
    }

    pub fn is_adaptive(&self) -> bool {
        matches!(self, Self::Adaptive { .. })
    }
}

impl From<ConcurrencyConfig> for RuntimeConcurrencyConfig {
    fn from(config: ConcurrencyConfig) -> Self {
        match config {
            ConcurrencyConfig::Fixed { value } => Self::fixed(value),
            ConcurrencyConfig::Adaptive {
                initial,
                min,
                max,
                dead_band,
            } => {
                let mut c = Self::adaptive(initial, min, max);
                if let Some([low, high]) = dead_band {
                    c = c.with_dead_band(low, high);
                }
                c
            }
        }
    }
}
