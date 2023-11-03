// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;
use pretty::RcDoc;
use std::rc::Rc;

use move_model::{
    ast::{Condition, ConditionKind, Exp, ExpData, Operation, QuantKind, SpecBlockTarget, Value},
    model::{AbilityConstraint, AbilitySet, GlobalEnv, TypeParameter},
    symbol::Symbol,
    ty::{PrimitiveType, Type},
};

/// A type alias for the way how we use crate `pretty`'s document type.
///
/// `pretty` is a Wadler-style pretty printer. Our simple usage doesn't require
/// any lifetime management.
type Doc = RcDoc<'static, ()>;

const PRETTY_WIDTH: usize = 80;

pub struct SpecPrinter<'env> {
    env: &'env GlobalEnv,
    ctxt: &'env SpecBlockTarget,
}

impl<'env> SpecPrinter<'env> {
    pub fn new(env: &'env GlobalEnv, ctxt: &'env SpecBlockTarget) -> Self {
        Self { env, ctxt }
    }
}

impl SpecPrinter<'_> {
    fn print_sym(&self, sym: Symbol) -> Doc {
        Self::doc(self.sym_str(sym))
    }

    pub fn print_value(&self, val: &Value) -> Doc {
        match val {
            Value::Address(v) => Self::doc(format!("@{:#x}", v)),
            Value::Number(v) => Self::doc(v),
            Value::Bool(v) => Self::doc(v),
            Value::ByteArray(v) => match std::str::from_utf8(v) {
                Ok(vstr) => Self::doc(format!("b\"{}\"", vstr)),
                Err(_) => Self::doc(format!(
                    "x\"{}\"",
                    v.iter().map(|e| format!("{:02x}", e)).join(""),
                )),
            },
            Value::AddressArray(v) => Self::doc(format!(
                "x\"{}\"",
                v.iter().map(|e| format!("@{:#x}", e)).join(""),
            )),
            Value::Vector(v) => Self::doc(v.iter().map(|e| format!("{}", e)).join(",")),
        }
    }

    pub fn print_type(&self, ty: &Type, ty_params: &[TypeParameter]) -> Doc {
        match ty {
            Type::Primitive(PrimitiveType::Bool) => Self::doc("bool"),
            Type::Primitive(PrimitiveType::U8) => Self::doc("u8"),
            Type::Primitive(PrimitiveType::U16) => Self::doc("u16"),
            Type::Primitive(PrimitiveType::U32) => Self::doc("u32"),
            Type::Primitive(PrimitiveType::U64) => Self::doc("u64"),
            Type::Primitive(PrimitiveType::U128) => Self::doc("u128"),
            Type::Primitive(PrimitiveType::U256) => Self::doc("u256"),
            Type::Primitive(PrimitiveType::Address) => Self::doc("address"),
            Type::Primitive(PrimitiveType::Signer) => Self::doc("signer"),
            Type::Primitive(PrimitiveType::Num) => Self::doc("num"),
            Type::Tuple(tys) => Self::mk_tuple(tys, |t| self.print_type(t, ty_params)),
            Type::Vector(ty) => Self::mk_inst(Self::doc("vector"), [ty.as_ref()], |t| {
                self.print_type(t, ty_params)
            }),
            Type::Struct(mid, sid, tys) => {
                let struct_env = self.env.get_struct(mid.qualified(*sid));
                let base = Self::doc(format!(
                    "{}::{}",
                    struct_env.module_env.get_full_name_str(),
                    self.sym_str(struct_env.get_name())
                ));
                if tys.is_empty() {
                    base
                } else {
                    Self::mk_inst(base, tys, |t| self.print_type(t, ty_params))
                }
            }
            Type::TypeParameter(idx) => self.print_sym(ty_params.get(*idx as usize).unwrap().0),
            Type::Reference(false, ty) => {
                Self::concat([Self::doc("&"), self.print_type(ty, ty_params)])
            }
            Type::Reference(true, ty) => {
                Self::concat([Self::doc("&mut"), self.print_type(ty, ty_params)])
            }
            Type::Fun(arg_tys, ret_ty) => {
                let doc_args = Self::mk_tuple(arg_tys, |t| self.print_type(t, ty_params));
                let doc_retn = self.print_type(ret_ty, ty_params);
                Self::sep_colon_space([doc_args, doc_retn])
            }
            Type::TypeDomain(ty) => self.print_type(ty, ty_params),
            Type::ResourceDomain(mid, sid, tys_opt) => {
                let struct_env = self.env.get_struct(mid.qualified(*sid));
                let base = Self::doc(format!(
                    "{}::{}",
                    struct_env.module_env.get_full_name_str(),
                    self.sym_str(struct_env.get_name())
                ));
                match tys_opt {
                    None => base,
                    Some(tys) => Self::mk_inst(base, tys, |t| self.print_type(t, ty_params)),
                }
            }
            // these types can't be declared in move/spec language
            Type::Primitive(PrimitiveType::Range) => Self::doc("<range>"),
            Type::Primitive(PrimitiveType::EventStore) => Self::doc("<event-store>"),
            Type::Var(idx) => Self::doc(format!("<var-{}>", *idx)),
            Type::Error => Self::doc("<error>"),
        }
    }

    pub fn print_exp(&self, exp: &Exp, ty_params: &[TypeParameter]) -> Doc {
        use Operation::*;

        match exp.as_ref() {
            ExpData::Invalid(_) => Self::doc("<invalid>"),
            ExpData::Value(_, val) => self.print_value(val),
            ExpData::LocalVar(_, sym) => self.print_sym(*sym),
            ExpData::Temporary(_, idx) => {
                let fun_id = match self.ctxt {
                    SpecBlockTarget::Function(mid, fid)
                    | SpecBlockTarget::FunctionCode(mid, fid, _) => mid.qualified(*fid),
                    _ => unreachable!(
                        "Only spec expressions in a function spec or inlined spec are allowed to \
                        refer to a temporary variable"
                    ),
                };
                let fun_env = self.env.get_function(fun_id);
                let fun_params = fun_env.get_parameters();
                match fun_params.get(*idx) {
                    None => Self::doc(format!("$t{}", idx)),
                    Some(param) => self.print_sym(param.0),
                }
            }
            ExpData::Invoke(_, lambda, args) => Self::sep_space([
                Self::wrap("(", self.print_exp(lambda, ty_params), ")"),
                Self::mk_tuple(args, |e| self.print_exp(e, ty_params)),
            ]),
            ExpData::Lambda(_, vars, body) => {
                let doc_head = Self::doc("fun");
                let doc_args = Self::mk_tuple(vars, |v| {
                    Self::sep_colon_space([
                        self.print_sym(v.name),
                        self.print_type(&self.env.get_node_type(v.id), ty_params),
                    ])
                });
                let doc_body = self.print_exp(body, ty_params);
                Self::sep_space([doc_head, doc_args, doc_body])
            }
            ExpData::Quant(_, kind, vars, _triggers, cond_opt, body) => {
                let doc_head = match kind {
                    QuantKind::Exists => Self::doc("exists"),
                    QuantKind::Forall => Self::doc("forall"),
                    QuantKind::Choose => Self::doc("choose"),
                    QuantKind::ChooseMin => {
                        Self::sep_space([Self::doc("choose"), Self::doc("min")])
                    }
                };
                let doc_vars = Self::sep_comma_space(vars.iter().map(|(v, e)| {
                    Self::sep_colon_space([self.print_sym(v.name), self.print_exp(e, ty_params)])
                }));
                // TODO: add triggers
                let doc_cond = match cond_opt {
                    None => Doc::nil(),
                    Some(cond) => {
                        Self::sep_space([Self::doc("where"), self.print_exp(cond, ty_params)])
                    }
                };
                let doc_body = self.print_exp(body, ty_params);
                Self::sep_space([doc_head, doc_vars, doc_cond, doc_body])
            }
            ExpData::Block(_, vars, body) => Self::sep_semicolon_line(
                vars.iter()
                    .map(|v| {
                        let decl_doc = Self::mk_binary_op(
                            self.print_sym(v.name),
                            ":",
                            self.print_type(&self.env.get_node_type(v.id), ty_params),
                        );
                        let bind_doc = match &v.binding {
                            None => decl_doc,
                            Some(binding) => Self::mk_binary_op(
                                decl_doc,
                                "=",
                                self.print_exp(binding, ty_params),
                            ),
                        };
                        Self::sep_space([Self::doc("let"), bind_doc])
                    })
                    .chain(std::iter::once(self.print_exp(body, ty_params))),
            ),
            ExpData::IfElse(_, cond, then_exp, else_exp) => Self::sep_space([
                Self::doc("if"),
                Self::wrap("(", self.print_exp(cond, ty_params), ")"),
                Self::doc("{"),
                self.print_exp(then_exp, ty_params),
                Self::doc("}"),
                Self::doc("{"),
                self.print_exp(else_exp, ty_params),
                Self::doc("}"),
            ]),
            ExpData::Call(node_id, op, args) => {
                // utilities
                let print_call_unary = |op_repr: &str| -> Doc {
                    Self::mk_unary_op(op_repr, self.print_exp(&args[0], ty_params))
                };
                let print_call_binary = |op_repr: &str| -> Doc {
                    Self::mk_binary_op(
                        self.print_exp(&args[0], ty_params),
                        op_repr,
                        self.print_exp(&args[1], ty_params),
                    )
                };
                let print_call_fun = |fun_name: &str| -> Doc {
                    let doc_args = Self::mk_tuple(args, |e| self.print_exp(e, ty_params));
                    Self::concat([Self::doc(fun_name), doc_args])
                };
                let print_call_fun_inst = |fun_name: &str| -> Doc {
                    let doc_inst = Self::mk_inst(
                        Self::doc(fun_name),
                        self.env.get_node_instantiation(*node_id),
                        |t| self.print_type(&t, ty_params),
                    );
                    let doc_args = Self::mk_tuple(args, |e| self.print_exp(e, ty_params));
                    Self::concat([doc_inst, doc_args])
                };

                match op {
                    // spec function calls
                    Function(mid, fid, _label_opt) => {
                        let callee_menv = self.env.get_module(*mid);
                        let callee_decl = callee_menv.get_spec_fun(*fid);
                        let callee_name = format!(
                            "{}::{}",
                            callee_menv.get_full_name_str(),
                            self.sym_str(callee_decl.name),
                        );
                        print_call_fun(&callee_name)
                    }
                    // primitive operations
                    Pack(mid, sid) => {
                        let struct_env = self.env.get_struct(mid.qualified(*sid));
                        let doc_head = Self::doc(format!(
                            "{}::{}",
                            struct_env.module_env.get_full_name_str(),
                            self.sym_str(struct_env.get_name())
                        ));
                        let doc_body = Self::sep_comma_space(
                            struct_env.get_fields().zip(args.iter()).map(|(f, e)| {
                                Self::sep_colon_space([
                                    self.print_sym(f.get_name()),
                                    self.print_exp(e, ty_params),
                                ])
                            }),
                        );
                        Self::sep_space([doc_head, Self::doc("{"), doc_body, Self::doc("}")])
                    }
                    Tuple => Self::mk_tuple(args, |e| self.print_exp(e, ty_params)),
                    Select(mid, sid, fid) => {
                        let struct_env = self.env.get_struct(mid.qualified(*sid));
                        let field_env = struct_env.get_field(*fid);
                        Self::sep_dot([
                            self.print_exp(&args[0], ty_params),
                            self.print_sym(field_env.get_name()),
                        ])
                    }
                    Result(idx) => Self::sep_dot([Self::doc("result"), Self::doc(*idx)]),
                    Index => Self::concat([
                        self.print_exp(&args[0], ty_params),
                        Self::wrap("[", self.print_exp(&args[1], ty_params), "]"),
                    ]),
                    Slice => Self::concat([
                        self.print_exp(&args[0], ty_params),
                        Self::wrap("[", self.print_exp(&args[1], ty_params), "]"),
                    ]),
                    // binary operators
                    Range => Self::concat([
                        self.print_exp(&args[0], ty_params),
                        Self::doc(".."),
                        self.print_exp(&args[1], ty_params),
                    ]),
                    Add => print_call_binary("+"),
                    Sub => print_call_binary("-"),
                    Mul => print_call_binary("*"),
                    Div => print_call_binary("/"),
                    Mod => print_call_binary("%"),
                    BitOr => print_call_binary("|"),
                    BitAnd => print_call_binary("&"),
                    Xor => print_call_binary("^"),
                    Shl => print_call_binary("<<"),
                    Shr => print_call_binary(">>"),
                    Implies => print_call_binary("==>"),
                    Iff => print_call_binary("<==>"),
                    And => print_call_binary("&&"),
                    Or => print_call_binary("||"),
                    Eq => print_call_binary("=="),
                    Identical => print_call_binary("==="),
                    Neq => print_call_binary("!="),
                    Lt => print_call_binary("<"),
                    Gt => print_call_binary(">"),
                    Le => print_call_binary("<="),
                    Ge => print_call_binary(">="),
                    // unary operators
                    Not => print_call_unary("!"),
                    Cast => print_call_unary("cast"),
                    Int2Bv => print_call_unary("int2bv"),
                    Bv2Int => print_call_unary("bv2int"),
                    // built-in functions
                    Len => print_call_fun("len"),
                    Old => print_call_fun("old"),
                    Trace(_) => print_call_fun("TRACE"),
                    Global(_label_opt) => print_call_fun_inst("global"),
                    Exists(_label_opt) => print_call_fun_inst("exists"),
                    EmptyVec => print_call_fun_inst("vec"),
                    SingleVec => print_call_fun("vec"),
                    UpdateVec => print_call_fun("update"),
                    ConcatVec => print_call_fun("concat"),
                    IndexOfVec => print_call_fun("index_of"),
                    ContainsVec => print_call_fun("contains"),
                    RangeVec => print_call_fun("range"),
                    UpdateField(..) => print_call_fun("update_field"),
                    InRangeRange => print_call_fun("in_range"),
                    InRangeVec => print_call_fun("in_range"),
                    TypeDomain => self.print_type(&self.env.get_node_type(*node_id), ty_params),
                    ResourceDomain => self.print_type(&self.env.get_node_type(*node_id), ty_params),
                    MaxU8 => Self::doc("MAX_U8"),
                    MaxU16 => Self::doc("MAX_U16"),
                    MaxU32 => Self::doc("MAX_U32"),
                    MaxU64 => Self::doc("MAX_U64"),
                    MaxU128 => Self::doc("MAX_U128"),
                    MaxU256 => Self::doc("MAX_U256"),
                    // unable to be specified by users
                    CanModify | AbortFlag | AbortCode | WellFormed | BoxValue | UnboxValue
                    | EmptyEventStore | ExtendEventStore | EventStoreIncludes
                    | EventStoreIncludedIn | NoOp => Doc::nil(),
                    // unexpected
                    TypeValue => unreachable!("TypeValue is not currently supported"),
                }
            }
        }
    }

    pub fn print_condition(&self, cond: &Condition) -> Doc {
        // collect type parameter
        let ty_params = match &cond.kind {
            ConditionKind::LetPre(..)
            | ConditionKind::LetPost(..)
            | ConditionKind::Requires
            | ConditionKind::AbortsIf
            | ConditionKind::AbortsWith
            | ConditionKind::SucceedsIf
            | ConditionKind::Ensures
            | ConditionKind::Modifies
            | ConditionKind::Emits
            | ConditionKind::Update
            | ConditionKind::FunctionInvariant => match &self.ctxt {
                SpecBlockTarget::Function(mid, fid) => {
                    let fun_env = self.env.get_function(mid.qualified(*fid));
                    fun_env.get_type_parameters()
                }
                _ => unreachable!(
                    "Condition kind `{}` is not allowed in the function spec context",
                    cond.kind
                ),
            },
            ConditionKind::Assert
            | ConditionKind::Assume
            | ConditionKind::LoopInvariant
            | ConditionKind::Decreases => match &self.ctxt {
                SpecBlockTarget::FunctionCode(mid, fid, _) => {
                    let fun_env = self.env.get_function(mid.qualified(*fid));
                    fun_env.get_type_parameters()
                }
                _ => unreachable!(
                    "Condition kind `{}` is not allowed in the inlined spec context",
                    cond.kind
                ),
            },
            ConditionKind::StructInvariant => match &self.ctxt {
                SpecBlockTarget::Struct(mid, sid) => {
                    let struct_env = self.env.get_struct(mid.qualified(*sid));
                    struct_env.get_type_parameters()
                }
                _ => unreachable!(
                    "Condition kind `{}` is not allowed in the inlined spec context",
                    cond.kind
                ),
            },
            ConditionKind::GlobalInvariant(syms)
            | ConditionKind::GlobalInvariantUpdate(syms)
            | ConditionKind::Axiom(syms) => {
                assert!(matches!(self.ctxt, SpecBlockTarget::Module));
                syms.iter()
                    .map(|s| TypeParameter(*s, AbilityConstraint(AbilitySet::EMPTY)))
                    .collect()
            }
            // not expected
            ConditionKind::SchemaInvariant => {
                unreachable!("The `invariant` conditions in schema should have been flattened");
            }
        };

        // produce the expression
        let doc_stmt = match &cond.kind {
            ConditionKind::LetPre(sym) => {
                let bind_doc = Self::mk_binary_op(
                    self.print_sym(*sym),
                    "=",
                    self.print_exp(&cond.exp, &ty_params),
                );
                Self::sep_space([Self::doc("let"), bind_doc])
            }
            ConditionKind::LetPost(sym) => {
                let bind_doc = Self::mk_binary_op(
                    self.print_sym(*sym),
                    "=",
                    self.print_exp(&cond.exp, &ty_params),
                );
                Self::sep_space([Self::doc("let"), Self::doc("post"), bind_doc])
            }
            ConditionKind::Assert => {
                Self::sep_space([Self::doc("assert"), self.print_exp(&cond.exp, &ty_params)])
            }
            ConditionKind::Assume => {
                Self::sep_space([Self::doc("assume"), self.print_exp(&cond.exp, &ty_params)])
            }
            ConditionKind::Requires => {
                Self::sep_space([Self::doc("requires"), self.print_exp(&cond.exp, &ty_params)])
            }
            ConditionKind::AbortsIf => {
                let base_doc = Self::sep_space([
                    Self::doc("aborts_if"),
                    self.print_exp(&cond.exp, &ty_params),
                ]);
                if cond.additional_exps.is_empty() {
                    base_doc
                } else {
                    Self::sep_space([
                        base_doc,
                        Self::doc("with"),
                        self.print_exp(&cond.additional_exps[0], &ty_params),
                    ])
                }
            }
            ConditionKind::AbortsWith => Self::sep_space([
                Self::doc("aborts_with"),
                self.print_exp(&cond.exp, &ty_params),
            ]),
            ConditionKind::Ensures => {
                Self::sep_space([Self::doc("ensures"), self.print_exp(&cond.exp, &ty_params)])
            }
            ConditionKind::Modifies => {
                Self::sep_space([Self::doc("modifies"), self.print_exp(&cond.exp, &ty_params)])
            }
            ConditionKind::Emits => {
                Self::sep_space([Self::doc("emits"), self.print_exp(&cond.exp, &ty_params)])
            }
            ConditionKind::StructInvariant
            | ConditionKind::FunctionInvariant
            | ConditionKind::LoopInvariant => Self::sep_space([
                Self::doc("invariant"),
                self.print_exp(&cond.exp, &ty_params),
            ]),
            ConditionKind::GlobalInvariant(syms) => {
                let base_doc = Self::doc("invariant");
                let head_doc = if syms.is_empty() {
                    base_doc
                } else {
                    Self::mk_inst(base_doc, syms, |s| self.print_sym(*s))
                };
                Self::sep_space([head_doc, self.print_exp(&cond.exp, &ty_params)])
            }
            ConditionKind::GlobalInvariantUpdate(syms) => {
                let base_doc = Self::doc("invariant");
                let head_doc = if syms.is_empty() {
                    base_doc
                } else {
                    Self::mk_inst(base_doc, syms, |s| self.print_sym(*s))
                };
                Self::sep_space([
                    head_doc,
                    Self::doc("update"),
                    self.print_exp(&cond.exp, &ty_params),
                ])
            }
            ConditionKind::Axiom(syms) => {
                let base_doc = Self::doc("axiom");
                let head_doc = if syms.is_empty() {
                    base_doc
                } else {
                    Self::mk_inst(base_doc, syms, |s| self.print_sym(*s))
                };
                Self::sep_space([head_doc, self.print_exp(&cond.exp, &ty_params)])
            }
            ConditionKind::Update => {
                Self::sep_space([Self::doc("update"), self.print_exp(&cond.exp, &ty_params)])
            }
            // not really supported
            ConditionKind::Decreases => Self::sep_space([
                Self::doc("decreases"),
                self.print_exp(&cond.exp, &ty_params),
            ]),
            ConditionKind::SucceedsIf => Self::sep_space([
                Self::doc("succeeds_if"),
                self.print_exp(&cond.exp, &ty_params),
            ]),
            // should not see any more
            ConditionKind::SchemaInvariant => {
                unreachable!("The `invariant` conditions in schema should have been flattened");
            }
        };
        Self::mk_stmt(doc_stmt)
    }
}

