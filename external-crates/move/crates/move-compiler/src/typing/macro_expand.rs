// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    diagnostics::Diagnostic,
    expansion::ast::ModuleIdent,
    naming::ast::{self as N, BlockLabel, Color, TParamID, Type, Type_, UseFuns, Var, Var_},
    parser::ast::FunctionName,
    shared::program_info::FunctionInfo,
    typing::ast as T,
    typing::core::{self, TParamSubst},
};
use move_ir_types::location::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

type LambdaMap = BTreeMap<Var_, (N::Lambda, Type, Type)>;

struct Context<'a, 'b> {
    core: &'a mut core::Context<'b>,
    lambdas: LambdaMap,
    tparam_subst: TParamSubst,
}

pub struct ExpandedMacro {
    pub argument_bindings: Vec<(Option<Loc>, Var, N::Type, T::Exp)>,
    pub body: N::Exp,
}

pub(crate) fn call(
    context: &mut core::Context,
    call_loc: Loc,
    m: ModuleIdent,
    f: FunctionName,
    type_args: Vec<N::Type>,
    sp!(_, args): Spanned<Vec<T::Exp>>,
) -> Option<ExpandedMacro> {
    let next_color = context.next_variable_color();
    // If none, there is no body to expand, likely because of an error in the macro definition
    let macro_body = context.macro_body(&m, &f)?;
    let macro_info = context.function_info(&m, &f);
    let (macro_type_params, macro_params, mut macro_body, return_label) =
        match recolor_macro(call_loc, &m, &f, macro_info, macro_body, next_color) {
            Ok(res) => res,
            Err(None) => {
                assert!(context.env.has_errors());
                return None;
            }
            Err(Some(diag)) => {
                context.env.add_diag(*diag);
                return None;
            }
        };

    if macro_type_params.len() != type_args.len() || macro_params.len() != args.len() {
        assert!(context.env.has_errors());
        return None;
    }
    // make tparam subst
    let tparam_subst = macro_type_params.into_iter().zip(type_args).collect();
    // make lambda map and bind non-lambda args to local vars
    let mut lambdas = BTreeMap::new();
    let mut argument_bindings = Vec::new();
    for ((mut_, param, param_ty), arg) in macro_params.into_iter().zip(args) {
        let param_ty = core::subst_tparams(&tparam_subst, param_ty);
        if let sp!(loc, Type_::Fun(param_tys, result_ty)) = param_ty {
            let param_tys = Type_::multiple(loc, param_tys);
            bind_lambda(
                context,
                &mut lambdas,
                param.value,
                arg,
                param_tys,
                *result_ty,
            )?;
        } else {
            argument_bindings.push((mut_, param, param_ty, arg))
        }
    }
    let mut break_labels: BTreeSet<_> = lambdas.values().map(|(l, _, _)| l.break_label).collect();
    break_labels.insert(return_label);
    let mut context = Context {
        core: context,
        lambdas,
        tparam_subst,
    };
    seq(&mut context, &mut macro_body);
    let mut wrapped_body = sp(call_loc, N::Exp_::Block(macro_body));
    for label in break_labels {
        let seq = (
            N::UseFuns::new(next_color),
            VecDeque::from([sp(call_loc, N::SequenceItem_::Seq(wrapped_body))]),
        );
        wrapped_body = sp(call_loc, N::Exp_::NamedBlock(label, seq));
    }
    Some(ExpandedMacro {
        argument_bindings,
        body: wrapped_body,
    })
}

fn recolor_macro(
    call_loc: Loc,
    _m: &ModuleIdent,
    _f: &FunctionName,
    macro_info: &FunctionInfo,
    macro_body: &N::Sequence,
    color: u16,
) -> Result<
    (
        Vec<TParamID>,
        Vec<(Option<Loc>, Var, N::Type)>,
        N::Sequence,
        BlockLabel,
    ),
    Option<Box<Diagnostic>>,
