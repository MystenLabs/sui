// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_stackless_bytecode_2::ast::{DataOp, PrimitiveOp};
use pretty::RcDoc;
use pretty::RcDoc as RD;

use crate::ast::Exp;

type Doc = RcDoc<'static, ()>;

pub fn to_doc(exp: &Exp) -> Doc {
    // Helpers -------------------------------------------------------------

    fn text<T: Into<String>>(t: T) -> Doc {
        RD::text(t.into())
    }

    fn space() -> Doc {
        RD::space()
    }

    fn comma_sep<I>(items: I) -> Doc
    where
        I: IntoIterator<Item = Doc>,
    {
        RD::intersperse(items, text(", "))
    }

    fn parens(d: Doc) -> Doc {
        text("(").append(d).append(text(")"))
    }

    fn braces_block(body: Doc) -> Doc {
        // Multiline block:
        // {
        //     ...
        // }
        text("{")
            .append(RD::nest(RD::hardline().append(body), 4))
            .append(RD::hardline())
            .append(text("}"))
    }

    fn stmt(doc: Doc) -> Doc {
        // attach a trailing semicolon to a simple statement
        doc.append(text(";"))
    }

    // Render a list of statements separated by hardlines.
    fn stmts<I>(it: I) -> Doc
    where
        I: IntoIterator<Item = Exp>,
    {
        // We print each sub-exp as a statement unless it’s already a blocky thing.
        let docs: Vec<Doc> = it
            .into_iter()
            .map(|e| match e {
                Exp::Loop(b) => RD::group(
                    text("loop")
                        .append(space())
                        .append(braces_block(e_block(*b))),
                ),
                Exp::While(c, b) => while_doc(*c, *b),
                Exp::IfElse(c, t, e) => if_doc(*c, *t, *e),
                // everything else we treat as a simple statement w/ semicolon
                other => stmt(expr(other)),
            })
            .collect();

        RD::intersperse(docs, RD::hardline())
    }

    // Expression-ish printers --------------------------------------------

    fn expr(e: Exp) -> Doc {
        match e {
            Exp::Break => text("break"),
            Exp::Continue => text("continue"),
            Exp::Return(es) => {
                if es.is_empty() {
                    text("return")
                } else {
                    text("return")
                        .append(space())
                        .append(comma_sep(es.into_iter().map(expr)))
                }
            }
            Exp::Assign(lhs, rhs) => {
                let lhs_doc = comma_sep(lhs.into_iter().map(|s| text(s)));
                lhs_doc
                    .append(space())
                    .append(text("="))
                    .append(space())
                    .append(expr(*rhs))
            }
            Exp::LetBind(lhs, rhs) => {
                let lhs_doc = comma_sep(lhs.into_iter().map(|s| text(s)));
                text("let")
                    .append(space())
                    .append(lhs_doc)
                    .append(space())
                    .append(text("="))
                    .append(space())
                    .append(expr(*rhs))
            }
            Exp::Call(f, args) => text(f).append(parens(comma_sep(args.into_iter().map(expr)))),
            Exp::Abort(e) => text("abort").append(space()).append(expr(*e)),
            Exp::Borrow(mutable, e) => {
                if mutable {
                    text("&mut").append(space()).append(expr(*e))
                } else {
                    text("&").append(expr(*e))
                }
            }
            Exp::Value(v) => text(format!("{v:?}")), // TODO: implement a nicer ToDoc for Value
            Exp::Variable(s) => text(s),
            Exp::Constant(c) => text(format!("{c:?}")), // TODO: nicer constant printing

            // Blocky/compound forms as expressions (when needed)
            Exp::Seq(vs) => braces_block(stmts(vs)),
            Exp::Loop(b) => text("loop")
                .append(space())
                .append(braces_block(e_block(*b))),
            Exp::While(c, b) => while_doc(*c, *b),
            Exp::IfElse(c, t, e) => if_doc(*c, *t, *e),
            Exp::Switch(subject, arms) => {
                // Very simple decompiled `switch`:
                // switch (scrut) {
                //     <arm0>;
                //     <arm1>;
                // }
                let arms_doc = stmts(arms);
                text("switch")
                    .append(space())
                    .append(parens(expr(*subject)))
                    .append(space())
                    .append(braces_block(arms_doc))
            }
            // Fallbacks for forms we didn’t special-case (shouldn’t happen here):
            Exp::Primitive { op, args } => primitive_op_doc(&op, &args),
            Exp::Data { op, args } => data_op_doc(&op, &args),
        }
    }

    fn e_block(e: Exp) -> Doc {
        // If it’s already a block/seq, print its statements;
        // otherwise, treat the single expression as a statement.
        match e {
            Exp::Seq(vs) => stmts(vs),
            other => stmt(expr(other)),
        }
    }

    fn while_doc(cond: Exp, body: Exp) -> Doc {
        text("while")
            .append(space())
            .append(parens(expr(cond)))
            .append(space())
            .append(braces_block(e_block(body)))
    }

    fn if_doc(cond: Exp, then_b: Exp, else_b: Option<Exp>) -> Doc {
        let then_block = braces_block(e_block(then_b));
        match else_b {
            None => text("if")
                .append(space())
                .append(parens(expr(cond)))
                .append(space())
                .append(then_block),
            Some(e) => {
                let else_block = braces_block(e_block(e));
                text("if")
                    .append(space())
                    .append(parens(expr(cond)))
                    .append(space())
                    .append(then_block)
                    .append(space())
                    .append(text("else"))
                    .append(space())
                    .append(else_block)
            }
        }
    }

    // Top-level: print as a *statement* unless clearly an expression-only context
    match exp {
        // These variants are already handled in `expr` with reasonable defaults.
        _ => expr(exp.clone()),
    }
}

