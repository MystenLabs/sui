// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod state;

use super::absint::*;
use crate::{
    cfgir::CFGContext,
    diag,
    diagnostics::{Diagnostic, Diagnostics},
    editions::Edition,
    expansion::ast::{AbilitySet, ModuleIdent, Mutability},
    hlir::{
        ast::*,
        translate::{display_var, DisplayVar},
    },
    naming::ast::{self as N, TParam},
    parser::ast::{Ability_, DatatypeName},
    shared::{program_info::DatatypeKind, unique_map::UniqueMap},
};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use state::*;
use std::collections::BTreeMap;

//**************************************************************************************************
// Entry and trait bindings
//**************************************************************************************************

struct LocalsSafety<'a> {
    context: &'a CFGContext<'a>,
    local_types: &'a UniqueMap<Var, (Mutability, SingleType)>,
    signature: &'a FunctionSignature,
    unused_mut: BTreeMap<Var, Loc>,
}

impl<'a> LocalsSafety<'a> {
    fn new(
        context: &'a CFGContext<'a>,
        local_types: &'a UniqueMap<Var, (Mutability, SingleType)>,
        signature: &'a FunctionSignature,
    ) -> Self {
        let unused_mut = local_types
            .key_cloned_iter()
            .filter_map(|(v, (mut_, _))| {
                if let Mutability::Mut(loc) = mut_ {
                    Some((v, *loc))
                } else {
                    None
                }
            })
            .collect();
        Self {
            context,
            local_types,
            signature,
            unused_mut,
        }
    }
}

struct Context<'a, 'b> {
    outer: &'a CFGContext<'a>,
    local_types: &'a UniqueMap<Var, (Mutability, SingleType)>,
    unused_mut: &'a mut BTreeMap<Var, Loc>,
    local_states: &'b mut LocalStates,
    signature: &'a FunctionSignature,
    diags: Diagnostics,
}

impl<'a, 'b> Context<'a, 'b> {
    fn new(locals_safety: &'a mut LocalsSafety, local_states: &'b mut LocalStates) -> Self {
        let outer = locals_safety.context;
        let local_types = locals_safety.local_types;
        let signature = locals_safety.signature;
        let unused_mut = &mut locals_safety.unused_mut;
        Self {
            outer,
            local_types,
            unused_mut,
            local_states,
            signature,
            diags: Diagnostics::new(),
        }
    }

    fn add_diag(&mut self, d: Diagnostic) {
        self.diags.add(d)
    }

    fn extend_diags(&mut self, diags: Diagnostics) {
        self.diags.extend(diags)
    }

    fn get_diags(self) -> Diagnostics {
        self.diags
    }

    fn get_state(&self, local: &Var) -> &LocalState {
        self.local_states.get_state(local)
    }

    fn set_state(&mut self, local: Var, state: LocalState) {
        self.local_states.set_state(local, state)
    }

    fn local_type(&self, local: &Var) -> &SingleType {
        &self.local_types.get(local).unwrap().1
    }

    fn local_mutability(&self, local: &Var) -> Mutability {
        self.local_types.get(local).unwrap().0
    }

    fn get_first_assignment(&self, local: &Var) -> Option<Loc> {
        self.local_states.get_first_assignment(local)
    }

    fn maybe_set_first_assignment(&mut self, local: Var, loc: Loc) {
        self.local_states.maybe_set_first_assignment(local, loc)
    }

    fn mark_mutable_usage(&mut self, _eloc: Loc, v: &Var) {
        self.unused_mut.remove(v);
    }

    //     let decl_loc = *context
    //     .datatype_declared_abilities
    //     .get(m)
    //     .unwrap()
    //     .get_loc(s)
    //     .unwrap();
    // let declared_abilities = context
    //     .datatype_declared_abilities
    //     .get(m)
    //     .unwrap()
    //     .get(s)
    //     .unwrap();

    fn datatype_decl_loc(&self, m: &ModuleIdent, n: &DatatypeName) -> Loc {
        let kind = self.outer.info.datatype_kind(m, n);
        match kind {
            DatatypeKind::Struct => self.outer.info.struct_declared_loc(m, n),
            DatatypeKind::Enum => self.outer.info.enum_declared_loc(m, n),
        }
    }

