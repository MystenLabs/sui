// Copyright (c) The Move Contributors SPDX-License-Identifier: Apache-2.0

use move_stackless_bytecode_2::ast::{DataOp, PrimitiveOp};
use move_symbol_pool::Symbol;
use pretty_simple::{Doc, Doc as D, ToDoc, to_list};

use crate::ast::Exp;

impl ToDoc for Exp {
    fn to_doc(&self) -> Doc {
        fn braces_block(body: Doc) -> Doc {
            D::braces(D::line().concat(body.indent(4)).concat(D::line()))
        }

        // Render a list of statements separated by lines.
        fn stmts<'a, I>(it: I) -> Doc
        where
            I: IntoIterator<Item = &'a Exp>,
        {
            to_list(it, D::text(";").concat(D::line()))
        }

        // Expression-ish printers --------------------------------------------

        fn recur(e: &Exp) -> Doc {
            match e {
                Exp::Break => D::text("break"),
                Exp::Continue => D::text("continue"),
                Exp::Return(es) => {
                    if es.is_empty() {
                        D::text("return")
                    } else {
                        D::text("return").concat_space(comma_sep(es.iter().map(recur)))
                    }
                }
                Exp::Assign(lhs, rhs) => match &lhs[..] {
                    [] => rhs.to_doc(),
                    [x] => D::text(x)
                        .concat_space(D::text("="))
                        .concat_space(rhs.to_doc()),
                    _ => {
                        let lhs = D::parens(D::intersperse(
                            lhs.iter().map(D::text),
                            D::text(",").concat(D::space()),
                        ))
                        .group();
                        lhs.concat_space(D::text("=")).concat_space(rhs.to_doc())
                    }
                },
                Exp::LetBind(lhs, rhs) => {
                    let lhs_doc = match &lhs[..] {
                        [] => D::text("_"),
                        [x] => D::text(x),
                        _ => D::parens(D::intersperse(
                            lhs.iter().map(D::text),
                            D::text(",").concat(D::space()),
                        ))
                        .group(),
                    };
                    D::text("let")
                        .concat_space(lhs_doc)
                        .concat_space(D::text("="))
                        .concat_space(recur(rhs))
                }
                Exp::Call((m, f), args) => D::text(format!("{m}::{f}"))
                    .concat(D::parens(comma_sep(args.iter().map(recur)))),
                Exp::Abort(e) => D::text("abort").concat_space(recur(e)),
                Exp::Borrow(mutable, e) => {
                    if *mutable {
                        D::text("&mut").concat_space(recur(e))
                    } else {
                        D::text("&").concat(recur(e))
                    }
                }
                Exp::Value(v) => value(v),
                Exp::Variable(s) => D::text(s),
                Exp::Constant(c) => D::text(format!("{c:?}")),
                Exp::Seq(vs) => {
                    let final_semi =
                        matches!(vs.last(), Some(Exp::LetBind(_, _) | Exp::Assign(_, _)));
                    let mut stmts = stmts(vs);
                    if final_semi {
                        stmts = stmts.concat(D::text(";"));
                    }
                    braces_block(stmts)
                }
                Exp::Loop(b) => D::text("loop").concat_space(e_block(b)),
                Exp::While(c, b) => while_doc(c, b),
                Exp::IfElse(c, t, e) => if_doc(c, t, e),
                Exp::Switch(subject, (mid, enum_), arms) => {
                    let arms_doc = Doc::intersperse(
                        arms.iter().map(|(variant, body)| {
                            D::text(variant.as_str())
                                .concat_space(D::text("=>"))
                                .concat_space(e_block(body))
                        }),
                        D::text(",").concat(D::line()),
                    );
                    D::text(format!("switch {mid}::{enum_}"))
                        .concat_space(D::parens(recur(subject)))
                        .concat_space(braces_block(arms_doc))
                }
                Exp::Primitive { op, args } => primitive_op_doc(op, args),
                Exp::Data { op, args } => data_op_doc(op, args),
                Exp::Unpack((mod_, struct_), items, exp) => {
                    let items_doc = fields(items);
                    D::text(format!("{mod_}::{struct_}"))
                        .concat_space(items_doc)
                        .concat_space(D::text("="))
                        .concat_space(recur(exp))
                }
                Exp::UnpackVariant(unpack_kind, (mod_, enum_, variant), items, exp) => {
                    let items_doc = fields(items);
                    let unpack_str = match unpack_kind {
                        crate::ast::UnpackKind::Value => "",
                        crate::ast::UnpackKind::ImmRef => "&",
                        crate::ast::UnpackKind::MutRef => "&mut ",
                    };
                    D::text(format!("{mod_}::{enum_}::{variant}"))
                        .concat_space(items_doc)
                        .concat_space(D::text("="))
                        .concat_space(D::text(unpack_str))
                        .concat(recur(exp))
                }
            }
        }

        fn e_block(e: &Exp) -> Doc {
            // If it’s already a block/seq, print its statements;
            // otherwise, treat the single expression as a statement.
            match e {
                Exp::Seq(vs) => {
                    let final_semi =
                        matches!(vs.last(), Some(Exp::LetBind(_, _) | Exp::Assign(_, _)));
                    let mut stmts = stmts(vs);
                    if final_semi {
                        stmts = stmts.concat(D::text(";"));
                    }
                    braces_block(stmts)
                }
                other => {
                    let final_semi = matches!(other, Exp::LetBind(_, _) | Exp::Assign(_, _));
                    let mut body = recur(other);
                    if final_semi {
                        body = body.concat(D::text(";"));
                    }
                    braces_block(body)
                }
            }
        }

        fn while_doc(cond: &Exp, body: &Exp) -> Doc {
            D::text("while")
                .concat_space(D::parens(recur(cond)))
                .concat_space(e_block(body))
        }

        fn if_doc(cond: &Exp, then_b: &Exp, else_b: &Option<Exp>) -> Doc {
            let then_block = e_block(then_b);
            match else_b {
                None => D::text("if")
                    .concat_space(D::parens(recur(cond)))
                    .concat_space(then_block),
                Some(e) => {
                    let else_block = e_block(e);
                    D::text("if")
                        .concat_space(D::parens(recur(cond)))
                        .concat_space(then_block)
                        .concat_space(D::text("else"))
                        .concat_space(else_block)
                }
            }
        }
        recur(self)
    }
}