fn parens(d: Doc) -> Doc {
    RD::text("(").append(d).append(RD::text(")"))
}

fn comma_sep<I: IntoIterator<Item = Doc>>(it: I) -> Doc {
    RD::intersperse(it, RD::text(", "))
}

pub fn data_op_doc(op: &DataOp, args: &[Exp]) -> Doc {
    match op {
        DataOp::Pack(_) => RD::text("/* TODO: pack */"),
        DataOp::Unpack(_) => RD::text("/* TODO: unpack */"),

        DataOp::ReadRef => RD::text("*").append(to_doc(&args[0])),

        DataOp::WriteRef => RD::text("*")
            .append(to_doc(&args[0]))
            .append(RD::space())
            .append(RD::text("="))
            .append(RD::space())
            .append(to_doc(&args[1])),

        DataOp::FreezeRef => RD::text("freeze").append(parens(to_doc(&args[0]))),

        DataOp::MutBorrowField(field_ref) => RD::text("&mut ")
            .append(parens(to_doc(&args[0])))
            .append(RD::text("."))
            .append(RD::as_string(&field_ref.field.name)),

        DataOp::ImmBorrowField(field_ref) => RD::text("&")
            .append(parens(to_doc(&args[0])))
            .append(RD::text("."))
            .append(RD::as_string(&field_ref.field.name)),

        DataOp::VecPack(_) => RD::text("vec![")
            .append(comma_sep(args.iter().map(|e| to_doc(e))))
            .append(RD::text("]")),

        DataOp::VecLen(_) => to_doc(&args[0]).append(RD::text(".len()")),

        DataOp::VecImmBorrow(_) => RD::text("&")
            .append(to_doc(&args[0]))
            .append(RD::text("["))
            .append(to_doc(&args[1]))
            .append(RD::text("]")),

        DataOp::VecMutBorrow(_) => RD::text("&mut ")
            .append(to_doc(&args[0]))
            .append(RD::text("["))
            .append(to_doc(&args[1]))
            .append(RD::text("]")),

        DataOp::VecPushBack(_) => to_doc(&args[0])
            .append(RD::text(".push_back("))
            .append(to_doc(&args[1]))
            .append(RD::text(")")),

        DataOp::VecPopBack(_) => to_doc(&args[0])
            .append(RD::text(".pop_back("))
            .append(to_doc(&args[1]))
            .append(RD::text(")")),

        DataOp::VecUnpack(_) => RD::text("/* unreachable: VecUnpack */"),

        DataOp::VecSwap(_) => to_doc(&args[0])
            .append(RD::text(".swap("))
            .append(to_doc(&args[1]))
            .append(RD::text(", "))
            .append(to_doc(&args[2]))
            .append(RD::text(")")),

        DataOp::PackVariant(_) => RD::text("/* TODO: PackVariant E::V { ... } */"),

        DataOp::UnpackVariant(_)
        | DataOp::UnpackVariantImmRef(_)
        | DataOp::UnpackVariantMutRef(_) => RD::text("/* unreachable: unpack variant */"),
    }
}

pub fn primitive_op_doc(op: &PrimitiveOp, args: &[Exp]) -> Doc {
    let bin = |lhs: &Exp, sym: &str, rhs: &Exp| {
        to_doc(lhs)
            .append(RD::space())
            .append(RD::text(sym.to_string()))
            .append(RD::space())
            .append(to_doc(rhs))
    };

    match op {
        PrimitiveOp::CastU8 => RD::text("/* TODO: cast<u8> */"),
        PrimitiveOp::CastU16 => RD::text("/* TODO: cast<u16> */"),
        PrimitiveOp::CastU32 => RD::text("/* TODO: cast<u32> */"),
        PrimitiveOp::CastU64 => RD::text("/* TODO: cast<u64> */"),
        PrimitiveOp::CastU128 => RD::text("/* TODO: cast<u128> */"),
        PrimitiveOp::CastU256 => RD::text("/* TODO: cast<u256> */"),

        PrimitiveOp::Add => bin(&args[0], "+", &args[1]),
        PrimitiveOp::Subtract => bin(&args[0], "-", &args[1]),
        PrimitiveOp::Multiply => bin(&args[0], "*", &args[1]),
        PrimitiveOp::Modulo => bin(&args[0], "%", &args[1]),
        PrimitiveOp::Divide => bin(&args[0], "/", &args[1]),
        PrimitiveOp::BitOr => bin(&args[0], "|", &args[1]),
        PrimitiveOp::BitAnd => bin(&args[0], "&", &args[1]),
        PrimitiveOp::Xor => bin(&args[0], "^", &args[1]),
        PrimitiveOp::Or => bin(&args[0], "||", &args[1]),
        PrimitiveOp::And => bin(&args[0], "&&", &args[1]),
        PrimitiveOp::Equal => bin(&args[0], "==", &args[1]),
        PrimitiveOp::NotEqual => bin(&args[0], "!=", &args[1]),
        PrimitiveOp::LessThan => bin(&args[0], "<", &args[1]),
        PrimitiveOp::GreaterThan => bin(&args[0], ">", &args[1]),
        PrimitiveOp::LessThanOrEqual => bin(&args[0], "<=", &args[1]),
        PrimitiveOp::GreaterThanOrEqual => bin(&args[0], ">=", &args[1]),

        PrimitiveOp::Not => RD::text("!").append(parens(to_doc(&args[0]))),

        PrimitiveOp::ShiftLeft => RD::text("/* TODO: << */"),
        PrimitiveOp::ShiftRight => RD::text("/* TODO: >> */"),
    }
}
