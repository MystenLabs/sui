// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;

use move_command_line_common::{
    address::ParsedAddress,
    types::{ParsedFqName, ParsedModuleId, ParsedStructType, ParsedType},
};
use sui_types::{base_types::ObjectID, Identifier};

use crate::sp;

use super::{argument::Argument, errors::Spanned};

pub type ParsedProgram = (Program, ProgramMetadata);

/// A PTB Program consisting of a list of commands and a flag indicating if the preview
/// warn-shadows command was present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub commands: Vec<Spanned<ParsedPTBCommand>>,
    // Held outside of metadata since this is used by the PTB builder
    pub warn_shadows_set: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramMetadata {
    pub preview_set: bool,
    pub summary_set: bool,
    pub gas_object_id: Option<Spanned<ObjectID>>,
    pub json_set: bool,
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
    PickGasBudget(Spanned<GasPicker>),
    GasBudget(Spanned<u64>),
    WarnShadows,
    Preview,
}

impl Display for GasPicker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use GasPicker::*;
        match self {
            Max => write!(f, "max"),
            Sum => write!(f, "sum"),
        }
    }
}

impl Display for ParsedPTBCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use super::token::*;
        match self {
            ParsedPTBCommand::TransferObjects(arg, args) => write!(
                f,
                "{TRANSFER_OBJECTS} {} [{}]",
                arg.value,
                args.value
                    .iter()
                    .map(|x| x.value.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
            ParsedPTBCommand::SplitCoins(arg, args) => write!(
                f,
                "{SPLIT_COINS} {} [{}]",
                arg.value,
                args.value
                    .iter()
                    .map(|x| x.value.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
            ParsedPTBCommand::MergeCoins(arg, args) => write!(
                f,
                "{MERGE_COINS} {} [{}]",
                arg.value,
                args.value
                    .iter()
                    .map(|x| x.value.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
            ParsedPTBCommand::Assign(arg, arg_opt) => write!(
                f,
                "{ASSIGN} {}{}",
                arg.value,
                if let Some(arg) = arg_opt {
                    format!(" {}", arg.value)
                } else {
                    "".to_string()
                }
            ),
            ParsedPTBCommand::Publish(s) => write!(f, "{PUBLISH} {}", s.value),
            ParsedPTBCommand::Upgrade(s, a) => write!(f, "{UPGRADE} {} {}", s.value, a.value),
            ParsedPTBCommand::WarnShadows => write!(f, "{WARN_SHADOWS}"),
            ParsedPTBCommand::Preview => write!(f, "{PREVIEW}"),
            ParsedPTBCommand::PickGasBudget(picker) => {
                write!(f, "{PICK_GAS_BUDGET} {}", picker.value)
            }
            ParsedPTBCommand::GasBudget(b) => write!(f, "{GAS_BUDGET} {}", b.value),
            ParsedPTBCommand::MakeMoveVec(ty, args) => write!(
                f,
                "{MAKE_MOVE_VEC} {} [{}]",
                TyDisplay(&ty.value),
                args.value
                    .iter()
                    .map(|x| x.value.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
            ParsedPTBCommand::MoveCall(
                sp!(
                    _,
                    ModuleAccess {
                        address,
                        module_name,
                        function_name
                    }
                ),
                tys,
                args,
            ) => {
                let address = match &address.value {
                    ParsedAddress::Named(n) => n.to_string(),
                    ParsedAddress::Numerical(n) => n.to_string(),
                };
                let type_args = match tys {
                    Some(tys) => format!(
                        "<{}>",
                        tys.value
                            .iter()
                            .map(|x| TyDisplay(x).to_string())
                            .collect::<Vec<String>>()
                            .join(", ")
                    ),
                    None => "".to_string(),
                };
                write!(
                    f,
                    "{MOVE_CALL} {}::{}::{}{} {}",
                    address,
                    module_name.value,
                    function_name.value,
                    type_args,
                    args.iter()
                        .map(|x| x.value.to_string())
                        .collect::<Vec<String>>()
                        .join(" ")
                )
            }
        }
    }
}

struct TyDisplay<'a>(&'a ParsedType);

impl<'a> Display for TyDisplay<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ParsedType::*;
        match self.0 {
            Address => write!(f, "address"),
            U8 => write!(f, "u8"),
            U64 => write!(f, "u64"),
            U128 => write!(f, "u128"),
            Bool => write!(f, "bool"),
            Vector(ty) => write!(f, "vector<{}>", TyDisplay(ty)),
            Struct(ParsedStructType {
                fq_name:
                    ParsedFqName {
                        module: ParsedModuleId { address, name },
                        name: struct_name,
                    },
                type_args,
            }) => {
                let address = match address {
                    ParsedAddress::Named(n) => n.to_string(),
                    ParsedAddress::Numerical(n) => n.to_string(),
                };
                let ty_str = if type_args.is_empty() {
                    "".to_string()
                } else {
                    format!(
                        "<{}>",
                        type_args
                            .iter()
                            .map(|x| TyDisplay(x).to_string())
                            .collect::<Vec<String>>()
                            .join(", ")
                    )
                };
                write!(f, "{}::{}::{}{}", address, name, struct_name, ty_str)
            }
            U16 => write!(f, "u16"),
            U32 => write!(f, "u32"),
            U256 => write!(f, "u256"),
            Signer => write!(f, "signer"),
        }
    }
}