> {
    let FunctionInfo {
        macro_, signature, ..
    } = macro_info;
    if macro_.is_none() {
        // error handled in call type checking
        return Err(None);
    }
    let N::FunctionSignature {
        type_parameters,
        parameters,
        ..
    } = signature;
    let tparam_ids = type_parameters.iter().map(|t| t.id).collect();
    let return_label = BlockLabel(sp(
        call_loc,
        N::Var_ {
            name: N::BlockLabel::MACRO_RETURN_NAME_SYMBOL,
            id: 0,
            color,
        },
    ));
    let recolor_use_funs = true;
    let recolor = &mut Recolor::new(color, Some(return_label), recolor_use_funs);
    recolor.add_params(&parameters);
    let parameters = parameters
        .iter()
        .map(|(mut_, v, t)| (*mut_, recolor_var_owned(recolor, *v), t.clone()))
        .collect();
    let body = {
        let mut body = macro_body.clone();
        recolor_seq(recolor, &mut body);
        body
    };
    Ok((tparam_ids, parameters, body, return_label))
}

fn bind_lambda(
    context: &mut core::Context,
    lambdas: &mut LambdaMap,
    param: Var_,
    arg: T::Exp,
    param_ty: Type,
    result_ty: Type,
) -> Option<()> {
    match arg.exp.value {
        T::UnannotatedExp_::Annotate(inner, _) => {
            bind_lambda(context, lambdas, param, *inner, param_ty, result_ty)
        }
        T::UnannotatedExp_::Lambda(lambda) => {
            lambdas.insert(param, (lambda, param_ty, result_ty));
            Some(())
        }
        _ => {
            let msg = format!(
                "Unable to bind lambda to parameter '{}'. The lambda must be passed directly",
                param.name
            );
            context
                .env
                .add_diag(diag!(TypeSafety::CannotExpandMacro, (arg.exp.loc, msg)));
            None
        }
    }
}

//**************************************************************************************************
// recolor
//**************************************************************************************************

// The mask is here to make sure we do not recolor captured variables/labels. So we don't need to
// generally care about scoping in the normal way, since that should already be handled by the
// unique-ing of variables done by naming
struct Recolor {
    color: Color,
    recolor_use_funs: bool,
    return_label: Option<BlockLabel>,
    vars: BTreeSet<Var>,
    block_labels: BTreeSet<BlockLabel>,
}

impl Recolor {
    pub fn new(color: u16, return_label: Option<BlockLabel>, recolor_use_funs: bool) -> Self {
        Self {
            color,
            recolor_use_funs,
            return_label,
            vars: BTreeSet::new(),
            block_labels: BTreeSet::new(),
        }
    }

    pub fn add_params(&mut self, params: &[(Option<Loc>, Var, N::Type)]) {
        for (_, v, _) in params {
            self.vars.insert(*v);
        }
    }

    pub fn add_lvalues(&mut self, lvalues: &N::LValueList) {
        for lvalue in &lvalues.value {
            self.add_lvalue(lvalue)
        }
    }

    pub fn add_lvalue(&mut self, sp!(_, lvalue_): &N::LValue) {
        match lvalue_ {
            N::LValue_::Ignore => (),
            N::LValue_::Var { var, .. } => {
                self.vars.insert(*var);
            }
            N::LValue_::Unpack(_, _, _, lvalues) => {
                for (_, _, (_, lvalue)) in lvalues {
                    self.add_lvalue(lvalue)
                }
            }
        }
    }

    pub fn add_block_label(&mut self, label: BlockLabel) {
        self.block_labels.insert(label);
    }
}

fn recolor_var_owned(ctx: &mut Recolor, mut v: Var) -> Var {
    assert!(ctx.vars.contains(&v));
    recolor_var(ctx, &mut v);
    v
}

fn recolor_var(ctx: &mut Recolor, v: &mut Var) {
    // do not recolor if not in the ctx
    // this is to handle captured variables in lambda bodies
    if !ctx.vars.contains(v) {
        return;
    }
    v.value.color = ctx.color;
}

fn recolor_block_label_owned(ctx: &mut Recolor, mut label: BlockLabel) -> BlockLabel {
    assert!(ctx.block_labels.contains(&label));
    recolor_block_label(ctx, &mut label);
    label
}

fn recolor_block_label(ctx: &mut Recolor, label: &mut BlockLabel) {
    // do not recolor if not in the ctx
    // this is to handle captured labels in lambda bodies
    if !ctx.block_labels.contains(label) {
        return;
    }
    label.0.value.color = ctx.color;
}

fn recolor_use_funs(ctx: &mut Recolor, use_funs: &mut UseFuns) {
    recolor_use_funs_(ctx, &mut use_funs.color);
}

