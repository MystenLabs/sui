// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt::{self, Display},
    str::FromStr,
};

use anyhow::bail;
use move_command_line_common::parser::Token;
use move_core_types::identifier;

#[derive(Eq, PartialEq, Debug, Clone, Copy, Hash)]
pub enum CommandToken {
    TransferObjects,
    SplitCoins,
    MergeCoins,
    MakeMoveVec,
    MoveCall,
    Publish,
    Upgrade,
    Assign,
    File,
    WarnShadows,
    Preview,
    PickGasBudget,
    GasBudget,
    FileStart,
    FileEnd,
}

pub const TRANSFER_OBJECTS: &str = "transfer_objects";
pub const SPLIT_COINS: &str = "split_coins";
pub const MERGE_COINS: &str = "merge_coins";
pub const MAKE_MOVE_VEC: &str = "make_move_vec";
pub const MOVE_CALL: &str = "move_call";
pub const PUBLISH: &str = "publish";
pub const UPGRADE: &str = "upgrade";
pub const ASSIGN: &str = "assign";
pub const FILE: &str = "file";
pub const PREVIEW: &str = "preview";
pub const WARN_SHADOWS: &str = "warn_shadows";
pub const PICK_GAS_BUDGET: &str = "pick_gas_budget";
pub const GAS_BUDGET: &str = "gas_budget";
pub const FILE_START: &str = "file-include-start";
pub const FILE_END: &str = "file-include-end";

impl Display for CommandToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            CommandToken::TransferObjects => TRANSFER_OBJECTS,
            CommandToken::SplitCoins => SPLIT_COINS,
            CommandToken::MergeCoins => MERGE_COINS,
            CommandToken::MakeMoveVec => MAKE_MOVE_VEC,
            CommandToken::MoveCall => MOVE_CALL,
            CommandToken::Publish => PUBLISH,
            CommandToken::Upgrade => UPGRADE,
            CommandToken::Assign => ASSIGN,
            CommandToken::File => FILE,
            CommandToken::Preview => PREVIEW,
            CommandToken::WarnShadows => WARN_SHADOWS,
            CommandToken::PickGasBudget => PICK_GAS_BUDGET,
            CommandToken::GasBudget => GAS_BUDGET,
            CommandToken::FileStart => FILE_START,
            CommandToken::FileEnd => FILE_END,
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
            MOVE_CALL => Ok(CommandToken::MoveCall),
            PUBLISH => Ok(CommandToken::Publish),
            UPGRADE => Ok(CommandToken::Upgrade),
            ASSIGN => Ok(CommandToken::Assign),
            FILE => Ok(CommandToken::File),
            PREVIEW => Ok(CommandToken::Preview),
            WARN_SHADOWS => Ok(CommandToken::WarnShadows),
            PICK_GAS_BUDGET => Ok(CommandToken::PickGasBudget),
            GAS_BUDGET => Ok(CommandToken::GasBudget),
            FILE_START => Ok(CommandToken::FileStart),
            FILE_END => Ok(CommandToken::FileEnd),
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
            CommandToken::MoveCall => CommandArity {
                types: Arity::Range(0, 1),
                args: Arity::Variable,
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
            CommandToken::GasBudget => CommandArity {
                types: Arity::Exact(0),
                args: Arity::Exact(1),
            },
            CommandToken::FileStart => CommandArity {
                types: Arity::Exact(0),
                args: Arity::Exact(1),
            },
            CommandToken::FileEnd => CommandArity {
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
            MOVE_CALL,
            PUBLISH,
            UPGRADE,
            ASSIGN,
            FILE,
            PREVIEW,
            WARN_SHADOWS,
            PICK_GAS_BUDGET,
            GAS_BUDGET,
            FILE_START,
            FILE_END,
        ];

        for s in &command_strs {
            let token = CommandToken::from_str(s).unwrap();
            assert_eq!(token.to_string(), *s);
        }
    }
}
