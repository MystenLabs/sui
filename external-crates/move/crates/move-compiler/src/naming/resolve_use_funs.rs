// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::diagnostics::warning_filters::WarningFilters;
use crate::diagnostics::{Diagnostic, DiagnosticReporter, Diagnostics};
use crate::expansion::ast::{self as E, ModuleIdent};
use crate::naming::ast as N;
use crate::parser::ast::{DocComment, FunctionName, Visibility};
use crate::shared::{program_info::NamingProgramInfo, unique_map::UniqueMap, *};
use crate::typing::core;
use crate::{diag, ice};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;

//**************************************************************************************************
// Entry
//**************************************************************************************************

struct Context<'env, 'info> {
    env: &'env CompilationEnv,
    reporter: DiagnosticReporter<'env>,
    info: &'info NamingProgramInfo,
    current_module: ModuleIdent,
}

impl<'env, 'info> Context<'env, 'info> {
    fn new(
        env: &'env CompilationEnv,
        info: &'info NamingProgramInfo,
        current_module: ModuleIdent,
    ) -> Self {
        let reporter = env.diagnostic_reporter_at_top_level();
        Self {
            env,
            reporter,
            info,
            current_module,
        }
    }

    pub fn add_diag(&self, diag: Diagnostic) {
        self.reporter.add_diag(diag);
    }

    #[allow(unused)]
    pub fn add_diags(&self, diags: Diagnostics) {
        self.reporter.add_diags(diags);
    }

    pub fn push_warning_filter_scope(&mut self, filters: WarningFilters) {
        self.reporter.push_warning_filter_scope(filters)
    }

    pub fn pop_warning_filter_scope(&mut self) {
        self.reporter.pop_warning_filter_scope()
    }
}

//**************************************************************************************************
// Entry
//**************************************************************************************************

pub fn program(env: &CompilationEnv, info: &mut NamingProgramInfo, inner: &mut N::Program_) {
    let N::Program_ { modules } = inner;
    for (mident, mdef) in modules.key_cloned_iter_mut() {
        module(env, info, mident, mdef);
    }
    let module_use_funs = modules
        .key_cloned_iter()
        .map(|(mident, mdef)| {
            let N::UseFuns {
                resolved,
                implicit_candidates,
                color: _,
            } = &mdef.use_funs;
            assert!(implicit_candidates.is_empty());
            (mident, resolved.clone())
        })
        .collect();
    info.set_use_funs(module_use_funs);
}

fn module(
    env: &CompilationEnv,
    info: &mut NamingProgramInfo,
    mident: ModuleIdent,
    mdef: &mut N::ModuleDefinition,
) {
    let context = &mut Context::new(env, info, mident);
    context.push_warning_filter_scope(mdef.warning_filter);
    use_funs(context, &mut mdef.use_funs);
    for (_, _, c) in &mut mdef.constants {
        constant(context, c);
    }
    for (_, _, f) in &mut mdef.functions {
        function(context, f);
    }
    context.pop_warning_filter_scope();
}

fn constant(context: &mut Context, c: &mut N::Constant) {
    context.push_warning_filter_scope(c.warning_filter);
    exp(context, &mut c.value);
    context.pop_warning_filter_scope();
}

fn function(context: &mut Context, function: &mut N::Function) {
    context.push_warning_filter_scope(function.warning_filter);
    if let N::FunctionBody_::Defined(seq) = &mut function.body.value {
        sequence(context, seq)
    }
    context.pop_warning_filter_scope();
}

//**************************************************************************************************
// Resolution
//**************************************************************************************************

