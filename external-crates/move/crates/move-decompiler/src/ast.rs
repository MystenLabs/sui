// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::normalized::Constant;

use move_stackless_bytecode_2::stackless::ast::{DataOp, PrimitiveOp, Value};
use move_symbol_pool::Symbol;

use std::collections::BTreeMap;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Module {
    pub name: Symbol,
    pub functions: BTreeMap<Symbol, Function>,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: Symbol,
    pub code: Exp,
    // TODO add function args?
}

#[derive(Debug, Clone)]
pub enum Exp {
    Break,
    Continue,
    Loop(Box<Exp>),
    Seq(Vec<Exp>),
    While(Box<Exp>, Box<Exp>),
    IfElse(Box<Exp>, Box<Exp>, Box<Option<Exp>>),
    Switch(Box<Exp>, Vec<Exp>),
    Return(Vec<Exp>),
    // --------------------------------
    // non-control expressions
    Assign(Vec<String>, Box<Exp>),
    LetBind(Vec<String>, Box<Exp>),
    Call(String, Vec<Exp>),
    Abort(Box<Exp>),
    // Do we need drop?
    Primitive { op: PrimitiveOp, args: Vec<Exp> },
    Data { op: DataOp, args: Vec<Exp> },
    Borrow(/* mut*/ bool, Box<Exp>),
    Value(Value),
    Variable(String),
    Constant(std::rc::Rc<Constant<Symbol>>),
    // TODO should we add specific exps for unpacks?
}

// -------------------------------------------------------------------------------------------------
// Display
// -------------------------------------------------------------------------------------------------

// Display trait for module
impl std::fmt::Display for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "module {} {{", self.name)?;
        for (name, fun) in &self.functions {
            // TODO print function args
            writeln!(f, "    public fun {} () {{", name)?;
            write!(f, "{}", fun)?;
            writeln!(f, "    }}\n")?;
        }
        writeln!(f, "}}")
    }
}

// Display trait for function
impl std::fmt::Display for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)
    }
}

impl std::fmt::Display for Exp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn indent(f: &mut std::fmt::Formatter<'_>, level: usize) -> std::fmt::Result {
            for _ in 0..level {
                write!(f, "    ")?;
            }
            Ok(())
        }

        fn fmt_exp(f: &mut std::fmt::Formatter<'_>, exp: &Exp, level: usize) -> std::fmt::Result {
            match exp {
                Exp::Break => {
                    indent(f, level)?;
                    writeln!(f, "break;")
                }
                Exp::Continue => {
                    indent(f, level)?;
                    writeln!(f, "continue;")
                }
                Exp::Loop(body) => {
                    indent(f, level)?;
                    writeln!(f, "loop {{")?;
                    fmt_exp(f, body, level + 1)?;
                    indent(f, level)?;
                    writeln!(f, "}}")
                }
                Exp::Seq(seq) => {
                    if seq.is_empty() {
                        return Ok(());
                    } else {
                        for exp in seq {
                            fmt_exp(f, exp, level)?;
                        }
                    }
                    Ok(())
                }
                Exp::While(cond, body) => {
                    indent(f, level)?;
                    writeln!(f, "while({}) {{", cond)?;
                    fmt_exp(f, body, level + 1)?;
                    indent(f, level)?;
                    writeln!(f, "}}")
                }
                Exp::IfElse(cond, conseq, alt) => {
                    indent(f, level)?;
                    writeln!(f, "if ({}) {{", cond)?;
                    fmt_exp(f, conseq, level + 1)?;
                    indent(f, level)?;
                    if let Some(alt) = &**alt {
                        writeln!(f, "}} else {{")?;
                        fmt_exp(f, alt, level + 1)?;
                        indent(f, level)?;
                    }
                    writeln!(f, "}}")
                }
                Exp::Switch(term, cases) => {
                    indent(f, level)?;
                    writeln!(f, "match({}) {{", term)?;
                    for case in cases {
                        indent(f, level + 1)?;
                        // TODO fix variant name
                        writeln!(f, "VARIANT NAME => {{")?;
                        fmt_exp(f, case, level + 2)?;
                        indent(f, level + 1)?;
                        writeln!(f, "}},")?;
                    }
                    indent(f, level)?;
                    writeln!(f, "}}")
                }
                Exp::Data { op, args } => {
                    indent(f, level)?;
                    write_data_op(f, op, args)
                }
                Exp::Return(exps) => {
                    indent(f, level)?;
                    write!(f, "return ")?;
                    for exp in exps {
                        fmt_exp(f, exp, level)?;
                    }
                    writeln!(f)
                }
                Exp::Assign(items, exp) => {
                    indent(f, level)?;
                    writeln!(f, "{} = {};", items.join(", "), exp)
                }
                Exp::LetBind(items, exp) => {
                    indent(f, level)?;
                    writeln!(f, "let {} = {};", items.join(", "), exp)
                }
                Exp::Call(fun_name, exps) => {
                    indent(f, level)?;
                    write!(f, "{fun_name}(")?;
                    for exp in exps {
                        fmt_exp(f, exp, level)?;
                    }
                    writeln!(f, ")")
                }
                Exp::Abort(exp) => {
                    indent(f, level)?;
                    writeln!(f, "abort!({});", exp)
                }
                Exp::Primitive { op, args } => write_primitive_op(f, op, args),
                Exp::Borrow(mut_, exp) => write!(f, "{}{}", if *mut_ { "&mut " } else { "&" }, exp),
                Exp::Value(value) => write!(f, "{}", value),
                Exp::Variable(name) => write!(f, "{}", name),
                Exp::Constant(constant) => write!(f, "{:?}", constant),
            }
        }

        fmt_exp(f, self, 2)
    }
}

