// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_compiler::linters::LintSelection;
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

    pub fn get(self) -> LintSelection {
        match self {
            Self::LEVEL_NONE => LintSelection::None,
            Self::LEVEL_DEFAULT => LintSelection::Default,
            Self::LEVEL_ALL => LintSelection::All,
            _ => unreachable!(),
        }
    }

    pub fn set(&mut self, selection: LintSelection) {
        *self = selection.into();
    }
}

impl From<LintSelection> for LintFlag {
    fn from(selection: LintSelection) -> Self {
        match selection {
            LintSelection::None => Self::LEVEL_NONE,
            LintSelection::Default => Self::LEVEL_DEFAULT,
            LintSelection::All => Self::LEVEL_ALL,
        }
    }
}