fn fields(fields: &[(Symbol, String)]) -> Doc {
    if fields.is_empty() {
        return D::nil().braces();
    };
    let doc = D::intersperse(
        fields.iter().map(|(name, ty)| {
            D::text(name.as_str())
                .concat(D::text(":"))
                .concat_space(D::text(ty))
        }),
        D::text(",").concat(D::space()),
    );
    D::space().concat(doc).concat(D::space()).braces()
}

fn value(v: &move_stackless_bytecode_2::ast::Value) -> Doc {
    match v {
        move_stackless_bytecode_2::ast::Value::Bool(b) => D::text(b.to_string()),
        move_stackless_bytecode_2::ast::Value::U8(u) => {
            D::text(u.to_string()).concat(D::text("u8"))
        }
        move_stackless_bytecode_2::ast::Value::U16(u) => {
            D::text(u.to_string()).concat(D::text("u16"))
        }
        move_stackless_bytecode_2::ast::Value::U32(u) => {
            D::text(u.to_string()).concat(D::text("u32"))
        }
        move_stackless_bytecode_2::ast::Value::U64(u) => {
            D::text(u.to_string()).concat(D::text("u64"))
        }
        move_stackless_bytecode_2::ast::Value::U128(u) => {
            D::text(u.to_string()).concat(D::text("u128"))
        }
        move_stackless_bytecode_2::ast::Value::U256(u) => {
            D::text(u.to_string()).concat(D::text("u256"))
        }
        move_stackless_bytecode_2::ast::Value::Address(a) => D::text(format!("@{:X}", a)),
        move_stackless_bytecode_2::ast::Value::Empty => D::nil(),
        move_stackless_bytecode_2::ast::Value::NotImplemented(_) => D::text("<not implemented>"),
        move_stackless_bytecode_2::ast::Value::Vector(values) => D::text("vec![")
            .concat(D::intersperse(
                values.iter().map(value),
                D::text(",").concat(D::space()),
            ))
            .concat(D::text("]")),
    }
}