    fn datatype_declared_abilities(&self, m: &ModuleIdent, n: &DatatypeName) -> &'a AbilitySet {
        let kind = self.outer.info.datatype_kind(m, n);
        match kind {
            DatatypeKind::Struct => self.outer.info.struct_declared_abilities(m, n),
            DatatypeKind::Enum => self.outer.info.enum_declared_abilities(m, n),
        }
    }
}

impl<'a> TransferFunctions for LocalsSafety<'a> {
    type State = LocalStates;

    fn execute(
        &mut self,
        pre: &mut Self::State,
        _lbl: Label,
        _idx: usize,
        cmd: &Command,
    ) -> Diagnostics {
        let mut context = Context::new(self, pre);
        command(&mut context, cmd);
        context.get_diags()
    }
}

impl<'a> AbstractInterpreter for LocalsSafety<'a> {}

pub fn verify(
    context: &super::CFGContext,
    cfg: &super::cfg::MutForwardCFG,
) -> BTreeMap<Label, LocalStates> {
    let super::CFGContext {
        signature, locals, ..
    } = context;
    let initial_state = LocalStates::initial(&signature.parameters, locals);
    let mut locals_safety = LocalsSafety::new(context, locals, signature);
    let (final_state, ds) = locals_safety.analyze_function(cfg, initial_state);
    unused_let_muts(context, locals, locals_safety.unused_mut);
    context.add_diags(ds);
    final_state
}

/// Generates warnings for unused mut declarations
fn unused_let_muts<T>(
    context: &CFGContext,
    locals: &UniqueMap<Var, T>,
    unused_mut_locals: BTreeMap<Var, Loc>,
) {
    for (v, mut_loc) in unused_mut_locals {
        if !v.starts_with_underscore() {
            let vstr = match display_var(v.value()) {
                DisplayVar::Tmp => panic!("ICE invalid unused mut tmp local {}", v.value()),
                DisplayVar::MatchTmp(s) => s,
                DisplayVar::Orig(s) => s,
            };
            let decl_loc = *locals.get_loc(&v).unwrap();
            let decl_msg = format!("The variable '{vstr}' is never used mutably");
            let mut_msg = "Consider removing the 'mut' declaration here";
            context.add_diag(diag!(
                UnusedItem::MutModifier,
                (decl_loc, decl_msg),
                (mut_loc, mut_msg)
            ))
        }
    }
}

//**************************************************************************************************
// Command
//**************************************************************************************************

#[growing_stack]
fn command(context: &mut Context, sp!(loc, cmd_): &Command) {
    use Command_ as C;
    match cmd_ {
        C::Assign(case, ls, e) => {
            exp(context, e);
            lvalues(context, *case, ls);
        }
        C::Mutate(el, er) => {
            exp(context, er);
            exp(context, el)
        }
        C::Abort(_, e)
        | C::IgnoreAndPop { exp: e, .. }
        | C::JumpIf { cond: e, .. }
        | C::VariantSwitch { subject: e, .. } => exp(context, e),

        C::Return { exp: e, .. } => {
            exp(context, e);
            let mut diags = Diagnostics::new();
            for (local, state) in context.local_states.iter() {
                match state {
                    LocalState::Unavailable(_, _) => (),
                    LocalState::Available(available)
                    | LocalState::MaybeUnavailable { available, .. } => {
                        let ty = context.local_type(&local);
                        let abilities = ty.value.abilities(ty.loc);
                        if !abilities.has_ability_(Ability_::Drop) {
                            let verb = match state {
                                LocalState::Unavailable(_, _) => unreachable!(),
                                LocalState::Available(_) => "still contains",
                                LocalState::MaybeUnavailable { .. } => "might still contain",
                            };
                            let available = *available;
                            let stmt = match display_var(local.value()) {
                                DisplayVar::Tmp => "The value is created but not used".to_owned(),
                                DisplayVar::MatchTmp(_name) => {
                                    "The match expression takes ownership of this value \
                                    but does not use it"
                                        .to_string()
                                }
                                DisplayVar::Orig(l) => {
                                    if context.signature.is_parameter(&local) {
                                        format!("The parameter '{}' {} a value", l, verb,)
                                    } else {
                                        format!("The local variable '{}' {} a value", l, verb,)
                                    }
                                }
                            };
                            let msg = format!(
                                "{}. The value does not have the '{}' ability and must be \
                                 consumed before the function returns",
                                stmt,
                                Ability_::Drop,
                            );
                            let mut diag = diag!(
                                MoveSafety::UnusedUndroppable,
                                (*loc, "Invalid return"),
                                (available, msg)
                            );
                            add_drop_ability_tip(context, &mut diag, ty.clone());
                            diags.add(diag);
                        }
                    }
                }
            }
            context.extend_diags(diags)
        }
        C::Jump { .. } => (),
        C::Break(_) | C::Continue(_) => panic!("ICE break/continue not translated to jumps"),
    }
}

