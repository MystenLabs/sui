// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use move_command_line_common::{
    address::{NumericalAddress, ParsedAddress},
    types::{ParsedFqName, ParsedModuleId, ParsedStructType, ParsedType},
};
use move_core_types::runtime_value::MoveValue;
use sui_types::{base_types::ObjectID, Identifier};

use crate::{error, sp};

use super::error::{PTBResult, Span, Spanned};

pub type ParsedProgram = (Program, ProgramMetadata);

// Commands
pub const TRANSFER_OBJECTS: &str = "transfer-objects";
pub const SPLIT_COINS: &str = "split-coins";
pub const MERGE_COINS: &str = "merge-coins";
pub const MAKE_MOVE_VEC: &str = "make-move-vec";
pub const MOVE_CALL: &str = "move-call";
pub const PUBLISH: &str = "publish";
pub const UPGRADE: &str = "upgrade";
pub const ASSIGN: &str = "assign";
pub const PREVIEW: &str = "preview";
pub const WARN_SHADOWS: &str = "warn-shadows";
pub const GAS_BUDGET: &str = "gas-budget";
pub const SUMMARY: &str = "summary";
pub const GAS_COIN: &str = "gas-coin";
pub const JSON: &str = "json";

// Types
pub const U8: &str = "u8";
pub const U16: &str = "u16";
pub const U32: &str = "u32";
pub const U64: &str = "u64";
pub const U128: &str = "u128";
pub const U256: &str = "u256";

// Keywords
pub const ADDRESS: &str = "address";
pub const BOOL: &str = "bool";
pub const VECTOR: &str = "vector";
pub const SOME: &str = "some";
pub const NONE: &str = "none";
pub const GAS: &str = "gas";

pub const KEYWORDS: &[&str] = &[
    ADDRESS, BOOL, VECTOR, SOME, NONE, GAS, U8, U16, U32, U64, U128, U256,
];

pub fn is_keyword(s: &str) -> bool {
    KEYWORDS.contains(&s)
}

pub fn all_keywords() -> String {
    KEYWORDS[..KEYWORDS.len() - 1]
        .iter()
        .map(|x| format!("'{}'", x))
        .collect::<Vec<_>>()
        .join(", ")
        + &format!(", or '{}'", KEYWORDS[KEYWORDS.len() - 1])
}

/// A PTB Program consisting of a list of commands and a flag indicating if the preview
/// warn-shadows command was present.
#[derive(Debug, Clone)]
pub struct Program {
    pub commands: Vec<Spanned<ParsedPTBCommand>>,
    // Held outside of metadata since this is used by the PTB builder
    pub warn_shadows_set: bool,
}

/// The `ProgramMetadata` struct holds metadata about a PTB program, such as whether the preview
/// flag was set, json output was set, etc.
#[derive(Debug, Clone)]
pub struct ProgramMetadata {
    pub preview_set: bool,
    pub summary_set: bool,
    pub gas_object_id: Option<Spanned<ObjectID>>,
    pub json_set: bool,
    pub gas_budget: Spanned<u64>,
}

/// A parsed module access consisting of the address, module name, and function name.
#[derive(Debug, Clone)]
pub struct ModuleAccess {
    pub address: Spanned<ParsedAddress>,
    pub module_name: Spanned<Identifier>,
    pub function_name: Spanned<Identifier>,
}

/// A parsed PTB command consisting of the command and the parsed arguments to the command.
#[derive(Debug, Clone)]
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
    WarnShadows,
    Preview,
}

/// An enum representing the parsed arguments of a PTB command.
#[derive(Debug, Clone)]
pub enum Argument {
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    U256(move_core_types::u256::U256),
    Gas,
    Identifier(String),
    VariableAccess(Spanned<String>, Vec<Spanned<String>>),
    Address(NumericalAddress),
    String(String),
    Vector(Vec<Spanned<Argument>>),
    Option(Spanned<Option<Box<Argument>>>),
}

impl Argument {
    /// Resolve an `Argument` into a `MoveValue` if possible. Errors if the `Argument` is not
    /// convertible to a `MoveValue`.
    pub fn to_move_value_opt(&self, loc: Span) -> PTBResult<MoveValue> {
        Ok(match self {
            Argument::Bool(b) => MoveValue::Bool(*b),
            Argument::U8(u) => MoveValue::U8(*u),
            Argument::U16(u) => MoveValue::U16(*u),
            Argument::U32(u) => MoveValue::U32(*u),
            Argument::U64(u) => MoveValue::U64(*u),
            Argument::U128(u) => MoveValue::U128(*u),
            Argument::U256(u) => MoveValue::U256(*u),
            Argument::Address(a) => MoveValue::Address(a.into_inner()),
            Argument::Vector(vs) => MoveValue::Vector(
                vs.iter()
                    .map(|sp!(loc, v)| v.to_move_value_opt(*loc))
                    .collect::<PTBResult<Vec<_>>>()
                    .map_err(|e| e.with_help(
                            format!("Was unable to parse '{self}' as a pure PTB value. This is most likely because \
                                    the vector contains non-primitive (e.g., object or array) \
                                    values which aren't permitted inside vectors")
                            ))?
            ),
            Argument::String(s) => {
                MoveValue::Vector(s.bytes().map(MoveValue::U8).collect::<Vec<_>>())
            }
            Argument::Option(sp!(loc, o)) => {
                if let Some(v) = o {
                    let v = v.as_ref().to_move_value_opt(*loc).map_err(|e| e.with_help(
                            format!(
                                "Was unable to parse '{self}' as a pure PTB value. This is most likely because \
                                the option contains a non-primitive (e.g., object or array) \
                                value which isn't permitted inside an option"
                                )
                            ))?;
                    MoveValue::Vector(vec![v])
                } else {
                    MoveValue::Vector(vec![])
                }
            }
            Argument::Identifier(_)
            | Argument::VariableAccess(_, _)
            | Argument::Gas => error!(loc, "Was unable to convert '{self}' to primitive value (i.e., non-object value)"),
        })
    }
}

impl fmt::Display for Argument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Argument::Bool(b) => write!(f, "{}", b),
            Argument::U8(u) => write!(f, "{}u8", u),
            Argument::U16(u) => write!(f, "{}u16", u),
            Argument::U32(u) => write!(f, "{}u32", u),
            Argument::U64(u) => write!(f, "{}u64", u),
            Argument::U128(u) => write!(f, "{}u128", u),
            Argument::U256(u) => write!(f, "{}u256", u),
            Argument::Gas => write!(f, "gas"),
            Argument::Identifier(i) => write!(f, "{}", i),
            Argument::VariableAccess(sp!(_, head), accesses) => {
                write!(f, "{}", head)?;
                for sp!(_, access) in accesses {
                    write!(f, ".{}", access)?;
                }
                Ok(())
            }
            Argument::Address(a) => write!(f, "@{}", a),
            Argument::String(s) => write!(f, "\"{}\"", s),
            Argument::Vector(v) => {
                write!(f, "vector[")?;
                for (i, sp!(_, arg)) in v.iter().enumerate() {
                    write!(f, "{}", arg)?;
                    if i != v.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, "]")
            }
            Argument::Option(sp!(_, o)) => match o {
                Some(v) => write!(f, "some({})", v),
                None => write!(f, "none"),
            },
        }
    }
}

impl fmt::Display for ParsedPTBCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

impl<'a> fmt::Display for TyDisplay<'a> {
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