impl SpecPrinter<'_> {
    pub fn convert(doc: Doc) -> String {
        doc.pretty(PRETTY_WIDTH).to_string()
    }
}

//
// High-level utilities for Doc
//

impl SpecPrinter<'_> {
    fn mk_tuple<E, I, F>(items: I, func: F) -> Doc
    where
        I: IntoIterator<Item = E>,
        F: Fn(E) -> Doc,
    {
        Self::wrap("(", Self::sep_comma_space(items.into_iter().map(func)), ")")
    }

    fn mk_inst<E, I, F>(base: Doc, items: I, func: F) -> Doc
    where
        I: IntoIterator<Item = E>,
        F: Fn(E) -> Doc,
    {
        Self::concat([
            base,
            Self::wrap('<', Self::sep_comma_space(items.into_iter().map(func)), ">"),
        ])
    }

    fn mk_stmt(base: Doc) -> Doc {
        base.append(Self::doc(";"))
    }

    fn mk_unary_op(op: &str, val: Doc) -> Doc {
        Self::concat([Self::doc(op), val])
    }

    fn mk_binary_op(lhs: Doc, op: &str, rhs: Doc) -> Doc {
        Self::sep_space([lhs, Self::doc(op), rhs])
    }
}

//
// Low-level utilities for Doc
//