fn lvalues(context: &mut Context, case: AssignCase, ls: &[LValue]) {
    ls.iter().for_each(|l| lvalue(context, case, l))
}

fn lvalue(context: &mut Context, case: AssignCase, sp!(loc, l_): &LValue) {
    use LValue_ as L;
    match l_ {
        L::Ignore => (),
        L::Var { var: v, .. } => {
            if case == AssignCase::Update {
                let mut_ = context.local_mutability(v);
                if let Some(assign_loc) = context.get_first_assignment(v) {
                    // If it has already been assigned, it is a mutation.
                    // This will trigger even if it was assigned and then moved
                    check_mutability(context, *loc, "assignment", v, mut_, Some(assign_loc));
                }
            }
            context.maybe_set_first_assignment(*v, *loc);
            let ty = context.local_type(v);
            let abilities = ty.value.abilities(ty.loc);
            if !abilities.has_ability_(Ability_::Drop) {
                let old_state = context.get_state(v);
                match old_state {
                    LocalState::Unavailable(_, _) => (),
                    LocalState::Available(available)
                    | LocalState::MaybeUnavailable { available, .. } => {
                        let verb = match old_state {
                            LocalState::Unavailable(_, _) => unreachable!(),
                            LocalState::Available(_) => "contains",
                            LocalState::MaybeUnavailable { .. } => "might contain",
                        };
                        let available = *available;
                        match display_var(v.value()) {
                            DisplayVar::Tmp | DisplayVar::MatchTmp(_) => {
                                let msg = format!(
                                    "This expression without the '{}' ability must be used",
                                    Ability_::Drop,
                                );
                                let mut diag = diag!(
                                    MoveSafety::UnusedUndroppable,
                                    (*loc, "Invalid usage of undroppable value".to_string()),
                                    (available, msg),
                                );
                                add_drop_ability_tip(context, &mut diag, ty.clone());
                                context.add_diag(diag)
                            }
                            DisplayVar::Orig(s) => {
                                let msg = format!(
                                    "The variable {} a value due to this assignment. The value \
                                    does not have the '{}' ability and must be used before you \
                                    assign to this variable again",
                                    verb,
                                    Ability_::Drop,
                                );
                                let mut diag = diag!(
                                    MoveSafety::UnusedUndroppable,
                                    (*loc, format!("Invalid assignment to variable '{}'", s)),
                                    (available, msg),
                                );
                                add_drop_ability_tip(context, &mut diag, ty.clone());
                                context.add_diag(diag)
                            }
                        };
                    }
                }
            }
            context.set_state(*v, LocalState::Available(*loc))
        }
        L::Unpack(_, _, fields) => fields.iter().for_each(|(_, l)| lvalue(context, case, l)),
        L::UnpackVariant(_, _, _, _, _, fields) => {
            fields.iter().for_each(|(_, l)| lvalue(context, case, l))
        }
    }
}

