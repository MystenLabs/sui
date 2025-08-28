// Copyright (c) Verichains, 2023

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fmt::Display,
    rc::Rc,
};

use super::super::super::naming::Naming;
use anyhow::Ok;
use move_model::ty::Type;
use move_stackless_bytecode::stackless_bytecode::Constant;

pub type ExprNodeRef = Rc<RefCell<ExprNode>>;
#[derive(Debug, PartialEq)]
pub enum ExprNodeOperation {
    Ignored,
    #[allow(dead_code)]
    Deleted,
    NonTrivial,
    Raw(String),
    Const(Constant),
    LocalVariable(usize),
    Field(ExprNodeRef, String),
    Unary(String, ExprNodeRef),
    Cast(String, ExprNodeRef),
    Binary(String, ExprNodeRef, ExprNodeRef),
    Func(String, Vec<ExprNodeRef>, Vec<Type>),

    Destroy(ExprNodeRef),
    FreezeRef(ExprNodeRef),
    ReadRef(ExprNodeRef),
    BorrowLocal(ExprNodeRef, /* mut */ bool),
    WriteRef(ExprNodeRef /* dst */, ExprNodeRef /* src */),
    DatatypePack(
        String, /* struct name */
        Vec<(
            String,      /* field name */
            ExprNodeRef, /* field value */
        )>,
        Vec<Type>,
    ),
    DatatypeUnpack(
        String,      /* struct name */
        Vec<String>, /* field names */
        ExprNodeRef,
        Vec<Type>,
    ),

    VariableSnapshot {
        variable: usize,
        assignment_id: usize,
        value: ExprNodeRef,
    },
}

#[derive(Clone, Debug)]
struct ToSourceCtx {
    in_borrow: bool,
    need_syntactical_brackets: bool,
}

impl ToSourceCtx {
    fn default() -> Self {
        Self {
            in_borrow: false,
            need_syntactical_brackets: false,
        }
    }

    fn with_syntactical_brackets(&self, need_syntactical_brackets: bool) -> Self {
        let mut ctx = self.clone();
        ctx.need_syntactical_brackets = need_syntactical_brackets;
        ctx
    }
}

pub fn effective_operation<R, const N: usize>(
    nodes: &[&ExprNodeRef; N],
    cb: &mut dyn FnMut(&[&ExprNodeRef; N]) -> R,
) -> R {
    fn resolve_node<R, const N: usize>(
        nodes: &[&ExprNodeRef; N],
        arr: &mut Vec<&ExprNodeRef>,
        idx: usize,
        cb: &mut dyn FnMut(&[&ExprNodeRef; N]) -> R,
    ) -> R {
        if idx == N {
            let mut fixed_arr = [nodes[0]; N];
            for i in 0..N {
                fixed_arr[i] = arr[i];
            }
            return cb(&fixed_arr);
        }
        let node = if idx < arr.len() {
            arr[idx]
        } else {
            nodes[idx]
        };
        let mut arr = arr.clone();
        if arr.len() <= idx {
            arr.push(&node);
        } else {
            arr[idx] = &node;
        }
        let node = node.borrow();
        match &node.operation {
            ExprNodeOperation::VariableSnapshot { value, .. } => {
                arr[idx] = value;
                resolve_node(nodes, &mut arr, idx, cb)
            }
            _ => resolve_node(nodes, &mut arr, idx + 1, cb),
        }
    }

    let mut effective_nodes = Vec::new();

    resolve_node(nodes, &mut effective_nodes, 0, cb)
}

