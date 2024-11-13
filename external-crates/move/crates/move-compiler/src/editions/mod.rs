// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module controls feature gating and breaking changes in new editions of the source language

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
    str::FromStr,
};

use crate::{
    diag,
    diagnostics::{Diagnostic, DiagnosticReporter},
    shared::string_utils::format_oxford_list,
};
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
    MacroFuns,
    Move2024Migration,
    SyntaxMethods,
    AutoborrowEq,
    CleverAssertions,
    NoParensCast,
    TypeHoles,
    Lambda,
    ModuleLabel,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, PartialOrd, Ord, Default)]
pub enum Flavor {
    #[default]
    Core,
    Sui,
}

pub const UPGRADE_NOTE: &str =
    "You can update the edition in the 'Move.toml', or via command line flag if invoking the \
    compiler directly.";

//**************************************************************************************************
// Entry
//**************************************************************************************************

/// Returns true if the feature is present in the given edition.
/// Adds an error to the environment.
pub fn check_feature_or_error(
    reporter: &DiagnosticReporter,
    edition: Edition,
    feature: FeatureGate,
    loc: Loc,
) -> bool {
    if !edition.supports(feature) {
        reporter.add_diag(create_feature_error(edition, feature, loc));
        false
    } else {
        true
    }
}

pub fn feature_edition_error_msg(edition: Edition, feature: FeatureGate) -> Option<String> {
    let supports_feature = edition.supports(feature);
    if !supports_feature {
        let valid_editions = valid_editions_for_feature(feature);
        let message =
            if valid_editions.is_empty() && Edition::DEVELOPMENT.features().contains(&feature) {
                format!(
                    "{} under development and should not be used right now.",
                    feature.error_prefix()
                )
            } else {
                valid_editions.last().map_or(
                    format!(
                        "{} not supported by any current edition '{edition}', \
                         the feature is still in development",
                        feature.error_prefix()
                    ),
                    |supporting_edition| {
                        format!(
                            "{} not supported by current edition '{edition}'; \
                             the '{supporting_edition}' edition supports this feature",
                            feature.error_prefix(),
                        )
                    },
                )
            };
        Some(message)
    } else {
        None
    }
}

pub fn create_feature_error(edition: Edition, feature: FeatureGate, loc: Loc) -> Diagnostic {
    assert!(!edition.supports(feature));
    let Some(message) = feature_edition_error_msg(edition, feature) else {
        panic!("Previous assert should have failed");
    };
    let mut diag = diag!(Editions::FeatureTooNew, (loc, message));
    diag.add_note(UPGRADE_NOTE);
    diag
}

