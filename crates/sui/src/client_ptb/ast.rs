// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use move_core_types::parsing::{
    address::{NumericalAddress, ParsedAddress},
    types::{ParsedFqName, ParsedModuleId, ParsedStructType, ParsedType},
};
use move_core_types::runtime_value::MoveValue;
use sui_types::{
    base_types::{ObjectID, RESOLVED_ASCII_STR, RESOLVED_STD_OPTION, RESOLVED_UTF8_STR},
    Identifier, TypeTag,
};

use crate::{err, error, sp};

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
pub const DRY_RUN: &str = "dry-run";
pub const DEV_INSPECT: &str = "dev-inspect";
pub const SERIALIZE_UNSIGNED: &str = "serialize-unsigned-transaction";
pub const SERIALIZE_SIGNED: &str = "serialize-signed-transaction";

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

pub const COMMANDS: &[&str] = &[
    TRANSFER_OBJECTS,
    SPLIT_COINS,
    MERGE_COINS,
    MAKE_MOVE_VEC,
    MOVE_CALL,
    PUBLISH,
    UPGRADE,
    ASSIGN,
    PREVIEW,
    WARN_SHADOWS,
    GAS_BUDGET,
    SUMMARY,
    GAS_COIN,
    JSON,
    DRY_RUN,
    DEV_INSPECT,
    SERIALIZE_UNSIGNED,
    SERIALIZE_SIGNED,
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
    pub serialize_unsigned_set: bool,
    pub serialize_signed_set: bool,
    pub gas_object_id: Option<Spanned<ObjectID>>,
    pub json_set: bool,
    pub dry_run_set: bool,
    pub dev_inspect_set: bool,
    pub gas_budget: Option<Spanned<u64>>,
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
    TransferObjects(Spanned<Vec<Spanned<Argument>>>, Spanned<Argument>),
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
    InferredNum(move_core_types::u256::U256),
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
    /// convertible to a `MoveValue` of the provided `tag`.
    pub fn checked_to_pure_move_value(&self, loc: Span, tag: &TypeTag) -> PTBResult<MoveValue> {
        Ok(match (self, tag) {
            (Argument::Bool(b), TypeTag::Bool) => MoveValue::Bool(*b),
            (Argument::U8(u), TypeTag::U8) => MoveValue::U8(*u),
            (Argument::U16(u), TypeTag::U16) => MoveValue::U16(*u),
            (Argument::U32(u), TypeTag::U32) => MoveValue::U32(*u),
            (Argument::U64(u), TypeTag::U64) => MoveValue::U64(*u),
            (Argument::U128(u), TypeTag::U128) => MoveValue::U128(*u),
            (Argument::U256(u), TypeTag::U256) => MoveValue::U256(*u),
            // Inferred numbers, unless they need to be used as a specific type default to u64.
            (Argument::InferredNum(u), tag) => Self::cast_inferrred_num(*u, tag, loc)?,
            (Argument::Address(a), TypeTag::Address) => MoveValue::Address(a.into_inner()),
            (Argument::Vector(vs), TypeTag::Vector(ty)) => MoveValue::Vector(
                vs.iter()
                    .map(|sp!(loc, v)| v.checked_to_pure_move_value(*loc, ty))
                    .collect::<PTBResult<Vec<_>>>()
                    .map_err(|e| {
                        e.with_help("Literal vectors cannot contain object values.".to_string())
                    })?,
            ),
            (Argument::String(s), TypeTag::Vector(ty)) if **ty == TypeTag::U8 => {
                MoveValue::Vector(s.bytes().map(MoveValue::U8).collect::<Vec<_>>())
            }
            (Argument::String(s), TypeTag::Struct(stag))
                if {
                    let resolved = (
                        &stag.address,
                        stag.module.as_ident_str(),
                        stag.name.as_ident_str(),
                    );
                    resolved == RESOLVED_ASCII_STR || resolved == RESOLVED_UTF8_STR
                } =>
            {
                MoveValue::Vector(s.bytes().map(MoveValue::U8).collect::<Vec<_>>())
            }
            (Argument::Option(sp!(loc, o)), TypeTag::Vector(ty)) => {
                if let Some(v) = o {
                    let v = v
                        .as_ref()
                        .checked_to_pure_move_value(*loc, ty)
                        .map_err(|e| {
                            e.with_help(
                                "Literal option values cannot contain object values.".to_string(),
                            )
                        })?;
                    MoveValue::Vector(vec![v])
                } else {
                    MoveValue::Vector(vec![])
                }
            }
            (Argument::Option(sp!(loc, o)), TypeTag::Struct(stag))
                if (
                    &stag.address,
                    stag.module.as_ident_str(),
                    stag.name.as_ident_str(),
                ) == RESOLVED_STD_OPTION
                    && stag.type_params.len() == 1 =>
            {
                if let Some(v) = o {
                    let v = v
                        .as_ref()
                        .checked_to_pure_move_value(*loc, &stag.type_params[0])
                        .map_err(|e| {
                            e.with_help(
                                "Literal option values cannot contain object values.".to_string(),
                            )
                        })?;
                    MoveValue::Vector(vec![v])
                } else {
                    MoveValue::Vector(vec![])
                }
            }
            (Argument::Identifier(_) | Argument::VariableAccess(_, _) | Argument::Gas, _) => {
                error!(loc, "Unable to convert '{self}' to non-object value.")
            }
            (arg, tag) => error!(loc, "Unable to serialize '{arg}' as a {tag} value"),
        })
    }

    /// Resolve an `Argument` into a `MoveValue` if possible. Errors if the `Argument` is not
    /// convertible to a `MoveValue`.
    pub fn to_pure_move_value(&self, loc: Span) -> PTBResult<MoveValue> {
        Ok(match self {
            Argument::Bool(b) => MoveValue::Bool(*b),
            Argument::U8(u) => MoveValue::U8(*u),
            Argument::U16(u) => MoveValue::U16(*u),
            Argument::U32(u) => MoveValue::U32(*u),
            Argument::U64(u) => MoveValue::U64(*u),
            Argument::U128(u) => MoveValue::U128(*u),
            Argument::U256(u) => MoveValue::U256(*u),
            // Inferred numbers, unless they need to be used as a specific type default to u64.
            Argument::InferredNum(u) => Self::cast_inferrred_num(*u, &TypeTag::U64, loc)?,
            Argument::Address(a) => MoveValue::Address(a.into_inner()),
            Argument::Vector(vs) => MoveValue::Vector(
                vs.iter()
                    .map(|sp!(loc, v)| v.to_pure_move_value(*loc))
                    .collect::<PTBResult<Vec<_>>>()
                    .map_err(|e| {
                        e.with_help("Literal vectors cannot contain object values.".to_string())
                    })?,
            ),
            Argument::String(s) => {
                MoveValue::Vector(s.bytes().map(MoveValue::U8).collect::<Vec<_>>())
            }
            Argument::Option(sp!(loc, o)) => {
                if let Some(v) = o {
                    let v = v.as_ref().to_pure_move_value(*loc).map_err(|e| {
                        e.with_help(
                            "Literal option values cannot contain object values.".to_string(),
                        )
                    })?;
                    MoveValue::Vector(vec![v])
                } else {
                    MoveValue::Vector(vec![])
                }
            }
            Argument::Identifier(_) | Argument::VariableAccess(_, _) | Argument::Gas => {
                error!(loc, "Unable to convert '{self}' to non-object value.")
            }
        })
    }

    fn cast_inferrred_num(
        val: move_core_types::u256::U256,
        tag: &TypeTag,
        loc: Span,
    ) -> PTBResult<MoveValue> {
        match tag {
            TypeTag::U8 => u8::try_from(val)
                .map(MoveValue::U8)
                .map_err(|_| err!(loc, "Value {val} is too large to be a u8 value")),
            TypeTag::U16 => u16::try_from(val)
                .map(MoveValue::U16)
                .map_err(|_| err!(loc, "Value {val} is too large to be a u16 value")),
            TypeTag::U32 => u32::try_from(val)
                .map(MoveValue::U32)
                .map_err(|_| err!(loc, "Value {val} is too large to be a u32 value")),
            TypeTag::U64 => u64::try_from(val)
                .map(MoveValue::U64)
                .map_err(|_| err!(loc, "Value {val} is too large to be a u64 value")),
            TypeTag::U128 => u128::try_from(val)
                .map(MoveValue::U128)
                .map_err(|_| err!(loc, "Value {val} is too large to be a u128 value")),
            TypeTag::U256 => Ok(MoveValue::U256(val)),
            _ => error!(loc, "Expected an integer type but got {tag} for '{val}'"),
        }
    }
}

