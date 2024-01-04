// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt::{self, Display},
    str::FromStr,
};

use anyhow::bail;
use move_command_line_common::parser::Token;
use move_core_types::identifier;

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum CommandToken {
    TransferObjects,
    SplitCoins,
    MergeCoins,
    MakeMoveVec,
    Publish,
    Upgrade,
    Assign,
    File,
    WarnShadows,
    Preview,
    PickGasBudget,
}

pub const TRANSFER_OBJECTS: &str = "transfer-objects";
pub const SPLIT_COINS: &str = "split-coins";
pub const MERGE_COINS: &str = "merge-coins";
pub const MAKE_MOVE_VEC: &str = "make-move-vec";
pub const PUBLISH: &str = "publish";
pub const UPGRADE: &str = "upgrade";
pub const ASSIGN: &str = "assign";
pub const FILE: &str = "file";
pub const PREVIEW: &str = "preview";
pub const WARN_SHADOWS: &str = "warn_shadows";
pub const PICK_GAS_BUDGET: &str = "pick-gas-budget";

impl Display for CommandToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            CommandToken::TransferObjects => TRANSFER_OBJECTS,
            CommandToken::SplitCoins => SPLIT_COINS,
            CommandToken::MergeCoins => MERGE_COINS,
            CommandToken::MakeMoveVec => MAKE_MOVE_VEC,
            CommandToken::Publish => PUBLISH,
            CommandToken::Upgrade => UPGRADE,
            CommandToken::Assign => ASSIGN,
            CommandToken::File => FILE,
            CommandToken::Preview => PREVIEW,
            CommandToken::WarnShadows => WARN_SHADOWS,
            CommandToken::PickGasBudget => PICK_GAS_BUDGET,
        };
        fmt::Display::fmt(s, f)
    }
}

impl FromStr for CommandToken {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            TRANSFER_OBJECTS => Ok(CommandToken::TransferObjects),
            SPLIT_COINS => Ok(CommandToken::SplitCoins),
            MERGE_COINS => Ok(CommandToken::MergeCoins),
            MAKE_MOVE_VEC => Ok(CommandToken::MakeMoveVec),
            PUBLISH => Ok(CommandToken::Publish),
            UPGRADE => Ok(CommandToken::Upgrade),
            ASSIGN => Ok(CommandToken::Assign),
            FILE => Ok(CommandToken::File),
            PREVIEW => Ok(CommandToken::Preview),
            WARN_SHADOWS => Ok(CommandToken::WarnShadows),
            PICK_GAS_BUDGET => Ok(CommandToken::PickGasBudget),
            _ => bail!("Invalid command token: {}", s),
        }
    }
}

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum Arity {
    Exact(usize),
    Range(usize, usize),
    Variable,
}

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub struct CommandArity {
    types: Arity,
    args: Arity,
}

impl CommandToken {
    pub fn arity(&self) -> CommandArity {
        match self {
            CommandToken::TransferObjects => CommandArity {
                types: Arity::Exact(0),
                args: Arity::Exact(2),
            },
            CommandToken::SplitCoins => CommandArity {
                types: Arity::Exact(0),
                args: Arity::Exact(2),
            },
            CommandToken::MergeCoins => CommandArity {
                types: Arity::Exact(0),
                args: Arity::Exact(2),
            },
            CommandToken::MakeMoveVec => CommandArity {
                types: Arity::Exact(1),
                args: Arity::Exact(1),
            },
            CommandToken::Publish => CommandArity {
                types: Arity::Exact(0),
                args: Arity::Exact(1),
            },
            CommandToken::Upgrade => CommandArity {
                types: Arity::Exact(0),
                args: Arity::Exact(1),
            },
            CommandToken::Assign => CommandArity {
                types: Arity::Exact(0),
                args: Arity::Range(1, 3),
            },
            CommandToken::File => CommandArity {
                types: Arity::Exact(0),
                args: Arity::Exact(1),
            },
            CommandToken::WarnShadows => CommandArity {
                types: Arity::Exact(0),
                args: Arity::Exact(0),
            },
            CommandToken::Preview => CommandArity {
                types: Arity::Exact(0),
                args: Arity::Exact(0),
            },
            CommandToken::PickGasBudget => CommandArity {
                types: Arity::Exact(0),
                args: Arity::Exact(1),
            },
        }
    }
}

mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let command_strs = vec![
            TRANSFER_OBJECTS,
            SPLIT_COINS,
            MERGE_COINS,
            MAKE_MOVE_VEC,
            PUBLISH,
            UPGRADE,
            ASSIGN,
            FILE,
            PREVIEW,
            WARN_SHADOWS,
            PICK_GAS_BUDGET,
        ];

        for s in &command_strs {
            let token = CommandToken::from_str(s).unwrap();
            assert_eq!(token.to_string(), *s);
        }
    }
}