pub fn valid_editions_for_feature(feature: FeatureGate) -> Vec<Edition> {
    Edition::VALID
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

const E2024_ALPHA_FEATURES: &[FeatureGate] = &[];

const E2024_BETA_FEATURES: &[FeatureGate] = &[];

const DEVELOPMENT_FEATURES: &[FeatureGate] = &[];

const E2024_MIGRATION_FEATURES: &[FeatureGate] = &[FeatureGate::Move2024Migration];

const E2024_FEATURES: &[FeatureGate] = &[
    FeatureGate::NestedUse,
    FeatureGate::PublicPackage,
    FeatureGate::PostFixAbilities,
    FeatureGate::StructTypeVisibility,
    FeatureGate::DotCall,
    FeatureGate::PositionalFields,
    FeatureGate::LetMut,
    FeatureGate::Move2024Keywords,
    FeatureGate::BlockLabels,
    FeatureGate::Move2024Paths,
    FeatureGate::Move2024Optimizations,
    FeatureGate::SyntaxMethods,
    FeatureGate::AutoborrowEq,
    FeatureGate::NoParensCast,
    FeatureGate::MacroFuns,
    FeatureGate::TypeHoles,
    FeatureGate::CleverAssertions,
    FeatureGate::Lambda,
    FeatureGate::ModuleLabel,
    FeatureGate::Enums,
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
    pub const E2024_BETA: Self = Self {
        edition: symbol!("2024"),
        release: Some(symbol!("beta")),
    };
    pub const E2024_MIGRATION: Self = Self {
        edition: symbol!("2024"),
        release: Some(symbol!("migration")),
    };
    pub const DEVELOPMENT: Self = Self {
        edition: symbol!("development"),
        release: None,
    };
    pub const E2024: Self = Self {
        edition: symbol!("2024"),
        release: None,
    };

    const SEP: &'static str = ".";

    pub const ALL: &'static [Self] = &[
        Self::LEGACY,
        Self::E2024_ALPHA,
        Self::E2024_BETA,
        Self::E2024_MIGRATION,
        Self::DEVELOPMENT,
        Self::E2024,
    ];
    // NB: This is the list of editions that are considered "valid" for the purposes of the Move.
    // This list should be kept in order from oldest edition to newest.
    pub const VALID: &'static [Self] = &[
        Self::LEGACY,
        Self::E2024_ALPHA,
        Self::E2024_BETA,
        Self::E2024,
    ];

    pub fn supports(&self, feature: FeatureGate) -> bool {
        SUPPORTED_FEATURES.get(self).unwrap().contains(&feature)
    }

    // Intended only for implementing the lazy static (supported feature map) above
    fn prev(&self) -> Option<Self> {
        match *self {
            Self::LEGACY => None,
            Self::E2024_ALPHA => Some(Self::E2024_BETA),
            Self::E2024_BETA => Some(Self::E2024),
            Self::E2024 => Some(Self::LEGACY),
            Self::E2024_MIGRATION => Some(Self::E2024),
            Self::DEVELOPMENT => Some(Self::E2024_ALPHA),
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
            Self::E2024_BETA => {
                let mut features = self.prev().unwrap().features();
                features.extend(E2024_BETA_FEATURES);
                features
            }
            Self::E2024 => {
                let mut features = self.prev().unwrap().features();
                features.extend(E2024_FEATURES);
                features
            }
            Self::E2024_MIGRATION => {
                let mut features = self.prev().unwrap().features();
                features.extend(E2024_MIGRATION_FEATURES);
                features
            }
            Self::DEVELOPMENT => {
                let mut features = self.prev().unwrap().features();
                features.extend(DEVELOPMENT_FEATURES);
                features
            }
            _ => self.unknown_edition_panic(),
        }
    }

    fn unknown_edition_panic(&self) -> ! {
        panic!("{}", self.unknown_edition_error())
    }

    pub fn unknown_edition_error(&self) -> anyhow::Error {
        anyhow::anyhow!(
            "Unsupported edition \"{self}\". Current supported editions include: {}",
            format_oxford_list!("and", "\"{}\"", Self::VALID)
        )
    }
}

impl Flavor {
    pub const CORE: &'static str = "core";
    pub const SUI: &'static str = "sui";
    pub const ALL: &'static [Self] = &[Self::Core, Self::Sui];
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
            FeatureGate::MacroFuns => "'macro' functions are",
            FeatureGate::Move2024Migration => "Move 2024 migration is",
            FeatureGate::SyntaxMethods => "'syntax' methods are",
            FeatureGate::AutoborrowEq => "Automatic borrowing is",
            FeatureGate::CleverAssertions => "Clever `assert!`, `abort`, and `#[error]` are",
            FeatureGate::NoParensCast => "'as' without parentheses is",
            FeatureGate::TypeHoles => "'_' placeholders for type inference are",
            FeatureGate::Lambda => "lambda expressions are",
            FeatureGate::ModuleLabel => "'module' label forms (ending with ';') are",
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
        if !Self::VALID.iter().any(|e| e == &edition) && edition != Edition::DEVELOPMENT {
            return Err(edition.unknown_edition_error());
        }
        Ok(edition)
    }
}

impl FromStr for Flavor {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            Self::CORE => Self::Core,
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
            Flavor::Core => write!(f, "{}", Self::CORE),
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
        Edition::E2024
    }
}