impl fmt::Display for Argument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Argument::Bool(b) => write!(f, "{b}"),
            Argument::U8(u) => write!(f, "{u}u8"),
            Argument::U16(u) => write!(f, "{u}u16"),
            Argument::U32(u) => write!(f, "{u}u32"),
            Argument::U64(u) => write!(f, "{u}u64"),
            Argument::U128(u) => write!(f, "{u}u128"),
            Argument::U256(u) => write!(f, "{u}u256"),
            Argument::InferredNum(u) => write!(f, "{u}"),
            Argument::Gas => write!(f, "gas"),
            Argument::Identifier(i) => write!(f, "{i}"),
            Argument::VariableAccess(sp!(_, head), accesses) => {
                write!(f, "{}", head)?;
                for sp!(_, access) in accesses {
                    write!(f, ".{}", access)?;
                }
                Ok(())
            }
            Argument::Address(a) => write!(f, "@{a}"),
            Argument::String(s) => write!(f, "{s:?}"),
            Argument::Vector(v) => {
                write!(f, "vector[")?;
                let mut prefix = "";
                for sp!(_, arg) in v.iter() {
                    write!(f, "{prefix}")?;
                    write!(f, "{arg}")?;
                    prefix = ", ";
                }
                write!(f, "]")
            }
            Argument::Option(sp!(_, o)) => match o {
                Some(v) => write!(f, "some({v})"),
                None => write!(f, "none"),
            },
        }
    }
}