impl ExprNodeOperation {
    pub fn copy(&self) -> Self {
        match self {
            ExprNodeOperation::Unary(op, arg) => {
                ExprNodeOperation::Unary(op.clone(), arg.borrow().copy_as_ref())
            }
            ExprNodeOperation::Cast(op, arg) => {
                ExprNodeOperation::Cast(op.clone(), arg.borrow().copy_as_ref())
            }
            ExprNodeOperation::Binary(op, lhs, rhs) => ExprNodeOperation::Binary(
                op.clone(),
                lhs.borrow().copy_as_ref(),
                rhs.borrow().copy_as_ref(),
            ),
            ExprNodeOperation::Func(name, args, types) => ExprNodeOperation::Func(
                name.clone(),
                args.iter().map(|x| x.borrow().copy_as_ref()).collect(),
                types.clone(),
            ),
            ExprNodeOperation::DatatypePack(name, args, types) => ExprNodeOperation::DatatypePack(
                name.clone(),
                args.iter()
                    .map(|x| ((x.0.clone(), x.1.borrow().copy_as_ref())))
                    .collect(),
                types.clone(),
            ),
            ExprNodeOperation::DatatypeUnpack(name, keys, val, types) => {
                ExprNodeOperation::DatatypeUnpack(
                    name.clone(),
                    keys.clone(),
                    val.borrow().copy_as_ref(),
                    types.clone(),
                )
            }
            ExprNodeOperation::Field(expr, name) => {
                ExprNodeOperation::Field(expr.borrow().copy_as_ref(), name.clone())
            }
            ExprNodeOperation::ReadRef(expr) => {
                ExprNodeOperation::ReadRef(expr.borrow().copy_as_ref())
            }
            ExprNodeOperation::BorrowLocal(expr, mutable) => {
                ExprNodeOperation::BorrowLocal(expr.borrow().copy_as_ref(), *mutable)
            }
            ExprNodeOperation::FreezeRef(expr) => {
                ExprNodeOperation::FreezeRef(expr.borrow().copy_as_ref())
            }
            ExprNodeOperation::Destroy(expr) => {
                ExprNodeOperation::Destroy(expr.borrow().copy_as_ref())
            }
            ExprNodeOperation::WriteRef(lhs, rhs) => {
                ExprNodeOperation::WriteRef(lhs.borrow().copy_as_ref(), rhs.borrow().copy_as_ref())
            }
            ExprNodeOperation::Raw(name) => ExprNodeOperation::Raw(name.clone()),
            ExprNodeOperation::Const(c) => ExprNodeOperation::Const(c.clone()),
            ExprNodeOperation::Ignored => ExprNodeOperation::Ignored,
            ExprNodeOperation::Deleted => ExprNodeOperation::Deleted,
            ExprNodeOperation::NonTrivial => ExprNodeOperation::NonTrivial,
            ExprNodeOperation::LocalVariable(idx) => ExprNodeOperation::LocalVariable(idx.clone()),
            ExprNodeOperation::VariableSnapshot {
                variable,
                assignment_id,
                value,
            } => ExprNodeOperation::VariableSnapshot {
                variable: variable.clone(),
                assignment_id: assignment_id.clone(),
                value: value.borrow().copy_as_ref(),
            },
        }
    }
    pub fn to_node(&self) -> ExprNodeRef {
        Rc::new(RefCell::new(ExprNode {
            operation: self.copy(),
        }))
    }
    pub fn to_expr(&self) -> Expr {
        Expr::new(self.to_node())
    }
    fn typeparams_to_source(types: &Vec<Type>, naming: &Naming) -> String {
        if types.is_empty() {
            String::new()
        } else {
            format!(
                "<{}>",
                types
                    .iter()
                    .map(|x| naming.ty(x))
                    .collect::<Vec<String>>()
                    .join(", ")
            )
        }
    }
    fn const_to_source(val: &Constant) -> Result<String, anyhow::Error> {
        match val {
            Constant::Bool(v) => Ok(format!("{}", v)),
            Constant::U8(x) => Ok(format!("{}", x)),
            Constant::U16(x) => Ok(format!("{}", x)),
            Constant::U32(x) => Ok(format!("{}", x)),
            Constant::U64(x) => Ok(format!("{}", x)),
            Constant::U128(x) => Ok(format!("{}", x)),
            Constant::U256(x) => Ok(format!("{}", x)),
            Constant::Address(x) => Ok(format!("@0x{}", x.to_str_radix(16))),
            Constant::ByteArray(v) => {
                let is_safe = v.iter().all(|x| *x >= 0x20 && *x <= 0x7e);
                if is_safe {
                    Ok(format!(
                        "b\"{}\"",
                        v.iter()
                            .map(|x| *x as char)
                            .collect::<String>()
                            .replace("\\", "\\\\")
                            .replace("\"", "\\\"")
                    ))
                } else {
                    Ok(format!(
                        "x\"{}\"",
                        v.iter()
                            .map(|x| format!("{:02x}", x))
                            .collect::<Vec<_>>()
                            .join(""),
                    ))
                }
            }
            Constant::AddressArray(v) => Ok(format!(
                "vector[{}]",
                v.iter()
                    .map(|x| Self::const_to_source(&Constant::Address(x.clone())))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", "),
            )),
            Constant::Vector(v) => Ok(format!(
                "vector[{}]",
                v.iter()
                    .map(|x| Self::const_to_source(x))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", "),
            )),
        }
    }
    pub fn to_source_decl(
        &self,
        naming: &Naming,
        need_syntactical_brackets: bool,
    ) -> Result<String, anyhow::Error> {
        match self {
            ExprNodeOperation::DatatypePack(name, args, types) => {
                if args.len() < 2 {
                    return self.to_source(naming, need_syntactical_brackets);
                }
                let k_width = args.iter().map(|x| x.0.len()).max().unwrap();
                Ok(format!(
                    "{}{}{{\n{},\n}}",
                    name,
                    Self::typeparams_to_source(types, naming),
                    args.iter()
                        .map(|x| x
                            .1
                            .borrow()
                            .to_source(naming, true)
                            .and_then(|v| Ok(format!("{:width$} : {}", x.0, v, width = k_width))))
                        .collect::<Result<Vec<_>, _>>()?
                        .join(", \n")
                ))
            }
            _ => self.to_source(naming, need_syntactical_brackets),
        }
    }
    pub fn to_source(
        &self,
        naming: &Naming,
        need_syntactical_brackets: bool,
    ) -> Result<String, anyhow::Error> {
        self.to_source_with_ctx(
            naming,
            &ToSourceCtx::default().with_syntactical_brackets(need_syntactical_brackets),
        )
    }
    fn to_source_with_ctx(
        &self,
        naming: &Naming,
        ctx: &ToSourceCtx,
    ) -> Result<String, anyhow::Error> {
        let mut ctx = ctx.clone();
        if ctx.in_borrow {
            match self {
                ExprNodeOperation::BorrowLocal(..) => {}
                ExprNodeOperation::Field(..) => {}
                _ => {
                    ctx.in_borrow = false;
                }
            }
        }
        let is_need_syntactical_brackets = ctx.need_syntactical_brackets;
        if is_need_syntactical_brackets
            && !matches!(self, ExprNodeOperation::VariableSnapshot { .. })
        {
            ctx.need_syntactical_brackets = false;
        }

        match self {
            ExprNodeOperation::LocalVariable(idx) => Ok(naming.variable(*idx)),
            ExprNodeOperation::Ignored => Ok("_".to_string()),
            ExprNodeOperation::Deleted => Ok("<<< !!! deleted !!! >>>".to_string()),
            ExprNodeOperation::NonTrivial => Ok("!!non-trivial!!".to_string()),
            ExprNodeOperation::Raw(x) => Ok(format!("((/*raw:*/{}))", x)),
            ExprNodeOperation::Const(c) => Self::const_to_source(c),
            ExprNodeOperation::Field(expr, name) => {
                // &(&object).field -> & object.field
                if ctx.in_borrow {
                    if let Some(r) = effective_operation(&[expr], &mut |[e]| -> Option<
                        Result<String, anyhow::Error>,
                    > {
                        let e = e.borrow();
                        if let ExprNodeOperation::BorrowLocal(inner_expr, _) = &e.operation {
                            let r = bracket_if_binary_with_ctx(inner_expr, Some(naming), &ctx);
                            match r {
                                std::result::Result::Ok(v) => {
                                    return Some(Ok(format!("{}.{}", v, name)))
                                }
                                Err(_) => return Some(r),
                            }
                        }
                        None
                    }) {
                        return r;
                    }
                }
                Ok(format!(
                    "{}.{}",
                    bracket_if_binary_with_ctx(expr, Some(naming), &ctx)?,
                    name
                ))
            }
            ExprNodeOperation::Unary(op, expr) => Ok(format!(
                "{}{}",
                op,
                bracket_if_binary_with_ctx(expr, Some(naming), &ctx)?
            )),
            ExprNodeOperation::Cast(ty, expr) => Ok(bracket_if(
                is_need_syntactical_brackets,
                format!(
                    "{} as {}",
                    bracket_if_binary_with_ctx(expr, Some(naming), &ctx)?,
                    ty
                ),
            )),
            ExprNodeOperation::Binary(op, a, b) => {
                let a_str = check_bracket_for_binary(a, get_precedence(op), Some(naming), &ctx)?;
                let b_str = check_bracket_for_binary(b, get_precedence(op), Some(naming), &ctx)?;
                Ok(format!("{} {} {}", a_str, op, b_str))
            }
            ExprNodeOperation::Func(name, args, types) => Ok(format!(
                "{}{}({})",
                name,
                Self::typeparams_to_source(types, naming),
                args.iter()
                    .map(|x| x
                        .borrow()
                        .to_source_with_ctx(naming, &ctx.with_syntactical_brackets(true)))
                    .collect::<Result<Vec<String>, anyhow::Error>>()?
                    .join(", ")
            )),
            ExprNodeOperation::Destroy(expr) => Ok(format!(
                "/*destroyed:{}*/",
                expr.borrow().to_source_with_ctx(naming, &ctx)?
            )),
            ExprNodeOperation::FreezeRef(expr) => expr.borrow().to_source_with_ctx(naming, &ctx),
            ExprNodeOperation::ReadRef(expr) => {
                effective_operation(&[expr], &mut |[expr]| match &expr.borrow().operation {
                    ExprNodeOperation::BorrowLocal(inner_expr, _) => {
                        // cleanup *&, *&mut
                        ctx.in_borrow = true;
                        Ok(format!(
                            "{}",
                            inner_expr.borrow().to_source_with_ctx(naming, &ctx)?
                        ))
                    }
                    _ => Ok(format!(
                        "*{}",
                        bracket_if_binary_with_ctx(expr, Some(naming), &ctx)?
                    )),
                })
            }
            ExprNodeOperation::BorrowLocal(expr, mutable) => {
                ctx.in_borrow = true;
                if *mutable {
                    Ok(format!(
                        "&mut {}",
                        expr.borrow().to_source_with_ctx(naming, &ctx)?
                    ))
                } else {
                    Ok(format!(
                        "&{}",
                        expr.borrow().to_source_with_ctx(naming, &ctx)?
                    ))
                }
            }
            ExprNodeOperation::WriteRef(lhs, rhs) => Ok(format!(
                "{} = {}",
                ExprNodeOperation::ReadRef(lhs.clone()).to_source_with_ctx(naming, &ctx)?,
                rhs.borrow()
                    .to_source_with_ctx(naming, &ctx.with_syntactical_brackets(true))?
            )),
            ExprNodeOperation::DatatypePack(name, args, types) => Ok(format!(
                "{}{}{{{}}}",
                name,
                Self::typeparams_to_source(types, naming),
                args.iter()
                    .map(|x| x
                        .1
                        .borrow()
                        .to_source_with_ctx(naming, &ctx.with_syntactical_brackets(true))
                        .and_then(|v| Ok(format!("{}: {}", x.0, v))))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ")
            )),
            ExprNodeOperation::DatatypeUnpack(name, keys, val, types) => Ok(format!(
                "{}{}{{{}}} = {}",
                name,
                Self::typeparams_to_source(types, naming),
                keys.iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
                val.borrow()
                    .to_source_with_ctx(naming, &ctx.with_syntactical_brackets(true))?
            )),
            ExprNodeOperation::VariableSnapshot { value, .. } => {
                value.borrow().to_source_with_ctx(naming, &ctx)
            }
        }
    }

    fn collect_variables(
        &self,
        result_variables: &mut HashMap<usize, usize>,
        implicit_variables: &mut HashMap<usize, usize>,
        in_implicit_expr: bool,
        collect_inside_implicit_expr: bool,
    ) {
        match self {
            ExprNodeOperation::LocalVariable(idx) => {
                if in_implicit_expr {
                    implicit_variables
                        .entry(*idx)
                        .and_modify(|x| *x += 1)
                        .or_insert(1);
                } else {
                    result_variables
                        .entry(*idx)
                        .and_modify(|x| *x += 1)
                        .or_insert(1);
                }
            }
            ExprNodeOperation::Ignored
            | ExprNodeOperation::Deleted
            | ExprNodeOperation::NonTrivial
            | ExprNodeOperation::Raw(..)
            | ExprNodeOperation::Const(..) => {}
            ExprNodeOperation::Field(expr, _) => expr.borrow().collect_variables(
                result_variables,
                implicit_variables,
                in_implicit_expr,
                collect_inside_implicit_expr,
            ),
            ExprNodeOperation::Unary(_, expr) => expr.borrow().collect_variables(
                result_variables,
                implicit_variables,
                in_implicit_expr,
                collect_inside_implicit_expr,
            ),
            ExprNodeOperation::Cast(_, expr) => expr.borrow().collect_variables(
                result_variables,
                implicit_variables,
                in_implicit_expr,
                collect_inside_implicit_expr,
            ),
            ExprNodeOperation::Binary(_, a, b) => {
                a.borrow().collect_variables(
                    result_variables,
                    implicit_variables,
                    in_implicit_expr,
                    collect_inside_implicit_expr,
                );
                b.borrow().collect_variables(
                    result_variables,
                    implicit_variables,
                    in_implicit_expr,
                    collect_inside_implicit_expr,
                );
            }
            ExprNodeOperation::Func(_, args, _) => {
                for arg in args {
                    arg.borrow().collect_variables(
                        result_variables,
                        implicit_variables,
                        in_implicit_expr,
                        collect_inside_implicit_expr,
                    );
                }
            }
            ExprNodeOperation::Destroy(expr)
            | ExprNodeOperation::FreezeRef(expr)
            | ExprNodeOperation::ReadRef(expr)
            | ExprNodeOperation::BorrowLocal(expr, _) => expr.borrow().collect_variables(
                result_variables,
                implicit_variables,
                in_implicit_expr,
                collect_inside_implicit_expr,
            ),
            ExprNodeOperation::WriteRef(lhs, rhs) => {
                lhs.borrow().collect_variables(
                    result_variables,
                    implicit_variables,
                    in_implicit_expr,
                    collect_inside_implicit_expr,
                );
                rhs.borrow().collect_variables(
                    result_variables,
                    implicit_variables,
                    in_implicit_expr,
                    collect_inside_implicit_expr,
                );
            }
            ExprNodeOperation::DatatypePack(_, args, _) => {
                for arg in args {
                    arg.1.borrow().collect_variables(
                        result_variables,
                        implicit_variables,
                        in_implicit_expr,
                        collect_inside_implicit_expr,
                    );
                }
            }
            ExprNodeOperation::DatatypeUnpack(_, _, val, _) => val.borrow().collect_variables(
                result_variables,
                implicit_variables,
                in_implicit_expr,
                collect_inside_implicit_expr,
            ),
            ExprNodeOperation::VariableSnapshot {
                variable, value, ..
            } => {
                implicit_variables
                    .entry(*variable)
                    .and_modify(|x| *x += 1)
                    .or_insert(1);
                if collect_inside_implicit_expr {
                    value.borrow().collect_variables(
                        result_variables,
                        implicit_variables,
                        in_implicit_expr,
                        collect_inside_implicit_expr,
                    );
                }
            }
        }
    }

    pub fn has_reference_to_any_variable(&self, variables: &HashSet<usize>) -> bool {
        match self {
            ExprNodeOperation::LocalVariable(idx) => variables.contains(idx),
            ExprNodeOperation::Ignored
            | ExprNodeOperation::Deleted
            | ExprNodeOperation::NonTrivial
            | ExprNodeOperation::Raw(..)
            | ExprNodeOperation::Const(..) => false,
            ExprNodeOperation::Field(expr, _) => expr
                .borrow()
                .operation
                .has_reference_to_any_variable(variables),
            ExprNodeOperation::Unary(_, expr) => expr
                .borrow()
                .operation
                .has_reference_to_any_variable(variables),
            ExprNodeOperation::Cast(_, expr) => expr
                .borrow()
                .operation
                .has_reference_to_any_variable(variables),
            ExprNodeOperation::Binary(_, a, b) => {
                a.borrow()
                    .operation
                    .has_reference_to_any_variable(variables)
                    || b.borrow()
                        .operation
                        .has_reference_to_any_variable(variables)
            }
            ExprNodeOperation::Func(_, args, _) => args.iter().any(|arg| {
                arg.borrow()
                    .operation
                    .has_reference_to_any_variable(variables)
            }),
            ExprNodeOperation::Destroy(expr)
            | ExprNodeOperation::FreezeRef(expr)
            | ExprNodeOperation::ReadRef(expr)
            | ExprNodeOperation::BorrowLocal(expr, _) => expr
                .borrow()
                .operation
                .has_reference_to_any_variable(variables),
            ExprNodeOperation::WriteRef(lhs, rhs) => {
                lhs.borrow()
                    .operation
                    .has_reference_to_any_variable(variables)
                    || rhs
                        .borrow()
                        .operation
                        .has_reference_to_any_variable(variables)
            }
            ExprNodeOperation::DatatypePack(_, args, _) => args.iter().any(|arg| {
                arg.1
                    .borrow()
                    .operation
                    .has_reference_to_any_variable(variables)
            }),
            ExprNodeOperation::DatatypeUnpack(_, _, val, _) => val
                .borrow()
                .operation
                .has_reference_to_any_variable(variables),
            ExprNodeOperation::VariableSnapshot {
                variable,
                assignment_id: _,
                value,
            } => {
                variables.contains(variable)
                    || value
                        .borrow()
                        .operation
                        .has_reference_to_any_variable(variables)
            }
        }
    }

    pub fn rename_variables(
        &mut self,
        renamed_variables: &HashMap<usize, usize>,
        check_map_full: bool,
    ) {
        match self {
            ExprNodeOperation::LocalVariable(idx) => {
                if renamed_variables.get(idx).is_none() {
                    if !check_map_full {
                        return;
                    }
                    panic!("Variable {} not found {:?}", idx, renamed_variables);
                }
                *idx = *renamed_variables.get(idx).unwrap();
            }
            ExprNodeOperation::NonTrivial => {
                panic!("NonTrivial should not be renamed");
            }
            ExprNodeOperation::Ignored
            | ExprNodeOperation::Deleted
            | ExprNodeOperation::Raw(..)
            | ExprNodeOperation::Const(..) => {}
            ExprNodeOperation::Binary(_, a, b) | ExprNodeOperation::WriteRef(a, b) => {
                a.borrow_mut()
                    .rename_variables(renamed_variables, check_map_full);
                b.borrow_mut()
                    .rename_variables(renamed_variables, check_map_full);
            }
            ExprNodeOperation::Func(_, args, _) => {
                for arg in args {
                    arg.borrow_mut()
                        .rename_variables(renamed_variables, check_map_full);
                }
            }
            ExprNodeOperation::DatatypePack(_, args, _) => {
                for arg in args {
                    arg.1
                        .borrow_mut()
                        .rename_variables(renamed_variables, check_map_full);
                }
            }
            ExprNodeOperation::DatatypeUnpack(_, _, val, _) => val
                .borrow_mut()
                .rename_variables(renamed_variables, check_map_full),
            ExprNodeOperation::Field(expr, _)
            | ExprNodeOperation::Unary(_, expr)
            | ExprNodeOperation::Cast(_, expr)
            | ExprNodeOperation::Destroy(expr)
            | ExprNodeOperation::FreezeRef(expr)
            | ExprNodeOperation::ReadRef(expr)
            | ExprNodeOperation::BorrowLocal(expr, _) => expr
                .borrow_mut()
                .rename_variables(renamed_variables, check_map_full),
            ExprNodeOperation::VariableSnapshot {
                variable, value, ..
            } => {
                value
                    .borrow_mut()
                    .rename_variables(renamed_variables, check_map_full);
                if renamed_variables.get(variable).is_none() {
                    if !check_map_full {
                        return;
                    }
                    panic!("Variable {} not found {:?}", variable, renamed_variables);
                }
                *variable = *renamed_variables.get(variable).unwrap();
            }
        }
    }

    fn commit_pending_variables(&self, variables: &HashSet<usize>) -> ExprNodeRef {
        match self {
            ExprNodeOperation::Ignored => self.to_node(),
            ExprNodeOperation::Deleted => self.to_node(),
            ExprNodeOperation::NonTrivial => self.to_node(),
            ExprNodeOperation::Raw(_) => self.to_node(),
            ExprNodeOperation::Const(_) => self.to_node(),
            ExprNodeOperation::LocalVariable(_) => self.to_node(),
            ExprNodeOperation::Field(expr, name) => ExprNodeOperation::Field(
                expr.borrow().commit_pending_variables(variables),
                name.clone(),
            )
            .to_node(),
            ExprNodeOperation::Unary(op, expr) => ExprNodeOperation::Unary(
                op.clone(),
                expr.borrow().commit_pending_variables(variables),
            )
            .to_node(),
            ExprNodeOperation::Cast(typ, expr) => ExprNodeOperation::Cast(
                typ.clone(),
                expr.borrow().commit_pending_variables(variables),
            )
            .to_node(),
            ExprNodeOperation::Binary(op, left, right) => ExprNodeOperation::Binary(
                op.clone(),
                left.borrow().commit_pending_variables(variables),
                right.borrow().commit_pending_variables(variables),
            )
            .to_node(),
            ExprNodeOperation::Func(name, args, typs) => ExprNodeOperation::Func(
                name.clone(),
                args.iter()
                    .map(|x| x.borrow().commit_pending_variables(variables))
                    .collect(),
                typs.clone(),
            )
            .to_node(),
            ExprNodeOperation::Destroy(expr) => {
                ExprNodeOperation::Destroy(expr.borrow().commit_pending_variables(variables))
                    .to_node()
            }
            ExprNodeOperation::FreezeRef(expr) => {
                ExprNodeOperation::FreezeRef(expr.borrow().commit_pending_variables(variables))
                    .to_node()
            }
            ExprNodeOperation::ReadRef(expr) => {
                ExprNodeOperation::ReadRef(expr.borrow().commit_pending_variables(variables))
                    .to_node()
            }
            ExprNodeOperation::BorrowLocal(expr, mutable) => ExprNodeOperation::BorrowLocal(
                expr.borrow().commit_pending_variables(variables),
                *mutable,
            )
            .to_node(),
            ExprNodeOperation::WriteRef(expr, expr2) => ExprNodeOperation::WriteRef(
                expr.borrow().commit_pending_variables(variables),
                expr2.borrow().commit_pending_variables(variables),
            )
            .to_node(),
            ExprNodeOperation::DatatypePack(name, fields, typs) => ExprNodeOperation::DatatypePack(
                name.clone(),
                fields
                    .iter()
                    .map(|x| {
                        (
                            x.0.clone(),
                            x.1.borrow().commit_pending_variables(variables),
                        )
                    })
                    .collect(),
                typs.clone(),
            )
            .to_node(),
            ExprNodeOperation::DatatypeUnpack(name, fields_names, expr, typs) => {
                ExprNodeOperation::DatatypeUnpack(
                    name.clone(),
                    fields_names.clone(),
                    expr.borrow().commit_pending_variables(variables),
                    typs.clone(),
                )
                .to_node()
            }
            ExprNodeOperation::VariableSnapshot {
                variable,
                assignment_id,
                value,
            } => {
                if variables.contains(variable) {
                    ExprNodeOperation::LocalVariable(*variable).to_node()
                } else {
                    ExprNodeOperation::VariableSnapshot {
                        variable: *variable,
                        assignment_id: *assignment_id,
                        value: value.borrow().commit_pending_variables(variables),
                    }
                    .to_node()
                }
            }
        }
    }
}