fn write_data_op(
    f: &mut std::fmt::Formatter<'_>,
    op: &DataOp,
    args: &[Exp],
) -> Result<(), std::fmt::Error> {
    match op {
        DataOp::Pack(_) => todo!(),
        DataOp::Unpack(_) => todo!(),
        DataOp::ReadRef => write!(f, "*{}", args[0]),
        DataOp::WriteRef => writeln!(f, "*{} = {}", args[0], args[1]),
        DataOp::FreezeRef => write!(f, "freeze({})", args[0]),
        DataOp::MutBorrowField(field_ref) => {
            write!(f, "&mut ({}).{}", args[0], field_ref.field.name)
        }
        DataOp::ImmBorrowField(field_ref) => write!(f, "&( {} ).{}", args[0], field_ref.field.name),
        DataOp::VecPack(_) => write!(
            f,
            "vec![{}]",
            args.iter()
                .map(|e| format!("{}", e))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        DataOp::VecLen(_) => write!(f, "{}.len()", args[0]),
        DataOp::VecImmBorrow(_) => write!(f, "&{}[{}]", args[0], args[1]),
        DataOp::VecMutBorrow(_) => write!(f, "&mut {}[{}]", args[0], args[1]),
        DataOp::VecPushBack(_) => write!(f, "{}.push_back({})", args[0], args[1]),
        DataOp::VecPopBack(_) => write!(f, "{}.pop_back({})", args[0], args[1]),
        DataOp::VecUnpack(_) => unreachable!(),
        DataOp::VecSwap(_) => write!(f, "{}.swap({}, {})", args[0], args[1], args[2]),
        DataOp::PackVariant(_) => write!(f, "E::V .. fields .. args"),
        DataOp::UnpackVariant(_) => unreachable!(),
        DataOp::UnpackVariantImmRef(_) => unreachable!(),
        DataOp::UnpackVariantMutRef(_) => unreachable!(),
    }
}

fn write_primitive_op(
    f: &mut std::fmt::Formatter<'_>,
    op: &PrimitiveOp,
    args: &[Exp],
) -> Result<(), std::fmt::Error> {
    match op {
        PrimitiveOp::CastU8 => todo!(),
        PrimitiveOp::CastU16 => todo!(),
        PrimitiveOp::CastU32 => todo!(),
        PrimitiveOp::CastU64 => todo!(),
        PrimitiveOp::CastU128 => todo!(),
        PrimitiveOp::CastU256 => todo!(),
        PrimitiveOp::Add => write!(f, "{} + {}", args[0], args[1]),
        PrimitiveOp::Subtract => write!(f, "{} - {}", args[0], args[1]),
        PrimitiveOp::Multiply => write!(f, "{} * {}", args[0], args[1]),
        PrimitiveOp::Modulo => write!(f, "{} % {}", args[0], args[1]),
        PrimitiveOp::Divide => write!(f, "{} / {}", args[0], args[1]),
        PrimitiveOp::BitOr => todo!(),
        PrimitiveOp::BitAnd => todo!(),
        PrimitiveOp::Xor => todo!(),
        PrimitiveOp::Or => todo!(),
        PrimitiveOp::And => todo!(),
        PrimitiveOp::Not => todo!(),
        PrimitiveOp::Equal => write!(f, "{} == {}", args[0], args[1]),
        PrimitiveOp::NotEqual => write!(f, "{} != {}", args[0], args[1]),
        PrimitiveOp::LessThan => write!(f, "{} < {}", args[0], args[1]),
        PrimitiveOp::GreaterThan => write!(f, "{} > {}", args[0], args[1]),
        PrimitiveOp::LessThanOrEqual => write!(f, "{} <= {}", args[0], args[1]),
        PrimitiveOp::GreaterThanOrEqual => write!(f, "{} >= {}", args[0], args[1]),
        PrimitiveOp::ShiftLeft => todo!(),
        PrimitiveOp::ShiftRight => todo!(),
    }
}