fn use_funs(context: &mut Context, uf: &mut N::UseFuns) {
    let N::UseFuns {
        resolved,
        implicit_candidates,
        color: _,
    } = uf;
    // remove any incorrect resolved functions
    for (tn, methods) in &mut *resolved {
        *methods = std::mem::take(methods).filter_map(|method, mut nuf| {
            let loc = nuf.loc;
            let (m, f) = nuf.target_function;
            let kind = nuf.kind;
            assert!(
                kind == N::UseFunKind::Explicit,
                "ICE all resolved use funs should be explicit at this stage. kind {kind:?}"
            );
            let (first_ty_loc, first_ty) = first_arg_type(context, &m, &f);
            let is_valid = match first_ty
                .as_ref()
                .and_then(|ty| ty.value.unfold_to_type_name())
            {
                Some(first_tn) => first_tn == tn,
                None => false,
            };
            if is_valid {
                if let Some(public_loc) = nuf.is_public {
                    let defining_module = match &tn.value {
                        N::TypeName_::Multiple(_) => {
                            context.add_diag(ice!((
                                tn.loc,
                                "ICE tuple type should not be reachable from use fun"
                            )));
                            return None;
                        }
                        N::TypeName_::Builtin(sp!(_, bt_)) => context.env.primitive_definer(*bt_),
                        N::TypeName_::ModuleType(m, _) => Some(m),
                    };
                    if Some(&context.current_module) != defining_module {
                        let msg = "Invalid visibility for 'use fun' declaration";
                        let vis_msg = format!(
                            "Module level 'use fun' declarations can be '{}' for the \
                            module's types, otherwise they must be internal to the declared scope.",
                            Visibility::PUBLIC
                        );
                        let mut diag = diag!(
                            Declarations::InvalidUseFun,
                            (loc, msg),
                            (public_loc, vis_msg)
                        );
                        if let Some(m) = defining_module {
                            diag.add_secondary_label((
                                m.loc,
                                format!("The type '{tn}' is defined here"),
                            ))
                        }
                        context.add_diag(diag);
                        nuf.is_public = None;
                    }
                }
                Some(nuf)
            } else {
                let msg = format!(
                    "Invalid 'use fun' for '{tn}.{method}'. \
                    Expected a '{tn}' type as the first argument \
                    (either by reference '&' '&mut' or by value)",
                );
                let first_tn_msg = match first_ty {
                    Some(ty) => {
                        let tys_str = core::error_format(&ty, &core::Subst::empty());
                        format!("But '{m}::{f}' has a first argument of type {tys_str}")
                    }
                    None => format!("But '{m}::{f}' takes no arguments"),
                };
                context.add_diag(diag!(
                    Declarations::InvalidUseFun,
                    (loc, msg),
                    (first_ty_loc, first_tn_msg),
                ));
                None
            }
        });
    }
    // remove any empty use funs
    resolved.retain(|_, methods| !methods.is_empty());

    // resolve implicit candidates, removing if
    // - It is not a valid method (i.e. if it would be invalid to declare as a 'use fun')
    // - The name is already bound
    for (method, implicit) in std::mem::take(implicit_candidates) {
        let E::ImplicitUseFunCandidate {
            loc,
            attributes,
            is_public,
            function: (target_m, target_f),
            kind: ekind,
        } = implicit;
        let Some((target_f, tn)) = is_valid_method(context, &target_m, target_f) else {
            if matches!(ekind, E::ImplicitUseFunKind::UseAlias { used: false }) {
                let msg = format!("Unused 'use' of alias '{}'. Consider removing it", method);
                context.add_diag(diag!(UnusedItem::Alias, (method.loc, msg),))
            }
            continue;
        };
        let (kind, used) = match ekind {
            E::ImplicitUseFunKind::FunctionDeclaration => (
                N::UseFunKind::FunctionDeclaration,
                /* silences unused warning */ true,
            ),
            E::ImplicitUseFunKind::UseAlias { used } => {
                assert!(is_public.is_none());
                (N::UseFunKind::UseAlias, used)
            }
        };
        let nuf = N::UseFun {
            doc: DocComment::empty(),
            loc,
            attributes,
            is_public,
            tname: tn.clone(),
            target_function: (target_m, target_f),
            kind,
            used,
        };
        let nuf_loc = nuf.loc;
        let methods = resolved.entry(tn.clone()).or_insert_with(UniqueMap::new);
        if let Err((_, prev)) = methods.add(method, nuf) {
            let msg = format!("Duplicate 'use fun' for '{}.{}'", tn, method);
            let tn_msg = match ekind {
                E::ImplicitUseFunKind::UseAlias { .. } => {
                    "'use' function aliases create an implicit 'use fun' when their first \
                    argument is a type defined in that module"
                }
                E::ImplicitUseFunKind::FunctionDeclaration => {
                    "Function declarations create an implicit 'use fun' when their first \
                    argument is a type defined in the same module"
                }
            };
            context.add_diag(diag!(
                Declarations::DuplicateItem,
                (nuf_loc, msg),
                (prev, "Previously declared here"),
                (tn.loc, tn_msg)
            ))
        }
    }
}

fn is_valid_method(
    context: &mut Context,
    target_m: &ModuleIdent,
    target_f: Name,
) -> Option<(FunctionName, N::TypeName)> {
    let target_f = FunctionName(target_f);
    // possible the function was removed, e.g. a spec function
    if !context
        .info
        .module(target_m)
        .functions
        .contains_key(&target_f)
    {
        return None;
    }
    let (_, first_ty) = first_arg_type(context, target_m, &target_f);
    let first_ty = first_ty?;
    let tn = first_ty.value.unfold_to_type_name()?;
    let defining_module = match &tn.value {
        N::TypeName_::Multiple(_) => return None,
        N::TypeName_::Builtin(sp!(_, bt_)) => context.env.primitive_definer(*bt_)?,
        N::TypeName_::ModuleType(m, _) => m,
    };
    if defining_module == target_m {
        Some((target_f, tn.clone()))
    } else {
        None
    }
}