fn bracket_if(with_bracket: bool, inner: String) -> String {
    if with_bracket {
        format!("({})", inner)
    } else {
        inner
    }
}

fn get_precedence(operator: &str) -> u32 {
    match operator {
        // spec
        // "==>" => 1,
        // ":=" => 3,
        "||" => 5,
        "&&" => 10,

        "==" | "!=" => 15,

        "<" | ">" | "<=" | ">=" => 15,
        ".." => 20,
        "|" => 25,
        "^" => 30,
        "&" => 35,
        "<<" | ">>" => 40,
        "+" | "-" => 45,
        "*" | "/" | "%" => 50,

        _ => 0, // anything else is not a binary operator
    }
}

fn check_bracket_for_binary(
    expr: &ExprNodeRef,
    parent_precedence: u32,
    naming: Option<&Naming>,
    ctx: &ToSourceCtx,
) -> Result<String, anyhow::Error> {
    effective_operation(&[expr], &mut |&[expr]| {
        let expr_str = if let Some(naming) = naming {
            expr.borrow().to_source_with_ctx(naming, ctx)?
        } else {
            expr.borrow().to_string()
        };
        let inner_precedence = match &expr.borrow().operation {
            ExprNodeOperation::Binary(op, _, _) => get_precedence(op),
            ExprNodeOperation::Cast(..) => 3,
            _ => 1000,
        };
        Ok(if inner_precedence < parent_precedence {
            format!("({})", expr_str)
        } else {
            expr_str
        })
    })
}

