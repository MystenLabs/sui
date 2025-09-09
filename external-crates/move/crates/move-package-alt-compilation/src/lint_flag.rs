// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_compiler::linters::LintLevel;
use serde::{Deserialize, Serialize};

#[derive(
    Parser, Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, PartialOrd, Default,
)]
pub struct LintFlag {
    /// If `true`, disable linters
    #[clap(
        name = "no-lint",
        long = "no-lint",
        global = true,
        group = "lint-level"
    )]
    no_lint: bool,

    /// If `true`, enables extra linters
    #[clap(name = "lint", long = "lint", global = true, group = "lint-level")]
    lint: bool,
}

impl LintFlag {
    pub const LEVEL_NONE: Self = Self {
        no_lint: true,
        lint: false,
    };
    pub const LEVEL_DEFAULT: Self = Self {
        no_lint: false,
        lint: false,
    };
    pub const LEVEL_ALL: Self = Self {
        no_lint: false,
        lint: true,
    };

    pub fn get(self) -> LintLevel {
        match self {
            Self::LEVEL_NONE => LintLevel::None,
            Self::LEVEL_DEFAULT => LintLevel::Default,
            Self::LEVEL_ALL => LintLevel::All,
            _ => unreachable!(),
        }
    }

    pub fn set(&mut self, level: LintLevel) {
        *self = level.into();
    }
}

impl From<LintLevel> for LintFlag {
    fn from(level: LintLevel) -> Self {
        match level {
            LintLevel::None => Self::LEVEL_NONE,
            LintLevel::Default => Self::LEVEL_DEFAULT,
            LintLevel::All => Self::LEVEL_ALL,
        }
    }
}
