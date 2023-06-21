// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module controls feature gating and breaking changes in new editions of the source language

use std::{fmt::Display, str::FromStr};

use crate::{diag, shared::CompilationEnv};
use move_ir_types::location::*;
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

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum FeatureGate {
    LetMut,
    ReceiverSyntax,
    MacroFun,
    UnderscoreType,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, PartialOrd, Ord)]
pub enum Flavor {
    GlobalStorage,
    Sui,
}

//**************************************************************************************************
// Entry
//**************************************************************************************************

pub fn check_feature(env: &mut CompilationEnv, edition: Edition, loc: Loc, feature: FeatureGate) {
    let min_edition = feature.min_edition();
    if edition < min_edition {
        env.add_diag(diag!(
            Editions::FeatureTooNew,
            (loc, "{} requires edition {} or newer")
        ))
    }
}

//**************************************************************************************************
// impls
//**************************************************************************************************

impl Edition {
    pub const LEGACY: &str = "legacy";
    pub const E2024_PREFIX: &str = "2024";

    pub const SUPPORTED: &[Self] = &[Self::Legacy, Self::E2024(EditionRelease::Alpha)];
}

impl EditionRelease {
    pub const ALPHA: &str = "alpha";
    pub const BETA: &str = "beta";
    pub const EXT_SEP: &str = ".";

    pub const SUFFIXES: &[Self] = &[Self::Alpha, Self::Beta];
}

impl FeatureGate {
    pub fn min_edition(&self) -> Edition {
        match self {
            FeatureGate::LetMut => Edition::E2024(EditionRelease::Alpha),
            FeatureGate::ReceiverSyntax => Edition::E2024(EditionRelease::Alpha),
            FeatureGate::MacroFun => Edition::E2024(EditionRelease::Alpha),
            FeatureGate::UnderscoreType => Edition::E2024(EditionRelease::Alpha),
        }
    }
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