fn bracket_if_binary_with_ctx(
    expr: &ExprNodeRef,
    naming: Option<&Naming>,
    ctx: &ToSourceCtx,
) -> Result<String, anyhow::Error> {
    effective_operation(&[expr], &mut |&[expr]| {
        let expr_str = if let Some(naming) = naming {
            expr.borrow().to_source_with_ctx(naming, ctx)?
        } else {
            expr.borrow().to_string()
        };
        Ok(match &expr.borrow().operation {
            ExprNodeOperation::Binary(..) => format!("({})", expr_str),
            ExprNodeOperation::Cast(..) => format!("({})", expr_str),
            _ => expr_str,
        })
    })
}

impl Display for ExprNodeOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExprNodeOperation::Deleted => write!(f, "<<< !!! deleted !!! >>>"),
            ExprNodeOperation::Ignored => write!(f, "_"),
            ExprNodeOperation::NonTrivial => write!(f, "!!non-trivial!!"),
            ExprNodeOperation::Raw(s) => write!(f, "((/*raw:*/{}))", s),
            ExprNodeOperation::Const(c) => write!(f, "{}", c),
            ExprNodeOperation::LocalVariable(idx) => write!(f, "_$local$_{}", idx),
            ExprNodeOperation::Unary(op, expr) => {
                write!(
                    f,
                    "{}{}",
                    op,
                    bracket_if_binary_with_ctx(expr, None, &ToSourceCtx::default()).unwrap()
                )
            }
            ExprNodeOperation::Cast(op, expr) => {
                write!(
                    f,
                    "{} as {}",
                    bracket_if_binary_with_ctx(expr, None, &ToSourceCtx::default()).unwrap(),
                    op
                )
            }
            ExprNodeOperation::BorrowLocal(expr, mutable) => {
                if *mutable {
                    write!(
                        f,
                        "&mut {}",
                        bracket_if_binary_with_ctx(expr, None, &ToSourceCtx::default()).unwrap()
                    )
                } else {
                    write!(
                        f,
                        "&{}",
                        bracket_if_binary_with_ctx(expr, None, &ToSourceCtx::default()).unwrap()
                    )
                }
            }
            ExprNodeOperation::Binary(op, a, b) => {
                let a_str =
                    check_bracket_for_binary(a, get_precedence(op), None, &ToSourceCtx::default())
                        .unwrap();
                let b_str =
                    check_bracket_for_binary(b, get_precedence(op), None, &ToSourceCtx::default())
                        .unwrap();
                write!(f, "{} {} {}", a_str, op, b_str)
            }
            // freezeref convert &mut to &, that typing is at variable declaration level so just ignore
            ExprNodeOperation::FreezeRef(expr) => write!(f, "{}", expr.borrow()),
            ExprNodeOperation::ReadRef(expr) => {
                write!(
                    f,
                    "*{}",
                    bracket_if_binary_with_ctx(expr, None, &ToSourceCtx::default()).unwrap()
                )
            }
            ExprNodeOperation::WriteRef(lhs, rhs) => {
                write!(
                    f,
                    "*{} = {}",
                    bracket_if_binary_with_ctx(lhs, None, &ToSourceCtx::default()).unwrap(),
                    rhs.borrow()
                )
            }
            ExprNodeOperation::Destroy(expr) => write!(f, "/*destroyed:{}*/", expr.borrow()),
            ExprNodeOperation::Field(expr, name) => {
                write!(
                    f,
                    "{}.{}",
                    bracket_if_binary_with_ctx(expr, None, &ToSourceCtx::default()).unwrap(),
                    name
                )
            }
            ExprNodeOperation::Func(name, args, typs) => {
                write!(
                    f,
                    "{}{}({})",
                    name,
                    if typs.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "<{}>",
                            typs.iter()
                                .map(|x| format!("{:?}", x))
                                .collect::<Vec<String>>()
                                .join(", ")
                        )
                    },
                    args.iter()
                        .map(|x| x.borrow().to_string())
                        .collect::<Vec<String>>()
                        .join(", ")
                )
            }
            ExprNodeOperation::DatatypePack(name, args, types) => {
                write!(
                    f,
                    "{}{}{{{}}}",
                    name,
                    if types.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "<{}>",
                            types
                                .iter()
                                .map(|x| format!("{:?}", x))
                                .collect::<Vec<String>>()
                                .join(", ")
                        )
                    },
                    args.iter()
                        .map(|x| format!("{}: {}", x.0, x.1.borrow().to_string()))
                        .collect::<Vec<String>>()
                        .join(", ")
                )
            }
            ExprNodeOperation::DatatypeUnpack(name, keys, val, types) => {
                write!(
                    f,
                    "{}{}{{{}}} = {}",
                    name,
                    if types.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "<{}>",
                            types
                                .iter()
                                .map(|x| format!("{:?}", x))
                                .collect::<Vec<String>>()
                                .join(", ")
                        )
                    },
                    keys.iter()
                        .map(|x| x.to_string())
                        .collect::<Vec<String>>()
                        .join(", "),
                    val.borrow().to_string()
                )
            }
            ExprNodeOperation::VariableSnapshot {
                variable,
                assignment_id,
                value,
            } => {
                write!(
                    f,
                    "/*snapshot:{}:{}*/{}",
                    variable,
                    assignment_id,
                    value.borrow().to_string()
                )
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct ExprNode {
    pub(crate) operation: ExprNodeOperation,
}

impl ExprNode {
    pub fn rename_variables(
        &mut self,
        renamed_variables: &HashMap<usize, usize>,
        check_map_full: bool,
    ) {
        self.operation
            .rename_variables(renamed_variables, check_map_full);
    }
    pub fn copy_as_ref(&self) -> ExprNodeRef {
        Rc::new(RefCell::new(Self {
            operation: self.operation.copy(),
        }))
    }

    pub fn to_source(
        &self,
        naming: &Naming,
        need_syntactical_brackets: bool,
    ) -> Result<String, anyhow::Error> {
        self.operation.to_source(naming, need_syntactical_brackets)
    }

    fn to_source_with_ctx(
        &self,
        naming: &Naming,
        ctx: &ToSourceCtx,
    ) -> Result<String, anyhow::Error> {
        self.operation.to_source_with_ctx(naming, ctx)
    }

    pub fn to_source_decl(
        &self,
        naming: &Naming,
        need_syntactical_brackets: bool,
    ) -> Result<String, anyhow::Error> {
        self.operation
            .to_source_decl(naming, need_syntactical_brackets)
    }

    pub fn collect_variables(
        &self,
        result_variables: &mut HashMap<usize, usize>,
        implicit_variables: &mut HashMap<usize, usize>,
        in_implicit_expr: bool,
        collect_inside_implicit_expr: bool,
    ) {
        self.operation.collect_variables(
            result_variables,
            implicit_variables,
            in_implicit_expr,
            collect_inside_implicit_expr,
        );
    }

    pub fn commit_pending_variables(&self, variables: &HashSet<usize>) -> ExprNodeRef {
        self.operation.commit_pending_variables(variables)
    }

    pub(crate) fn is_single_variable(&self) -> Option<usize> {
        match &self.operation {
            ExprNodeOperation::LocalVariable(idx) => Some(*idx),
            ExprNodeOperation::VariableSnapshot { value, .. } => {
                value.borrow().is_single_variable()
            }
            _ => None,
        }
    }
}

impl Display for ExprNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.operation.fmt(f)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    node: ExprNodeRef,
}

