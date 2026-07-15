// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;

use sui_futures::stream::ConcurrencyConfig as RuntimeConcurrencyConfig;

/// Serde-friendly concurrency configuration used by ingestion, processing, and committing.
///
/// A positive integer such as `10` configures fixed concurrency while preserving the original
/// configuration format. The tagged forms `{ kind = "fixed", value = 10 }` and
/// `{ kind = "adaptive", initial = 5, min = 1, max = 20 }` are also accepted.
///
/// Adaptive mode accepts optional tuning overrides:
/// - `dead_band`: `[low, high]` fill fraction thresholds (default: [0.6, 0.85])
#[derive(Debug, Clone, PartialEq)]
pub enum ConcurrencyConfig {
    Fixed {
        value: usize,
    },
    Adaptive {
        initial: usize,
        min: usize,
        max: usize,
        dead_band: Option<[f64; 2]>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum TaggedConcurrencyConfig {
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

#[derive(Deserialize)]
#[serde(untagged)]
enum ConcurrencyConfigRepresentation {
    Fixed(usize),
    Tagged(TaggedConcurrencyConfig),
}

impl From<ConcurrencyConfigRepresentation> for ConcurrencyConfig {
    fn from(config: ConcurrencyConfigRepresentation) -> Self {
        match config {
            ConcurrencyConfigRepresentation::Fixed(value) => Self::fixed(value),
            ConcurrencyConfigRepresentation::Tagged(config) => config.into(),
        }
    }
}

impl From<TaggedConcurrencyConfig> for ConcurrencyConfig {
    fn from(config: TaggedConcurrencyConfig) -> Self {
        match config {
            TaggedConcurrencyConfig::Fixed { value } => Self::Fixed { value },
            TaggedConcurrencyConfig::Adaptive {
                initial,
                min,
                max,
                dead_band,
            } => Self::Adaptive {
                initial,
                min,
                max,
                dead_band,
            },
        }
    }
}

impl Serialize for ConcurrencyConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Fixed { value } => value.serialize(serializer),
            Self::Adaptive {
                initial,
                min,
                max,
                dead_band,
            } => TaggedConcurrencyConfig::Adaptive {
                initial: *initial,
                min: *min,
                max: *max,
                dead_band: *dead_band,
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ConcurrencyConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(ConcurrencyConfigRepresentation::deserialize(deserializer)?.into())
    }
}

impl ConcurrencyConfig {
    pub const fn fixed(value: usize) -> Self {
        Self::Fixed { value }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_concurrency_deserializes_from_integer() {
        assert_eq!(
            serde_json::from_str::<ConcurrencyConfig>("10").unwrap(),
            ConcurrencyConfig::fixed(10)
        );
    }

    #[test]
    fn fixed_concurrency_deserializes_from_tagged_config() {
        assert_eq!(
            serde_json::from_str::<ConcurrencyConfig>(r#"{"kind":"fixed","value":10}"#).unwrap(),
            ConcurrencyConfig::fixed(10)
        );
    }

    #[test]
    fn adaptive_concurrency_deserializes_from_tagged_config() {
        assert_eq!(
            serde_json::from_str::<ConcurrencyConfig>(
                r#"{"kind":"adaptive","initial":5,"min":1,"max":20}"#
            )
            .unwrap(),
            ConcurrencyConfig::Adaptive {
                initial: 5,
                min: 1,
                max: 20,
                dead_band: None,
            }
        );
    }

    #[test]
    fn fixed_concurrency_serializes_as_integer() {
        assert_eq!(
            serde_json::to_string(&ConcurrencyConfig::fixed(10)).unwrap(),
            "10"
        );
    }
}