fn recolor_use_funs_(ctx: &mut Recolor, use_fun_color: &mut Color) {
    if ctx.recolor_use_funs {
        *use_fun_color = ctx.color
    }
}

fn recolor_seq(ctx: &mut Recolor, (use_funs, seq): &mut N::Sequence) {
    recolor_use_funs(ctx, use_funs);
    for sp!(_, item_) in seq {
        match item_ {
            N::SequenceItem_::Seq(e) => recolor_exp(ctx, e),
            N::SequenceItem_::Declare(lvalues, _) => recolor_lvalues(ctx, lvalues),
            N::SequenceItem_::Bind(lvalues, e) => {
                recolor_lvalues(ctx, lvalues);
                recolor_exp(ctx, e)
            }
        }
    }
}

fn recolor_lvalues(ctx: &mut Recolor, lvalues: &mut N::LValueList) {
    ctx.add_lvalues(lvalues);
    for lvalue in &mut lvalues.value {
        recolor_lvalue(ctx, lvalue)
    }
}

fn recolor_lvalue(ctx: &mut Recolor, sp!(_, lvalue_): &mut N::LValue) {
    match lvalue_ {
        N::LValue_::Ignore => (),
        N::LValue_::Var { var, .. } => recolor_var(ctx, var),
        N::LValue_::Unpack(_, _, _, lvalues) => {
            for (_, _, (_, lvalue)) in lvalues {
                recolor_lvalue(ctx, lvalue)
            }
        }
    }
}

fn recolor_exp(ctx: &mut Recolor, sp!(_, e_): &mut N::Exp) {
    match e_ {
        N::Exp_::Value(_) | N::Exp_::Constant(_, _) => (),
        N::Exp_::Give(label, e) => {
            recolor_block_label(ctx, label);
            recolor_exp(ctx, e)
        }
        N::Exp_::Continue(label) => recolor_block_label(ctx, label),
        N::Exp_::Unit { .. } | N::Exp_::UnresolvedError => (),
        N::Exp_::Var(var) => recolor_var(ctx, var),
        N::Exp_::Return(e) => {
            recolor_exp(ctx, e);
            if let Some(label) = ctx.return_label {
                let N::Exp_::Return(e) =
                    std::mem::replace(e_, /* dummy */ N::Exp_::UnresolvedError)
                else {
                    unreachable!()
                };
                *e_ = N::Exp_::Give(label, e)
            }
        }

        N::Exp_::Abort(e)
        | N::Exp_::Dereference(e)
        | N::Exp_::UnaryExp(_, e)
        | N::Exp_::Cast(e, _)
        | N::Exp_::Annotate(e, _) => recolor_exp(ctx, e),
        N::Exp_::Assign(lvalues, e) => {
            recolor_lvalues(ctx, lvalues);
            recolor_exp(ctx, e)
        }
        N::Exp_::IfElse(econd, et, ef) => {
            recolor_exp(ctx, econd);
            recolor_exp(ctx, et);
            recolor_exp(ctx, ef);
        }
        N::Exp_::Loop(name, e) => {
            ctx.add_block_label(*name);
            recolor_block_label(ctx, name);
            recolor_exp(ctx, e)
        }
        N::Exp_::While(econd, name, ebody) => {
            ctx.add_block_label(*name);
            recolor_block_label(ctx, name);
            recolor_exp(ctx, econd);
            recolor_exp(ctx, ebody)
        }
        N::Exp_::Block(s) => recolor_seq(ctx, s),
        N::Exp_::NamedBlock(name, s) => {
            ctx.add_block_label(*name);
            recolor_block_label(ctx, name);
            recolor_seq(ctx, s)
        }
        N::Exp_::FieldMutate(ed, e) => {
            recolor_exp_dotted(ctx, ed);
            recolor_exp(ctx, e)
        }
        N::Exp_::Mutate(el, er) | N::Exp_::BinopExp(el, _, er) => {
            recolor_exp(ctx, el);
            recolor_exp(ctx, er)
        }
        N::Exp_::Pack(_, _, _, fields) => {
            for (_, _, (_, e)) in fields {
                recolor_exp(ctx, e)
            }
        }
        N::Exp_::Builtin(_, sp!(_, es))
        | N::Exp_::Vector(_, _, sp!(_, es))
        | N::Exp_::ModuleCall(_, _, _, _, sp!(_, es))
        | N::Exp_::ExpList(es) => {
            for e in es {
                recolor_exp(ctx, e)
            }
        }
        N::Exp_::MethodCall(ed, _, _, _, sp!(_, es)) => {
            recolor_exp_dotted(ctx, ed);
            for e in es {
                recolor_exp(ctx, e)
            }
        }
        N::Exp_::VarCall(v, sp!(_, es)) => {
            recolor_var(ctx, v);
            for e in es {
                recolor_exp(ctx, e)
            }
        }

        N::Exp_::Lambda(N::Lambda {
            parameters,
            break_label,
            return_label,
            use_fun_color,
            body,
        }) => {
            ctx.add_block_label(*break_label);
            ctx.add_block_label(*return_label);
            recolor_use_funs_(ctx, use_fun_color);
            recolor_lvalues(ctx, parameters);
            recolor_block_label(ctx, break_label);
            recolor_block_label(ctx, return_label);
            recolor_exp(ctx, body)
        }
        N::Exp_::ExpDotted(_dotted_usage, ed) => recolor_exp_dotted(ctx, ed),
    }
}

