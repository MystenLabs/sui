// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module controls feature gating and breaking changes in new editions of the source language

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
    str::FromStr,
};

use crate::{diag, diagnostics::Diagnostic, shared::CompilationEnv};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

//**************************************************************************************************
// types
//**************************************************************************************************

#[derive(PartialEq, Eq, Clone, Copy, Debug, PartialOrd, Ord)]
pub struct Edition {
    pub edition: Symbol,
    pub release: Option<Symbol>,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, PartialOrd, Ord)]
pub enum FeatureGate {
    NestedUse,
    PublicPackage,
    PostFixAbilities,
    StructTypeVisibility,
    Enums,
    DotCall,
    PositionalFields,
    LetMut,
    Move2024Optimizations,
    Move2024Keywords,
    BlockLabels,
    Move2024Paths,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, PartialOrd, Ord, Default)]
pub enum Flavor {
    #[default]
    GlobalStorage,
    Sui,
}

pub const UPGRADE_NOTE: &str = "You can update the edition in the 'Move.toml', \
            or via command line flag if invoking the compiler directly.";

//**************************************************************************************************
// Entry
//**************************************************************************************************

/// Returns true if the feature is present in the given edition.
/// Adds an error to the environment.
pub fn check_feature_or_error(
    env: &mut CompilationEnv,
    edition: Edition,
    feature: FeatureGate,
    loc: Loc,
) -> bool {
    if let Some(msg) = feature_edition_error_msg(edition, feature) {
        let mut diag = diag!(Editions::FeatureTooNew, (loc, msg));
        diag.add_note(UPGRADE_NOTE);
        env.add_diag(diag);
        false
    } else {
        true
    }
}

pub fn feature_edition_error_msg(edition: Edition, feature: FeatureGate) -> Option<String> {
    let supports_feature = edition.supports(feature);
    if !supports_feature {
        let valid_editions = valid_editions_for_feature(feature)
            .into_iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!(
            "{} not supported by current edition '{edition}', \
                    only '{valid_editions}' support this feature",
            feature.error_prefix(),
        ))
    } else {
        None
    }
}

pub fn create_feature_error(edition: Edition, feature: FeatureGate, loc: Loc) -> Diagnostic {
    assert!(!edition.supports(feature));
    let valid_editions = valid_editions_for_feature(feature)
        .into_iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let mut diag = diag!(
        Editions::FeatureTooNew,
        (
            loc,
            format!(
                "{} not supported by current edition '{edition}', \
                only '{valid_editions}' support this feature",
                feature.error_prefix(),
            )
        )
    );
    diag.add_note(
        "You can update the edition in the 'Move.toml', \
        or via command line flag if invoking the compiler directly.",
    );
    diag
}

pub fn valid_editions_for_feature(feature: FeatureGate) -> Vec<Edition> {
    Edition::ALL
        .iter()
        .filter(|e| e.supports(feature))
        .copied()
        .collect()
}

//**************************************************************************************************
// impls
//**************************************************************************************************

static SUPPORTED_FEATURES: Lazy<BTreeMap<Edition, BTreeSet<FeatureGate>>> =
    Lazy::new(|| BTreeMap::from_iter(Edition::ALL.iter().map(|e| (*e, e.features()))));

const E2024_ALPHA_FEATURES: &[FeatureGate] = &[
    FeatureGate::NestedUse,
    FeatureGate::PublicPackage,
    FeatureGate::PostFixAbilities,
    FeatureGate::StructTypeVisibility,
    FeatureGate::Enums,
    FeatureGate::DotCall,
    FeatureGate::PositionalFields,
    FeatureGate::LetMut,
    FeatureGate::Move2024Keywords,
    FeatureGate::BlockLabels,
    FeatureGate::Move2024Paths,
];

impl Edition {
    pub const LEGACY: Self = Self {
        edition: symbol!("legacy"),
        release: None,
    };
    pub const E2024_ALPHA: Self = Self {
        edition: symbol!("2024"),
        release: Some(symbol!("alpha")),
    };

    const SEP: &'static str = ".";

    pub const ALL: &'static [Self] = &[Self::LEGACY, Self::E2024_ALPHA];

    pub fn supports(&self, feature: FeatureGate) -> bool {
        SUPPORTED_FEATURES.get(self).unwrap().contains(&feature)
    }

    // Intended only for implementing the lazy static (supported feature map) above
    fn prev(&self) -> Option<Self> {
        match *self {
            Self::LEGACY => None,
            Self::E2024_ALPHA => Some(Self::LEGACY),
            _ => self.unknown_edition_panic(),
        }
    }

    // Inefficient and should be called only to implement the lazy static
    // (supported feature map) above
    fn features(&self) -> BTreeSet<FeatureGate> {
        match *self {
            Self::LEGACY => BTreeSet::new(),
            Self::E2024_ALPHA => {
                let mut features = self.prev().unwrap().features();
                features.extend(E2024_ALPHA_FEATURES);
                features
            }
            _ => self.unknown_edition_panic(),
        }
    }

    fn unknown_edition_panic(&self) -> ! {
        panic!("{}", self.unknown_edition_error())
    }

    fn unknown_edition_error(&self) -> anyhow::Error {
        anyhow::anyhow!(
            "Unsupported edition \"{self}\". Current supported editions include: {}",
            Self::ALL
                .iter()
                .map(|e| format!("\"{}\"", e))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl Flavor {
    pub const GLOBAL_STORAGE: &'static str = "global-storage";
    pub const SUI: &'static str = "sui";
    pub const ALL: &'static [Self] = &[Self::GlobalStorage, Self::Sui];
}

impl FeatureGate {
    fn error_prefix(&self) -> &'static str {
        match self {
            FeatureGate::NestedUse => "Nested 'use' forms are",
            FeatureGate::PublicPackage => "'public(package)' is",
            FeatureGate::PostFixAbilities => "Postfix abilities are",
            FeatureGate::StructTypeVisibility => "Struct visibility modifiers are",
            FeatureGate::Enums => "Enums are",
            FeatureGate::DotCall => "Method syntax is",
            FeatureGate::PositionalFields => "Positional fields are",
            FeatureGate::LetMut => "'mut' variable modifiers are",
            FeatureGate::Move2024Optimizations => "Move 2024 optimizations are",
            FeatureGate::Move2024Keywords => "Move 2024 keywords are",
            FeatureGate::BlockLabels => "Block labels are",
            FeatureGate::Move2024Paths => "Move 2024 paths are",
        }
    }
}

//**************************************************************************************************
// Parsing/Deserialize
//**************************************************************************************************

impl FromStr for Edition {
    type Err = anyhow::Error;

    // Required method
    fn from_str(s: &str) -> anyhow::Result<Self> {
        let (edition, release) = if let Some((edition, release)) = s.split_once(Edition::SEP) {
            (edition, Some(release))
        } else {
            (s, None)
        };
        let edition = Edition {
            edition: Symbol::from(edition),
            release: release.map(Symbol::from),
        };
        if !Self::ALL.iter().any(|e| e == &edition) {
            return Err(edition.unknown_edition_error());
        }
        Ok(edition)
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
        match &self.release {
            None => write!(f, "{}", self.edition),
            Some(release) => write!(f, "{}{}{}", self.edition, Self::SEP, release),
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
        Edition::LEGACY
    }
}