pub struct VariablesInfo {
    pub variables: HashMap<usize, usize>,
    pub implicit_variables: HashMap<usize, usize>,
}
impl VariablesInfo {
    pub fn any_variables(&self) -> HashSet<usize> {
        self.variables
            .keys()
            .collect::<HashSet<_>>()
            .union(&self.implicit_variables.keys().collect::<HashSet<_>>())
            .map(|x| **x)
            .collect()
    }
}

impl Expr {
    pub fn new(node: ExprNodeRef) -> Self {
        Self { node }
    }

    pub fn rename_variables(
        &mut self,
        renamed_variables: &HashMap<usize, usize>,
        check_map_full: bool,
    ) {
        self.node
            .borrow_mut()
            .rename_variables(renamed_variables, check_map_full);
    }

    pub fn non_trivial() -> Self {
        Self {
            node: ExprNodeOperation::NonTrivial.to_node(),
        }
    }

    pub fn is_non_trivial(&self) -> bool {
        match &self.node.borrow().operation {
            ExprNodeOperation::NonTrivial => true,
            _ => false,
        }
    }

    pub fn is_flushed(&self) -> bool {
        match &self.node.borrow().operation {
            ExprNodeOperation::Raw(..) => true,
            ExprNodeOperation::LocalVariable(..) => true,
            _ => false,
        }
    }