impl SpecPrinter<'_> {
    fn doc<S: ToString>(txt: S) -> Doc {
        Doc::text(txt.to_string())
    }

    fn sym_str(&self, sym: Symbol) -> Rc<String> {
        self.env.symbol_pool().string(sym)
    }
}

impl SpecPrinter<'_> {
    fn concat<I>(tokens: I) -> Doc
    where
        I: IntoIterator<Item = Doc>,
    {
        Doc::concat(tokens)
    }

    fn sep_space<I>(tokens: I) -> Doc
    where
        I: IntoIterator<Item = Doc>,
    {
        Doc::intersperse(tokens, Doc::space())
    }

    fn sep_dot<I>(tokens: I) -> Doc
    where
        I: IntoIterator<Item = Doc>,
    {
        Doc::intersperse(tokens, Doc::text("."))
    }

    fn sep_comma_space<I>(tokens: I) -> Doc
    where
        I: IntoIterator<Item = Doc>,
    {
        Doc::intersperse(tokens, Doc::text(",").append(Doc::space()))
    }

    fn sep_colon_space<I>(tokens: I) -> Doc
    where
        I: IntoIterator<Item = Doc>,
    {
        Doc::intersperse(tokens, Doc::text(":").append(Doc::space()))
    }

    fn sep_semicolon_line<I>(tokens: I) -> Doc
    where
        I: IntoIterator<Item = Doc>,
    {
        Doc::intersperse(tokens, Doc::text(";").append(Doc::line()))
    }

    fn wrap<S1, S2>(before: S1, content: Doc, after: S2) -> Doc
    where
        S1: ToString,
        S2: ToString,
    {
        Doc::text(before.to_string())
            .append(content)
            .append(Doc::text(after.to_string()))
    }
}
