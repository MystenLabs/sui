// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod state;

use super::absint::*;
use crate::{
    diag,
    diagnostics::{Diagnostic, Diagnostics},
    hlir::{
        ast::*,
        translate::{DisplayVar, display_var},
    },
    ice_assert,
    parser::ast::BinOp_,
    shared::unique_map::UniqueMap,
};
use move_proc_macros::growing_stack;

use state::{Value, *};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

//**************************************************************************************************
// Entry and trait bindings
//**************************************************************************************************

struct BorrowSafety {
    local_numbers: UniqueMap<Var, usize>,
    mutably_used: RefExpInfoMap,
    outer_env_has_errors: bool,
}

impl BorrowSafety {
    fn new<T>(local_types: &UniqueMap<Var, T>, outer_env_has_errors: bool) -> Self {
        let mut local_numbers = UniqueMap::new();
        for (idx, (v, _)) in local_types.key_cloned_iter().enumerate() {
            local_numbers.add(v, idx).unwrap();
        }
        Self {
            local_numbers,
            mutably_used: Rc::new(RefCell::new(BTreeMap::new())),
            outer_env_has_errors,
        }
    }
}

struct Context<'a, 'b> {
    local_numbers: &'a UniqueMap<Var, usize>,
    borrow_state: &'b mut BorrowState,
    diags: Diagnostics,
    outer_env_has_errors: bool,
}

impl<'a, 'b> Context<'a, 'b> {
    fn new(safety: &'a BorrowSafety, borrow_state: &'b mut BorrowState) -> Self {
        let local_numbers = &safety.local_numbers;
        let outer_env_has_errors = safety.outer_env_has_errors;
        Self {
            local_numbers,
            borrow_state,
            diags: Diagnostics::new(),
            outer_env_has_errors,
        }
    }

    fn get_diags(self) -> Diagnostics {
        self.diags
    }

    fn add_diag(&mut self, diag: Diagnostic) {
        self.diags.add(diag);
    }

    fn add_diags(&mut self, additional: Diagnostics) {
        self.diags.extend(additional);
    }
}

impl TransferFunctions for BorrowSafety {
    type State = BorrowState;

    fn execute(
        &mut self,
        pre: &mut Self::State,
        lbl: Label,
        idx: usize,
        cmd: &Command,
    ) -> Diagnostics {
        pre.start_command(lbl, idx);
        let mut context = Context::new(self, pre);
        command(&mut context, cmd);
        context
            .borrow_state
            .canonicalize_locals(context.local_numbers);
        context.get_diags()
    }
}

pub fn verify(
    context: &super::CFGContext,
    cfg: &super::cfg::MutForwardCFG,
) -> BTreeMap<Label, BorrowState> {
    let super::CFGContext {
        signature, locals, ..
    } = context;
    let has_errors = context.env.has_errors();
    let mut safety = BorrowSafety::new(locals, has_errors);

    // check for existing errors
    let mut initial_state = BorrowState::initial(locals, safety.mutably_used.clone(), has_errors);
    initial_state.bind_arguments(&signature.parameters);
    initial_state.canonicalize_locals(&safety.local_numbers);
    let (final_state, ds) = analyze_function(&mut safety, cfg, initial_state);
    context.add_diags(ds);
    unused_mut_borrows(context, safety.mutably_used);
    final_state
}

fn unused_mut_borrows(context: &super::CFGContext, mutably_used: RefExpInfoMap) {
    const MSG: &str = "Mutable reference is never used mutably, \
    consider switching to an immutable reference '&' instead";

    for info in RefCell::borrow(&mutably_used).values() {
        let RefExpInfo {
            loc,
            is_mut,
            used_mutably,
            param_name,
        } = info;
        if *is_mut && !*used_mutably {
            let diag = if let Some(v) = param_name {
                if matches!(context.visibility, Visibility::Public(_)) {
                    // silence the warning for public function parameters
                    continue;
                }
                let param_loc = v.loc();
                let DisplayVar::Orig(v) = display_var(v.value()) else {
                    panic!("ICE param {v:?} is a tmp")
                };
                let param_msg = format!(
                    "For parameters, this can be silenced by prefixing \
                    the name with an underscore, e.g. '_{v}'"
                );
                diag!(UnusedItem::MutParam, (*loc, MSG), (param_loc, param_msg))
            } else {
                diag!(UnusedItem::MutReference, (*loc, MSG))
            };
            context.add_diag(diag)
        }
    }
}