#[growing_stack]
fn exp(context: &mut Context, parent_e: &Exp) {
    use UnannotatedExp_ as E;
    let eloc = &parent_e.exp.loc;
    match &parent_e.exp.value {
        E::Unit { .. }
        | E::Value(_)
        | E::Constant(_)
        | E::UnresolvedError
        | E::ErrorConstant { .. } => (),

        E::BorrowLocal(mut_, var) => {
            if *mut_ {
                let mutability = context.local_mutability(var);
                check_mutability(context, *eloc, "mutable borrow", var, mutability, None)
            }
            use_local(context, eloc, var)
        }
        E::Copy { var, .. } => use_local(context, eloc, var),

        E::Move { var, .. } => {
            use_local(context, eloc, var);
            context.set_state(
                *var,
                LocalState::Unavailable(*eloc, UnavailableReason::Moved),
            )
        }

        E::ModuleCall(mcall) => mcall.arguments.iter().map(|e| exp(context, e)).collect(),
        E::Vector(_, _, _, args) => args.iter().map(|e| exp(context, e)).collect(),
        E::Freeze(e)
        | E::Dereference(e)
        | E::UnaryExp(_, e)
        | E::Borrow(_, e, _, _)
        | E::Cast(e, _) => exp(context, e),

        E::BinopExp(e1, _, e2) => {
            exp(context, e1);
            exp(context, e2)
        }

        E::Pack(_, _, fields) => fields.iter().for_each(|(_, _, e)| exp(context, e)),

        E::PackVariant(_, _, _, fields) => fields.iter().for_each(|(_, _, e)| exp(context, e)),

        E::Multiple(es) => es.iter().for_each(|e| exp(context, e)),

        E::Unreachable => panic!("ICE should not analyze dead code"),
    }
}

fn use_local(context: &mut Context, loc: &Loc, local: &Var) {
    use LocalState as L;
    let state = context.get_state(local);
    match state {
        L::Available(_) => (),
        L::Unavailable(unavailable, unavailable_reason)
        | L::MaybeUnavailable {
            unavailable,
            unavailable_reason,
            ..
        } => {
            let unavailable = *unavailable;
            let vstr = match display_var(local.value()) {
                DisplayVar::Tmp => panic!("ICE invalid use tmp local {}", local.value()),
                DisplayVar::MatchTmp(s) => s,
                DisplayVar::Orig(s) => s,
            };
            match unavailable_reason {
                UnavailableReason::Unassigned => {
                    let msg = format!(
                        "The variable '{}' {} not have a value. The variable must be assigned a \
                         value before being used.",
                        vstr,
                        match state {
                            LocalState::Available(_) => unreachable!(),
                            LocalState::Unavailable(_, _) => "does",
                            LocalState::MaybeUnavailable { .. } => "might",
                        }
                    );
                    context.add_diag(diag!(
                        MoveSafety::UnassignedVariable,
                        (
                            *loc,
                            format!("Invalid usage of unassigned variable '{}'", vstr)
                        ),
                        (unavailable, msg),
                    ));
                }
                UnavailableReason::Moved => {
                    let verb = match state {
                        LocalState::Available(_) => unreachable!(),
                        LocalState::Unavailable(_, _) => "was",
                        LocalState::MaybeUnavailable { .. } => "might have been",
                    };
                    let suggestion = format!("Suggestion: use 'copy {}' to avoid the move.", vstr);
                    let reason = if *loc == unavailable {
                        "In a loop, this typically means it was moved in the first iteration, and \
                         is not available by the second iteration."
                            .to_string()
                    } else {
                        format!("The value of '{}' {} previously moved here.", vstr, verb)
                    };
                    context.add_diag(diag!(
                        MoveSafety::UnassignedVariable,
                        (
                            *loc,
                            format!("Invalid usage of previously moved variable '{}'.", vstr)
                        ),
                        (unavailable, reason),
                        (unavailable, suggestion),
                    ));
                }
            };
        }
    }
}

fn check_mutability(
    context: &mut Context,
    eloc: Loc,
    usage: &str,
    v: &Var,
    mut_: Mutability,
    prev_assignment: Option<Loc>,
) {
    context.mark_mutable_usage(eloc, v);
    if mut_ == Mutability::Imm {
        let vstr = match display_var(v.value()) {
            DisplayVar::Tmp => panic!("ICE invalid mutation tmp local {}", v.value()),
            DisplayVar::MatchTmp(s) => s,
            DisplayVar::Orig(s) => s,
        };
        let decl_loc = *context.local_types.get_loc(v).unwrap();
        let usage_msg = format!("Invalid {usage} of immutable variable '{vstr}'");
        let decl_msg =
            format!("To use the variable mutably, it must be declared 'mut', e.g. 'mut {vstr}'");
        if context.outer.env.edition(context.outer.package) == Edition::E2024_MIGRATION {
            context.add_diag(diag!(Migration::NeedsLetMut, (decl_loc, decl_msg.clone())))
        } else {
            let mut diag = diag!(
                TypeSafety::InvalidImmVariableUsage,
                (eloc, usage_msg),
                (decl_loc, decl_msg),
            );
            if let Some(prev) = prev_assignment {
                if prev != decl_loc {
                    let msg = if eloc == prev {
                        "The variable is assigned multiple times here in a loop"
                    } else {
                        "The variable was initially assigned here"
                    };
                    diag.add_secondary_label((prev, msg));
                }
            }
            context.add_diag(diag)
        }
    }
}

