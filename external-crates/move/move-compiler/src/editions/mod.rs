// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module controls feature gating and breaking changes in new editions of the source language

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
    str::FromStr,
};

use crate::{diag, shared::CompilationEnv};
use move_ir_types::location::*;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

//**************************************************************************************************
// types
//**************************************************************************************************

#[derive(PartialEq, Eq, Clone, Copy, Debug, PartialOrd, Ord)]
pub enum Edition {
    Legacy,
    E2024(EditionRelease),
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, PartialOrd, Ord)]
pub enum EditionRelease {
    Final,
    Beta,
    Alpha,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, PartialOrd, Ord)]
pub enum FeatureGate {}

#[derive(PartialEq, Eq, Clone, Copy, Debug, PartialOrd, Ord)]
pub enum Flavor {
    GlobalStorage,
    Sui,
}

//**************************************************************************************************
// Entry
//**************************************************************************************************

pub fn check_feature(env: &mut CompilationEnv, edition: Edition, loc: Loc, feature: FeatureGate) {
    let is_supported = SUPPORTED_FEATURES.get(&edition).unwrap().contains(&feature);
    if !is_supported {
        env.add_diag(diag!(
            Editions::FeatureTooNew,
            (
                loc,
                format!("{feature} requires edition {edition} or newer")
            )
        ))
    }
}

//**************************************************************************************************
// impls
//**************************************************************************************************

static SUPPORTED_FEATURES: Lazy<BTreeMap<Edition, BTreeSet<FeatureGate>>> =
    Lazy::new(|| BTreeMap::from_iter(Edition::ALL.iter().map(|e| (*e, e.features()))));

impl Edition {
    pub const LEGACY: &str = "legacy";
    pub const E2024_PREFIX: &str = "2024";

    pub const ALL: &[Self] = &[
        Self::Legacy,
        Self::E2024(EditionRelease::Alpha),
        Self::E2024(EditionRelease::Beta),
        Self::E2024(EditionRelease::Final),
    ];
    pub const SUPPORTED: &[Self] = &[Self::Legacy, Self::E2024(EditionRelease::Alpha)];

    // Intended only for implementing the lazy static (supported feature map) above
    fn prev(&self) -> Option<Self> {
        match self {
            Self::Legacy => None,
            Self::E2024(EditionRelease::Alpha) => Some(Self::Legacy),
            Self::E2024(EditionRelease::Beta) => Some(Self::E2024(EditionRelease::Alpha)),
            Self::E2024(EditionRelease::Final) => Some(Self::E2024(EditionRelease::Beta)),
        }
    }

    // Inefficient and should be called only to implement the lazy static
    // (supported feature map) above
    fn features(&self) -> BTreeSet<FeatureGate> {
        match self {
            Edition::Legacy => BTreeSet::new(),
            Edition::E2024(EditionRelease::Alpha) => self.prev().unwrap().features(),
            Edition::E2024(EditionRelease::Beta) => self.prev().unwrap().features(),
            Edition::E2024(EditionRelease::Final) => self.prev().unwrap().features(),
        }
    }
}

impl EditionRelease {
    pub const ALPHA: &str = "alpha";
    pub const BETA: &str = "beta";
    pub const EXT_SEP: &str = ".";

    pub const SUFFIXES: &[Self] = &[Self::Alpha, Self::Beta];
}

impl Flavor {
    pub const GLOBAL_STORAGE: &str = "global-storage";
    pub const SUI: &str = "sui";
    pub const ALL: &[Self] = &[Self::GlobalStorage, Self::Sui];
}

//**************************************************************************************************
// Parsing/Deserialize
//**************************************************************************************************

impl FromStr for Edition {
    type Err = anyhow::Error;

    // Required method
    fn from_str(s: &str) -> anyhow::Result<Self> {
        let edition = if s == Edition::LEGACY {
            Edition::Legacy
        } else if let Some(s) = s.strip_prefix(Edition::E2024_PREFIX) {
            let release = EditionRelease::from_str(s)?;
            let edition = Edition::E2024(release);
            edition
        } else {
            anyhow::bail!(
                "Unknown edition \"{s}\". Expected one of: {}",
                Self::SUPPORTED
                    .iter()
                    .map(|e| format!("\"{}\"", e))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        if !Self::SUPPORTED.iter().any(|e| *e == edition) {
            anyhow::bail!(
                "Unsupported edition \"{s}\". Current supported editions include: {}",
                Self::SUPPORTED
                    .iter()
                    .map(|e| format!("\"{}\"", e))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        Ok(edition)
    }
}

impl FromStr for EditionRelease {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        Ok(if let Some(s) = s.strip_prefix(EditionRelease::EXT_SEP) {
            match s {
                Self::ALPHA => Self::Alpha,
                Self::BETA => Self::Beta,
                _ => anyhow::bail!(
                    "Unknown release suffix \"{}{s}\". Expected no suffix, or one of: {}",
                    Self::EXT_SEP,
                    Self::SUFFIXES
                        .iter()
                        .map(|e| format!("\"{}\"", e))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }
        } else {
            if !s.is_empty() {
                anyhow::bail!(
                    "Unknown release suffix \"{s}\". Expected no suffix, or one of: {}",
                    Self::SUFFIXES
                        .iter()
                        .map(|e| format!("\"{}\"", e))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Self::Final
        })
    }
}

impl FromStr for Flavor {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            Self::GLOBAL_STORAGE => Self::GlobalStorage,
            Self::SUI => Self::Sui,
            _ => anyhow::bail!(
                "Unknown flavor \"{s}\". Expected one of: {}",
                Self::ALL
                    .iter()
                    .map(|e| format!("\"{}\"", e))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        })
    }
}

impl<'de> Deserialize<'de> for Edition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Edition::from_str(&String::deserialize(deserializer)?)
            .map_err(|e| serde::de::Error::custom(format!("{e}")))
    }
}

impl<'de> Deserialize<'de> for Flavor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Flavor::from_str(&String::deserialize(deserializer)?)
            .map_err(|e| serde::de::Error::custom(format!("{e}")))
    }
}

//**************************************************************************************************
// Display/Serialize
//**************************************************************************************************

impl Display for Edition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Edition::Legacy => write!(f, "{}", Self::LEGACY),
            Edition::E2024(release) => write!(f, "{}{release}", Self::E2024_PREFIX),
        }
    }
}

impl Display for EditionRelease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditionRelease::Final => write!(f, ""),
            EditionRelease::Alpha => write!(f, ".{}", Self::ALPHA),
            EditionRelease::Beta => write!(f, ".{}", Self::BETA),
        }
    }
}

impl Display for Flavor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Flavor::GlobalStorage => write!(f, "{}", Self::GLOBAL_STORAGE),
            Flavor::Sui => write!(f, "{}", Self::SUI),
        }
    }
}

impl Serialize for Edition {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&format!("{}", self))
    }
}

impl Serialize for Flavor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&format!("{}", self))
    }
}

impl Display for FeatureGate {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

//**************************************************************************************************
// traits
//**************************************************************************************************

impl Default for Edition {
    fn default() -> Self {
        Edition::Legacy
    }
}

impl Default for Flavor {
    fn default() -> Self {
        Flavor::GlobalStorage
    }
}