fn recolor_exp_dotted(ctx: &mut Recolor, sp!(_, ed_): &mut N::ExpDotted) {
    match ed_ {
        N::ExpDotted_::Exp(e) => recolor_exp(ctx, e),
        N::ExpDotted_::Dot(ed, _) => recolor_exp_dotted(ctx, ed),
    }
}

//**************************************************************************************************
// recolor
//**************************************************************************************************

fn types(context: &mut Context, tys: &mut [Type]) {
    for ty in tys {
        type_(context, ty)
    }
}

fn type_(context: &mut Context, ty: &mut N::Type) {
    *ty = core::subst_tparams(&context.tparam_subst, ty.clone())
}

fn seq(context: &mut Context, (_use_funs, seq): &mut N::Sequence) {
    for sp!(_, item_) in seq {
        match item_ {
            N::SequenceItem_::Seq(e) => exp(context, e),
            N::SequenceItem_::Declare(lvs, _) => lvalues(context, lvs),
            N::SequenceItem_::Bind(lvs, e) => {
                lvalues(context, lvs);
                exp(context, e)
            }
        }
    }
}

fn lvalues(context: &mut Context, sp!(_, lvs_): &mut N::LValueList) {
    for lv in lvs_ {
        lvalue(context, lv)
    }
}

fn lvalue(context: &mut Context, sp!(_, lv_): &mut N::LValue) {
    match lv_ {
        N::LValue_::Ignore | N::LValue_::Var { .. } => (),
        N::LValue_::Unpack(_, _, tys_opt, lvalues) => {
            if let Some(tys) = tys_opt {
                types(context, tys)
            }
            for (_, _, (_, lv)) in lvalues {
                lvalue(context, lv)
            }
        }
    }
}