//**************************************************************************************************
// Error helper
//**************************************************************************************************

fn add_drop_ability_tip(context: &Context, diag: &mut Diagnostic, st: SingleType) {
    use N::{TypeName_ as TN, Type_ as T};
    let ty = single_type_to_naming_type(st);
    let owned_abilities;
    let (declared_loc_opt, declared_abilities, ty_args) = match &ty.value {
        T::Param(TParam {
            user_specified_name,
            abilities,
            ..
        }) => (Some(user_specified_name.loc), abilities, vec![]),
        T::Apply(_, sp!(_, TN::Builtin(b)), ty_args) => {
            owned_abilities = b.value.declared_abilities(b.loc);
            (None, &owned_abilities, ty_args.clone())
        }
        T::Apply(_, sp!(_, TN::ModuleType(m, s)), ty_args) => {
            let decl_loc = context.datatype_decl_loc(m, s);
            let declared_abilities = context.datatype_declared_abilities(m, s);
            (Some(decl_loc), declared_abilities, ty_args.clone())
        }
        t => panic!(
            "ICE either the type did not have 'drop' when it should have or it was converted \
             incorrectly {:?}",
            t
        ),
    };
    crate::typing::core::ability_not_satisfied_tips(
        &crate::typing::core::Subst::empty(),
        diag,
        Ability_::Drop,
        &ty,
        declared_loc_opt,
        declared_abilities,
        ty_args.iter().map(|ty_arg| {
            let abilities = match &ty_arg.value {
                T::Unit => AbilitySet::collection(ty_arg.loc),
                T::Ref(_, _) => AbilitySet::references(ty_arg.loc),
                T::UnresolvedError | T::Anything => AbilitySet::all(ty_arg.loc),
                T::Param(TParam { abilities, .. }) | T::Apply(Some(abilities), _, _) => {
                    abilities.clone()
                }
                T::Var(_) | T::Apply(None, _, _) | T::Fun(_, _) => panic!("ICE expansion failed"),
            };
            (ty_arg, abilities)
        }),
    )
}

fn single_type_to_naming_type(sp!(loc, st_): SingleType) -> N::Type {
    sp(loc, single_type_to_naming_type_(st_))
}

fn single_type_to_naming_type_(st_: SingleType_) -> N::Type_ {
    use SingleType_ as S;
    use N::Type_ as T;
    match st_ {
        S::Ref(mut_, b) => T::Ref(mut_, Box::new(base_type_to_naming_type(b))),
        S::Base(sp!(_, b_)) => base_type_to_naming_type_(b_),
    }
}

fn base_type_to_naming_type(sp!(loc, bt_): BaseType) -> N::Type {
    sp(loc, base_type_to_naming_type_(bt_))
}

fn base_type_to_naming_type_(bt_: BaseType_) -> N::Type_ {
    use BaseType_ as B;
    use N::Type_ as T;
    match bt_ {
        B::Unreachable => T::Anything,
        B::UnresolvedError => T::UnresolvedError,
        B::Param(tp) => T::Param(tp),
        B::Apply(abilities, tn, ty_args) => T::Apply(
            Some(abilities),
            type_name_to_naming_type_name(tn),
            ty_args.into_iter().map(base_type_to_naming_type).collect(),
        ),
    }
}

fn type_name_to_naming_type_name(sp!(loc, tn_): TypeName) -> N::TypeName {
    sp(loc, type_name_to_naming_type_name_(tn_))
}

fn type_name_to_naming_type_name_(tn_: TypeName_) -> N::TypeName_ {
    use TypeName_ as TN;
    use N::TypeName_ as NTN;
    match tn_ {
        TN::Builtin(b) => NTN::Builtin(b),
        TN::ModuleType(m, n) => NTN::ModuleType(m, n),
    }
}