//**************************************************************************************************
// Command
//**************************************************************************************************

macro_rules! single_value_exp {
    ($context:ident, $e:expr) => {{
        let e = $e;
        let has_errors = $context.outer_env_has_errors;
        let values = exp($context, e);
        ice_assert!(
            $context,
            values.len() == 1 || has_errors,
            e.exp.loc,
            "Expected a single value",
        );
        let mut iter = values.into_iter();
        let v = iter.next().unwrap_or(Value::NonRef);
        for extra in iter {
            $context.borrow_state.release_value(extra);
        }
        v
    }};
}

macro_rules! assert_non_ref {
    ($context:ident, $v:expr, $loc:expr) => {
        let has_errors = $context.outer_env_has_errors;
        let v = $v;
        ice_assert!(
            $context,
            !v.is_ref() || has_errors,
            $loc,
            "Expected a non_reference value",
        );
        $context.borrow_state.release_value(v);
    };
}

#[growing_stack]
fn command(context: &mut Context, sp!(loc, cmd_): &Command) {
    use Command_ as C;
    match cmd_ {
        C::Assign(_, ls, e) => {
            let values = exp(context, e);
            lvalues(context, ls, values);
        }
        C::Mutate(el, er) => {
            let value = single_value_exp!(context, er);
            assert_non_ref!(context, value, er.exp.loc);
            let lvalue = single_value_exp!(context, el);
            let diags = context.borrow_state.mutate(*loc, lvalue);
            context.add_diags(diags);
        }
        C::JumpIf { cond: e, .. } => {
            let value = single_value_exp!(context, e);
            assert_non_ref!(context, value, e.exp.loc);
        }
        C::VariantSwitch { subject, .. } => {
            let value = single_value_exp!(context, subject);
            let diags = context.borrow_state.variant_switch(*loc, value);
            context.add_diags(diags);
        }
        C::IgnoreAndPop { exp: e, .. } => {
            let values = exp(context, e);
            context.borrow_state.release_values(values);
        }

        C::Return { exp: e, .. } => {
            let values = exp(context, e);
            let diags = context.borrow_state.return_(*loc, values);
            context.add_diags(diags);
        }
        C::Abort(_, e) => {
            let value = single_value_exp!(context, e);
            assert_non_ref!(context, value, e.exp.loc);
            context.borrow_state.abort()
        }
        C::Jump { .. } => (),
        C::Break(_) | C::Continue(_) => panic!("ICE break/continue not translated to jumps"),
    }
}

fn lvalues(context: &mut Context, ls: &[LValue], values: Values) {
    ls.iter()
        .zip(values)
        .for_each(|(l, value)| lvalue(context, l, value))
}

fn lvalue(context: &mut Context, sp!(loc, l_): &LValue, value: Value) {
    use LValue_ as L;
    match l_ {
        L::Ignore
        | L::Var {
            unused_assignment: true,
            ..
        } => {
            context.borrow_state.release_value(value);
        }
        L::Var {
            var: v,
            unused_assignment: false,
            ..
        } => {
            let diags = context.borrow_state.assign_local(*loc, v, value);
            context.add_diags(diags)
        }
        L::Unpack(_, _, fields) => {
            assert_non_ref!(context, value, *loc);
            fields
                .iter()
                .for_each(|(_, l)| lvalue(context, l, Value::NonRef))
        }
        L::UnpackVariant(_, _, unpack_type, _, _, fields) => match unpack_type {
            UnpackType::ByValue => {
                assert_non_ref!(context, value, *loc);
                fields
                    .iter()
                    .for_each(|(_, l)| lvalue(context, l, Value::NonRef))
            }
            UnpackType::ByImmRef => {
                let (diags, fvs) = context
                    .borrow_state
                    .borrow_variant_fields(*loc, false, value, fields);
                context.add_diags(diags);
                assert!(fvs.len() == fields.len());
                fvs.into_iter()
                    .zip(fields.iter())
                    .for_each(|(fv, (_, l))| lvalue(context, l, fv));
            }
            UnpackType::ByMutRef => {
                let (diags, fvs) = context
                    .borrow_state
                    .borrow_variant_fields(*loc, true, value, fields);
                context.add_diags(diags);
                assert!(fvs.len() == fields.len());
                fvs.into_iter()
                    .zip(fields.iter())
                    .for_each(|(fv, (_, l))| lvalue(context, l, fv));
            }
        },
    }
}