fn delimited_list<T: fmt::Display>(
    f: &mut std::fmt::Formatter<'_>,
    sep: &str,
    items: impl IntoIterator<Item = T>,
) -> std::fmt::Result {
    let mut prefix = "";
    for item in items {
        write!(f, "{}{}", prefix, item)?;
        prefix = sep;
    }
    Ok(())
}

impl fmt::Display for ParsedPTBCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParsedPTBCommand::TransferObjects(args, arg) => {
                write!(f, "{TRANSFER_OBJECTS} [")?;
                delimited_list(f, ", ", args.value.iter().map(|x| &x.value))?;
                write!(f, "]")?;
                write!(f, " {}", arg.value)
            }
            ParsedPTBCommand::SplitCoins(arg, args) => {
                write!(f, "{SPLIT_COINS} {} [", arg.value)?;
                delimited_list(f, ", ", args.value.iter().map(|x| &x.value))?;
                write!(f, "]")
            }
            ParsedPTBCommand::MergeCoins(arg, args) => {
                write!(f, "{MERGE_COINS} {} [", arg.value)?;
                delimited_list(f, ", ", args.value.iter().map(|x| &x.value))?;
                write!(f, "]")
            }
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
            ParsedPTBCommand::MakeMoveVec(ty, args) => {
                write!(f, "{MAKE_MOVE_VEC} <",)?;
                write!(f, "{}", TyDisplay(&ty.value))?;
                write!(f, "> [")?;
                delimited_list(f, ", ", args.value.iter().map(|x| &x.value))?;
                write!(f, "]")
            }
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
                let type_args = |f: &mut std::fmt::Formatter| match tys {
                    Some(tys) => {
                        write!(f, "<")?;
                        delimited_list(f, ", ", tys.value.iter().map(TyDisplay))?;
                        write!(f, ">")
                    }
                    None => Ok(()),
                };
                write!(
                    f,
                    "{MOVE_CALL} {}::{}::{}",
                    address.value, module_name.value, function_name.value
                )?;
                type_args(f)?;

                if !args.is_empty() {
                    write!(f, " ")?;
                }

                delimited_list(f, " ", args.iter().map(|x| x.value.to_string()))
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
            U16 => write!(f, "u16"),
            U32 => write!(f, "u32"),
            U64 => write!(f, "u64"),
            U128 => write!(f, "u128"),
            U256 => write!(f, "u256"),
            Bool => write!(f, "bool"),
            Signer => write!(f, "signer"),
            Vector(ty) => write!(f, "vector<{}>", TyDisplay(ty)),
            Struct(ParsedStructType {
                fq_name:
                    ParsedFqName {
                        module: ParsedModuleId { address, name },
                        name: struct_name,
                    },
                type_args,
            }) => {
                write!(f, "{address}::{name}::{struct_name}")?;
                if type_args.is_empty() {
                    Ok(())
                } else {
                    write!(f, "<")?;
                    delimited_list(f, ", ", type_args.iter().map(TyDisplay))?;
                    write!(f, ">")
                }
            }
        }
    }
}