fn comma_sep<I: IntoIterator<Item = Doc>>(it: I) -> Doc {
    D::intersperse(it, D::text(", "))
}

pub fn data_op_doc(op: &DataOp, args: &[Exp]) -> Doc {
    match op {
        DataOp::Pack(_) => D::text("/* TODO: pack */"),
        DataOp::Unpack(_) => D::text("/* TODO: unpack */"),

        DataOp::ReadRef => D::text("*").concat(args[0].to_doc()),

        DataOp::WriteRef => D::text("*")
            .concat(args[0].to_doc())
            .concat_space(D::text("="))
            .concat_space(args[1].to_doc()),

        DataOp::FreezeRef => D::text("freeze").concat_space(D::parens(args[0].to_doc())),

        DataOp::MutBorrowField(field_ref) => D::text("&mut ")
            .concat(D::parens(args[0].to_doc()))
            .concat(D::text("."))
            .concat(D::text(field_ref.field.name.as_str())),

        DataOp::ImmBorrowField(field_ref) => D::text("&")
            .concat(D::parens(args[0].to_doc()))
            .concat(D::text("."))
            .concat(D::text(field_ref.field.name.as_str())),

        DataOp::VecPack(_) => D::text("vec![")
            .concat(to_list(args, D::text(",").concat(D::space())))
            .concat(D::text("]")),

        DataOp::VecLen(_) => args[0].to_doc().concat(D::text(".len()")),

        DataOp::VecImmBorrow(_) => D::text("&")
            .concat(args[0].to_doc())
            .concat(D::text("["))
            .concat(args[1].to_doc())
            .concat(D::text("]")),

        DataOp::VecMutBorrow(_) => D::text("&mut ")
            .concat(args[0].to_doc())
            .concat(D::text("["))
            .concat(args[1].to_doc())
            .concat(D::text("]")),

        DataOp::VecPushBack(_) => args[0]
            .to_doc()
            .concat(D::text(".push_back("))
            .concat(args[1].to_doc())
            .concat(D::text(")")),

        DataOp::VecPopBack(_) => args[0]
            .to_doc()
            .concat(D::text(".pop_back("))
            .concat(args[1].to_doc())
            .concat(D::text(")")),

        DataOp::VecUnpack(_) => D::text("/* unreachable: VecUnpack */"),

        DataOp::VecSwap(_) => args[0]
            .to_doc()
            .concat(D::text(".swap("))
            .concat(args[1].to_doc())
            .concat(D::text(", "))
            .concat(args[2].to_doc())
            .concat(D::text(")")),

        DataOp::PackVariant(_) => D::text("/* TODO: PackVariant E::V { ... } */"),

        DataOp::UnpackVariant(_)
        | DataOp::UnpackVariantImmRef(_)
        | DataOp::UnpackVariantMutRef(_) => D::text("/* unreachable: unpack variant */"),
    }
}

pub fn primitive_op_doc(op: &PrimitiveOp, args: &[Exp]) -> Doc {
    let bin = |lhs: &Exp, sym: &str, rhs: &Exp| {
        lhs.to_doc()
            .concat_space(D::text(sym.to_string()))
            .concat_space(rhs.to_doc())
    };

    match op {
        PrimitiveOp::CastU8 => args[0].to_doc().concat(D::text("as u8")),
        PrimitiveOp::CastU16 => args[0].to_doc().concat(D::text("as u16")),
        PrimitiveOp::CastU32 => args[0].to_doc().concat(D::text("as u32")),
        PrimitiveOp::CastU64 => args[0].to_doc().concat(D::text("as u64")),
        PrimitiveOp::CastU128 => args[0].to_doc().concat(D::text("as u128")),
        PrimitiveOp::CastU256 => args[0].to_doc().concat(D::text("as u256")),

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

        PrimitiveOp::Not => D::text("!").concat(D::parens(args[0].to_doc())),

        PrimitiveOp::ShiftLeft => bin(&args[0], "<<", &args[1]),
        PrimitiveOp::ShiftRight => bin(&args[0], ">>", &args[1]),
    }
}
