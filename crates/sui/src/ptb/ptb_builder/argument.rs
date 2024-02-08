// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error,
    ptb::ptb_builder::errors::{PTBResult, Span, Spanned},
    sp,
};
use core::fmt::{self, Debug};
use move_command_line_common::{
    address::{NumericalAddress, ParsedAddress},
    types::ParsedType,
};
use move_core_types::annotated_value::MoveValue;
use sui_types::{resolve_address, Identifier};

/// An enum representing the parsed arguments of a PTB command.
#[derive(Eq, PartialEq, Debug, Clone)]
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
    Array(Vec<Spanned<Argument>>),
    Option(Spanned<Option<Box<Argument>>>),
    ModuleAccess {
        address: Spanned<ParsedAddress>,
        module_name: Spanned<Identifier>,
        function_name: Spanned<Identifier>,
    },
    TyArgs(Vec<ParsedType>),
}

impl Argument {
    /// Resolve an `Argument` into a `MoveValue` if possible. Errors if the `Argument` is not
    /// convertible to a `MoveValue`.
    pub fn into_move_value_opt(&self, loc: Span) -> PTBResult<MoveValue> {
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
                    .map(|sp!(loc, v)| v.into_move_value_opt(*loc))
                    .collect::<PTBResult<Vec<_>>>()
                    .map_err(|e| e.with_help(
                            format!("Was unable to parse '{self}' as a pure PTB value. This is most likely because \
                                    the vector contains non-primitive (e.g., object or array) \
                                    values which aren't permitted inside vectors")
                            ))?
            ),
            Argument::String(s) => {
                MoveValue::Vector(s.bytes().into_iter().map(MoveValue::U8).collect::<Vec<_>>())
            }
            Argument::Option(sp!(loc, o)) => {
                if let Some(v) = o {
                    let v = v.as_ref().into_move_value_opt(*loc).map_err(|e| e.with_help(
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
            | Argument::Array(_)
            | Argument::ModuleAccess { .. }
            | Argument::VariableAccess(_, _)
            | Argument::Gas
            | Argument::TyArgs(_) => error!(loc, "Was unable to convert '{self}' to primitive value (i.e., non-object value)"),
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
            Argument::Array(a) => {
                write!(f, "[")?;
                for (i, sp!(_, arg)) in a.iter().enumerate() {
                    write!(f, "{}", arg)?;
                    if i != a.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, "]")
            }
            Argument::Option(sp!(_, o)) => match o {
                Some(v) => write!(f, "some({})", v),
                None => write!(f, "none"),
            },
            Argument::ModuleAccess {
                address,
                module_name,
                function_name,
            } => {
                let addr_string = match &address.value {
                    ParsedAddress::Named(name) => format!("{}", name),
                    ParsedAddress::Numerical(addr) => format!("{}", addr),
                };
                write!(
                    f,
                    "{}::{}::{}",
                    addr_string, module_name.value, function_name.value
                )
            }
            Argument::TyArgs(ts) => {
                write!(f, "<")?;
                for (i, t) in ts.iter().enumerate() {
                    write!(f, "{}", t.clone().into_type_tag(&resolve_address).unwrap())?;
                    if i != ts.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, ">")
            }
        }
    }
}
