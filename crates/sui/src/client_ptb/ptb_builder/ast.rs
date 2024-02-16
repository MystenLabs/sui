// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_command_line_common::{address::ParsedAddress, types::ParsedType};
use sui_types::Identifier;

use super::{argument::Argument, errors::Spanned};

/// A PTB Program consisting of a list of commands and a flag indicating if the preview
/// warn-shadows command was present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub commands: Vec<Spanned<ParsedPTBCommand>>,
    pub preview_set: bool,
    pub warn_shadows_set: bool,
    pub summary_set: bool,
}

/// Types of gas pickers that can be used to pick a gas budget from a list of gas budgets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GasPicker {
    Max,
    Sum,
}

#[derive(Eq, PartialEq, Debug, Clone)]
pub struct ModuleAccess {
    pub address: Spanned<ParsedAddress>,
    pub module_name: Spanned<Identifier>,
    pub function_name: Spanned<Identifier>,
}

/// A parsed PTB command consisting of the command and the parsed arguments to the command.
#[derive(Eq, PartialEq, Debug, Clone)]
pub enum ParsedPTBCommand {
    TransferObjects(Spanned<Argument>, Spanned<Vec<Spanned<Argument>>>),
    SplitCoins(Spanned<Argument>, Spanned<Vec<Spanned<Argument>>>),
    MergeCoins(Spanned<Argument>, Spanned<Vec<Spanned<Argument>>>),
    MakeMoveVec(Spanned<ParsedType>, Spanned<Vec<Spanned<Argument>>>),
    MoveCall(
        Spanned<ModuleAccess>,
        Option<Spanned<Vec<ParsedType>>>,
        Vec<Spanned<Argument>>,
    ),
    Assign(Spanned<String>, Option<Spanned<Argument>>),
    Publish(Spanned<String>),
    Upgrade(Spanned<String>, Spanned<Argument>),
    WarnShadows(Spanned<Argument>),
    Preview(Spanned<Argument>),
    PickGasBudget(Spanned<GasPicker>),
    GasBudget(Spanned<u64>),
}