    pub fn copy(&self) -> Self {
        Self {
            node: self.value_copied(),
        }
    }

    pub fn value(&self) -> &ExprNodeRef {
        &self.node
    }

    pub fn value_copied(&self) -> ExprNodeRef {
        self.node.borrow().copy_as_ref()
    }

    pub fn ignored() -> Expr {
        Expr::new(ExprNodeOperation::Ignored.to_node())
    }

    #[allow(dead_code)]
    pub fn deleted() -> Expr {
        Expr::new(ExprNodeOperation::Deleted.to_node())
    }

    pub fn to_source(
        &self,
        naming: &Naming,
        need_syntactical_brackets: bool,
    ) -> Result<String, anyhow::Error> {
        self.node
            .borrow()
            .to_source(naming, need_syntactical_brackets)
    }

    pub fn to_source_decl(
        &self,
        naming: &Naming,
        need_syntactical_brackets: bool,
    ) -> Result<String, anyhow::Error> {
        self.node
            .borrow()
            .to_source_decl(naming, need_syntactical_brackets)
    }

    pub fn commit_pending_variables(&self, variables: &HashSet<usize>) -> Expr {
        Expr::new(self.node.borrow().commit_pending_variables(variables))
    }

    pub(crate) fn should_ignore(&self) -> bool {
        match &self.node.borrow().operation {
            ExprNodeOperation::Destroy(..) => true,
            _ => false,
        }
    }

    pub(crate) fn collect_variables_with_count(
        &self,
        in_implicit_expr: bool,
        collect_inside_implicit_expr: bool,
    ) -> VariablesInfo {
        let mut result_variables = HashMap::new();
        let mut implicit_variables = HashMap::new();
        self.node.borrow().collect_variables(
            &mut result_variables,
            &mut implicit_variables,
            in_implicit_expr,
            collect_inside_implicit_expr,
        );
        VariablesInfo {
            variables: result_variables,
            implicit_variables,
        }
    }

    pub(crate) fn is_single_variable(&self) -> Option<usize> {
        self.node.borrow().is_single_variable()
    }

    pub(crate) fn has_reference_to_any_variable(&self, variables: &HashSet<usize>) -> bool {
        self.node
            .borrow()
            .operation
            .has_reference_to_any_variable(variables)
    }
}

impl Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.node.borrow().fmt(f)
    }
}