fn exp(context: &mut Context, sp!(_, e_): &mut N::Exp) {
    match e_ {
        N::Exp_::Value(_)
        | N::Exp_::Constant(_, _)
        | N::Exp_::Continue(_)
        | N::Exp_::Unit { .. }
        | N::Exp_::UnresolvedError
        | N::Exp_::Var(_) => (),
        N::Exp_::Give(_, e)
        | N::Exp_::Return(e)
        | N::Exp_::Abort(e)
        | N::Exp_::Dereference(e)
        | N::Exp_::UnaryExp(_, e)
        | N::Exp_::Loop(_, e) => exp(context, e),
        N::Exp_::Cast(e, ty) | N::Exp_::Annotate(e, ty) => {
            exp(context, e);
            type_(context, ty)
        }
        N::Exp_::Assign(lvs, e) => {
            lvalues(context, lvs);
            exp(context, e)
        }
        N::Exp_::IfElse(econd, et, ef) => {
            exp(context, econd);
            exp(context, et);
            exp(context, ef);
        }
        N::Exp_::While(econd, _name, ebody) => {
            exp(context, econd);
            exp(context, ebody)
        }
        N::Exp_::NamedBlock(_, s) | N::Exp_::Block(s) => seq(context, s),
        N::Exp_::FieldMutate(ed, e) => {
            exp_dotted(context, ed);
            exp(context, e)
        }
        N::Exp_::Mutate(el, er) | N::Exp_::BinopExp(el, _, er) => {
            exp(context, el);
            exp(context, er)
        }
        N::Exp_::Pack(_, _, tys_opt, fields) => {
            if let Some(tys) = tys_opt {
                types(context, tys)
            }
            for (_, _, (_, e)) in fields {
                exp(context, e)
            }
        }
        N::Exp_::Builtin(_, sp!(_, es)) => exps(context, es),
        N::Exp_::Vector(_, ty_opt, sp!(_, es)) => {
            if let Some(ty) = ty_opt {
                type_(context, ty)
            }
            exps(context, es)
        }
        N::Exp_::ModuleCall(_, _, _, tys_opt, sp!(_, es)) => {
            if let Some(tys) = tys_opt {
                types(context, tys)
            }
            exps(context, es)
        }
        N::Exp_::MethodCall(ed, _, _, _, sp!(_, es)) => {
            exp_dotted(context, ed);
            exps(context, es)
        }
        N::Exp_::ExpList(es) => exps(context, es),
        N::Exp_::Lambda(N::Lambda {
            parameters: lvs,
            body: e,
            ..
        }) => {
            lvalues(context, lvs);
            exp(context, e)
        }
        N::Exp_::ExpDotted(_usage, ed) => exp_dotted(context, ed),
        N::Exp_::VarCall(v, sp!(_, es)) if context.lambdas.contains_key(&v.value) => {
            exps(context, es);
            // param_ty and result_ty have already been substituted
            let (
                N::Lambda {
                    parameters: mut lambda_params,
                    break_label: _, // not used at this level
                    return_label,
                    use_fun_color,
                    body: mut lambda_body,
                },
                param_ty,
                result_ty,
            ) = context.lambdas.get(&v.value).unwrap().clone();
            // recolor in case the lambda is used more than once
            let next_color = context.core.next_variable_color();
            let recolor_use_funs = false;
            let recolor = &mut Recolor::new(
                next_color,
                /* return already labeled */ None,
                recolor_use_funs,
            );
            recolor.add_block_label(return_label);
            recolor.add_lvalues(&lambda_params);
            let return_label = recolor_block_label_owned(recolor, return_label);
            recolor_lvalues(recolor, &mut lambda_params);
            recolor_exp(recolor, &mut lambda_body);
            let param_loc = lambda_params.loc;
            let N::Exp_::VarCall(_, sp!(args_loc, arg_list)) =
                std::mem::replace(e_, /* dummy */ N::Exp_::UnresolvedError)
            else {
                unreachable!()
            };
            let args = sp(args_loc, N::Exp_::ExpList(arg_list));
            let annot_args = sp(args_loc, N::Exp_::Annotate(Box::new(args), param_ty));
            let body_loc = lambda_body.loc;
            let annot_body = sp(body_loc, N::Exp_::Annotate(lambda_body, result_ty));
            let labeled_seq = VecDeque::from([sp(body_loc, N::SequenceItem_::Seq(annot_body))]);
            let labeled_body = sp(
                body_loc,
                N::Exp_::NamedBlock(return_label, (N::UseFuns::new(use_fun_color), labeled_seq)),
            );
            let result = VecDeque::from([
                sp(param_loc, N::SequenceItem_::Bind(lambda_params, annot_args)),
                sp(body_loc, N::SequenceItem_::Seq(labeled_body)),
            ]);
            let current_use_fun_color = context
                .core
                .current_use_fun_color()
                .expect("ICE cannot expand a macro in the global env");
            *e_ = N::Exp_::Block((N::UseFuns::new(current_use_fun_color), result));
        }
        N::Exp_::VarCall(_, sp!(_, es)) => exps(context, es),
    }
}

fn exp_dotted(context: &mut Context, sp!(_, ed_): &mut N::ExpDotted) {
    match ed_ {
        N::ExpDotted_::Exp(e) => exp(context, e),
        N::ExpDotted_::Dot(ed, _) => exp_dotted(context, ed),
    }
}

fn exps(context: &mut Context, es: &mut [N::Exp]) {
    for e in es {
        exp(context, e)
    }
}