#[growing_stack]
fn exp(context: &mut Context, parent_e: &Exp) -> Values {
    use UnannotatedExp_ as E;
    let eloc = &parent_e.exp.loc;
    let svalue = || vec![Value::NonRef];
    match &parent_e.exp.value {
        E::Move { var, annotation } => {
            let last_usage = matches!(annotation, MoveOpAnnotation::InferredLastUsage);
            let (diags, value) = context.borrow_state.move_local(*eloc, var, last_usage);
            context.add_diags(diags);
            vec![value]
        }
        E::Copy { var, .. } => {
            let (diags, value) = context.borrow_state.copy_local(*eloc, var);
            context.add_diags(diags);
            vec![value]
        }
        E::BorrowLocal(mut_, var) => {
            let (diags, value) = context.borrow_state.borrow_local(*eloc, *mut_, var);
            context.add_diags(diags);
            assert!(value.is_ref());
            vec![value]
        }
        E::Freeze(e) => {
            let evalue = single_value_exp!(context, e);
            let (diags, value) = context.borrow_state.freeze(*eloc, evalue);
            context.add_diags(diags);
            vec![value]
        }
        E::Dereference(e) => {
            let evalue = single_value_exp!(context, e);
            let (errors, value) = context.borrow_state.dereference(*eloc, evalue);
            context.add_diags(errors);
            vec![value]
        }
        E::Borrow(mut_, e, f, shared_borrow) => {
            let evalue = single_value_exp!(context, e);
            let (diags, value) =
                context
                    .borrow_state
                    .borrow_field(*eloc, *mut_, evalue, f, *shared_borrow);
            context.add_diags(diags);
            vec![value]
        }

        E::Vector(_, n, _, args) => {
            let evalues: Values = args.iter().flat_map(|arg| exp(context, arg)).collect();
            debug_assert_eq!(*n, evalues.len());
            for v in evalues {
                assert_non_ref!(context, v, *eloc);
            }
            svalue()
        }

        E::ModuleCall(mcall) => {
            let evalues: Values = mcall
                .arguments
                .iter()
                .flat_map(|arg| exp(context, arg))
                .collect();
            let ret_ty = &parent_e.ty;
            let (diags, values) = context.borrow_state.call(*eloc, evalues, ret_ty);
            context.add_diags(diags);
            values
        }

        E::Unit { .. } => vec![],
        E::Value(_) | E::Constant(_) | E::UnresolvedError | E::ErrorConstant { .. } => svalue(),

        E::Cast(e, _) | E::UnaryExp(_, e) => {
            let v = single_value_exp!(context, e);
            assert_non_ref!(context, v, e.exp.loc);
            svalue()
        }
        E::BinopExp(e1, sp!(_, BinOp_::Eq), e2) | E::BinopExp(e1, sp!(_, BinOp_::Neq), e2) => {
            let v1 = single_value_exp!(context, e1);
            let v2 = single_value_exp!(context, e2);
            // must check separately incase of using a local with an unassigned value
            if v1.is_ref() {
                let (errors, _) = context.borrow_state.dereference(e1.exp.loc, v1);
                assert!(errors.is_empty(), "ICE eq freezing failed");
            }
            if v2.is_ref() {
                let (errors, _) = context.borrow_state.dereference(e1.exp.loc, v2);
                assert!(errors.is_empty(), "ICE eq freezing failed");
            }
            svalue()
        }
        E::BinopExp(e1, _, e2) => {
            let v1 = single_value_exp!(context, e1);
            let v2 = single_value_exp!(context, e2);
            assert_non_ref!(context, v1, e1.exp.loc);
            assert_non_ref!(context, v2, e2.exp.loc);
            svalue()
        }
        E::Pack(_, _, fields) => {
            for (_, _, e) in fields.iter() {
                let arg = single_value_exp!(context, e);
                assert_non_ref!(context, arg, e.exp.loc);
            }
            svalue()
        }
        E::PackVariant(_, _, _, fields) => {
            for (_, _, e) in fields.iter() {
                let arg = single_value_exp!(context, e);
                assert_non_ref!(context, arg, e.exp.loc);
            }
            svalue()
        }

        E::Multiple(es) => es.iter().flat_map(|e| exp(context, e)).collect(),

        E::Unreachable => panic!("ICE should not analyze dead code"),
    }
}