fn first_arg_type(
    context: &mut Context,
    m: &ModuleIdent,
    f: &FunctionName,
) -> (Loc, Option<N::Type>) {
    let finfo = context.info.function_info(m, f);
    match finfo
        .signature
        .parameters
        .first()
        .map(|(_, _, t)| t.clone())
    {
        None => (finfo.defined_loc, None),
        Some(t) => (t.loc, Some(t)),
    }
}

//**************************************************************************************************
// Expressions
//**************************************************************************************************

fn sequence(context: &mut Context, (uf, seq): &mut N::Sequence) {
    use_funs(context, uf);
    for sp!(_, item_) in seq {
        match item_ {
            N::SequenceItem_::Seq(e) | N::SequenceItem_::Bind(_, e) => exp(context, e),
            N::SequenceItem_::Declare(_, _) => (),
        }
    }
}

#[growing_stack]
fn exp(context: &mut Context, sp!(_, e_): &mut N::Exp) {
    match e_ {
        N::Exp_::Value(_)
        | N::Exp_::Var(_)
        | N::Exp_::Constant(_, _)
        | N::Exp_::Continue(_)
        | N::Exp_::Unit { .. }
        | N::Exp_::ErrorConstant { .. }
        | N::Exp_::UnresolvedError => (),
        N::Exp_::Return(e)
        | N::Exp_::Abort(e)
        | N::Exp_::Give(_, _, e)
        | N::Exp_::Dereference(e)
        | N::Exp_::UnaryExp(_, e)
        | N::Exp_::Cast(e, _)
        | N::Exp_::Assign(_, e)
        | N::Exp_::Loop(_, e)
        | N::Exp_::Annotate(e, _)
        | N::Exp_::Lambda(N::Lambda {
            parameters: _,
            return_type: _,
            return_label: _,
            use_fun_color: _,
            body: e,
        }) => exp(context, e),
        N::Exp_::IfElse(econd, et, ef_opt) => {
            exp(context, econd);
            exp(context, et);
            if let Some(ef) = ef_opt {
                exp(context, ef);
            }
        }
        N::Exp_::Match(esubject, arms) => {
            exp(context, esubject);
            for arm in &mut arms.value {
                if let Some(guard) = arm.value.guard.as_mut() {
                    exp(context, guard)
                }
                exp(context, &mut arm.value.rhs);
            }
        }
        N::Exp_::While(_, econd, ebody) => {
            exp(context, econd);
            exp(context, ebody)
        }
        N::Exp_::Block(N::Block {
            name: _,
            from_macro_argument: _,
            seq,
        }) => sequence(context, seq),
        N::Exp_::FieldMutate(ed, e) => {
            exp_dotted(context, ed);
            exp(context, e)
        }
        N::Exp_::Mutate(el, er) | N::Exp_::BinopExp(el, _, er) => {
            exp(context, el);
            exp(context, er)
        }
        N::Exp_::Pack(_, _, _, fields) => {
            for (_, _, (_, e)) in fields {
                exp(context, e)
            }
        }
        N::Exp_::PackVariant(_, _, _, _, fields) => {
            for (_, _, (_, e)) in fields {
                exp(context, e)
            }
        }
        N::Exp_::Builtin(_, sp!(_, es))
        | N::Exp_::Vector(_, _, sp!(_, es))
        | N::Exp_::ModuleCall(_, _, _, _, sp!(_, es))
        | N::Exp_::VarCall(_, sp!(_, es))
        | N::Exp_::ExpList(es) => {
            for e in es {
                exp(context, e)
            }
        }
        N::Exp_::MethodCall(ed, _, _, _, _, sp!(_, es)) => {
            exp_dotted(context, ed);
            for e in es {
                exp(context, e)
            }
        }

        N::Exp_::ExpDotted(_, ed) => exp_dotted(context, ed),
    }
}

#[growing_stack]
fn exp_dotted(context: &mut Context, sp!(_, ed_): &mut N::ExpDotted) {
    match ed_ {
        N::ExpDotted_::Exp(e) => exp(context, e),
        N::ExpDotted_::Dot(ed, _, _) | N::ExpDotted_::DotAutocomplete(_, ed) => {
            exp_dotted(context, ed)
        }
        N::ExpDotted_::Index(ed, sp!(_, es)) => {
            exp_dotted(context, ed);
            for e in es {
                exp(context, e)
            }
        }
    }
}
