// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod state;

use super::{
    absint::*,
    cfg::{MutForwardCFG, MutReverseCFG, ReverseCFG, CFG},
    locals,
};
use crate::{
    diagnostics::Diagnostics,
    expansion::ast::Mutability,
    hlir::ast::{self as H, *},
    shared::unique_map::UniqueMap,
};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use state::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

//**************************************************************************************************
// Entry and trait bindings
//**************************************************************************************************

type PerCommandStates = BTreeMap<Label, VecDeque<LivenessState>>;
type ForwardIntersections = BTreeMap<Label, BTreeSet<Var>>;
type FinalInvariants = BTreeMap<Label, LivenessState>;

struct Liveness {
    states: PerCommandStates,
}

impl Liveness {
    fn new(cfg: &MutReverseCFG) -> Self {
        let states = cfg
            .blocks()
            .map(|(lbl, block)| {
                let init = block.iter().map(|_| LivenessState::initial()).collect();
                (*lbl, init)
            })
            .collect();
        Liveness { states }
    }
}

impl TransferFunctions for Liveness {
    type State = LivenessState;

    fn execute(
        &mut self,
        state: &mut Self::State,
        label: Label,
        idx: usize,
        cmd: &Command,
    ) -> Diagnostics {
        command(state, cmd);
        // set current [label][command_idx] data with the new liveness data
        let cur_label_states = self.states.get_mut(&label).unwrap();
        cur_label_states[idx] = state.clone();
        Diagnostics::new()
    }
}

impl AbstractInterpreter for Liveness {}

//**************************************************************************************************
// Analysis
//**************************************************************************************************

fn analyze(
    cfg: &mut MutForwardCFG,
    infinite_loop_starts: &BTreeSet<Label>,
) -> (FinalInvariants, PerCommandStates) {
    let reverse = &mut ReverseCFG::new(cfg, infinite_loop_starts);
    let initial_state = LivenessState::initial();
    let mut liveness = Liveness::new(reverse);
    let (final_invariants, errors) = liveness.analyze_function(reverse, initial_state);
    assert!(errors.is_empty());
    (final_invariants, liveness.states)
}

#[growing_stack]
fn command(state: &mut LivenessState, sp!(_, cmd_): &Command) {
    use Command_ as C;
    match cmd_ {
        C::Assign(_, ls, e) => {
            lvalues(state, ls);
            exp(state, e);
        }
        C::Mutate(el, er) => {
            exp(state, er);
            exp(state, el)
        }
        C::Return { exp: e, .. }
        | C::Abort(_, e)
        | C::IgnoreAndPop { exp: e, .. }
        | C::JumpIf { cond: e, .. }
        | C::VariantSwitch { subject: e, .. } => exp(state, e),

        C::Jump { .. } => (),
        C::Break(_) | C::Continue(_) => panic!("ICE break/continue not translated to jumps"),
    }
}

fn lvalues(state: &mut LivenessState, ls: &[LValue]) {
    ls.iter().for_each(|l| lvalue(state, l))
}

fn lvalue(state: &mut LivenessState, sp!(_, l_): &LValue) {
    use LValue_ as L;
    match l_ {
        L::Ignore => (),
        L::Var { var, .. } => {
            state.0.remove(var);
        }
        L::Unpack(_, _, fields) => fields.iter().for_each(|(_, l)| lvalue(state, l)),
        L::UnpackVariant(_, _, _, _, _, fields) => {
            fields.iter().for_each(|(_, l)| lvalue(state, l))
        }
    }
}

#[growing_stack]
fn exp(state: &mut LivenessState, parent_e: &Exp) {
    use UnannotatedExp_ as E;
    match &parent_e.exp.value {
        E::Unit { .. }
        | E::Value(_)
        | E::Constant(_)
        | E::UnresolvedError
        | E::ErrorConstant { .. } => (),

        E::BorrowLocal(_, var) | E::Copy { var, .. } | E::Move { var, .. } => {
            state.0.insert(*var);
        }

        E::ModuleCall(mcall) => mcall.arguments.iter().for_each(|e| exp(state, e)),
        E::Vector(_, _, _, args) => args.iter().for_each(|e| exp(state, e)),
        E::Freeze(e)
        | E::Dereference(e)
        | E::UnaryExp(_, e)
        | E::Borrow(_, e, _, _)
        | E::Cast(e, _) => exp(state, e),

        E::BinopExp(e1, _, e2) => {
            exp(state, e1);
            exp(state, e2)
        }

        E::Pack(_, _, fields) => fields.iter().for_each(|(_, _, e)| exp(state, e)),
        E::PackVariant(_, _, _, fields) => fields.iter().for_each(|(_, _, e)| exp(state, e)),

        E::Multiple(es) => es.iter().for_each(|e| exp(state, e)),

        E::Unreachable => panic!("ICE should not analyze dead code"),
    }
}

//**************************************************************************************************
// Copy Refinement
//**************************************************************************************************

/// This pass:
/// - Switches the last inferred `copy` to a `move`.
///   It will error if the `copy` was specified by the user
/// - Reports an error if an assignment/let was not used
///   Switches it to an `Ignore` if it has the drop ability (helps with error messages for borrows)

pub fn last_usage(context: &super::CFGContext, cfg: &mut MutForwardCFG) {
    let super::CFGContext {
        infinite_loop_starts,
        ..
    } = context;
    let (final_invariants, per_command_states) = analyze(cfg, infinite_loop_starts);
    for (lbl, block) in cfg.blocks_mut() {
        let final_invariant = final_invariants
            .get(lbl)
            .unwrap_or_else(|| panic!("ICE no liveness states for {}", lbl));
        let command_states = per_command_states.get(lbl).unwrap();
        last_usage::block(context, final_invariant, command_states, block)
    }
}

mod last_usage {
    use move_proc_macros::growing_stack;

    use crate::{
        cfgir::{liveness::state::LivenessState, CFGContext},
        diag,
        hlir::{
            ast::*,
            translate::{display_var, DisplayVar},
        },
    };
    use std::collections::{BTreeSet, VecDeque};

    struct Context<'a, 'b> {
        outer: &'a CFGContext<'a>,
        next_live: &'b BTreeSet<Var>,
        dropped_live: BTreeSet<Var>,
    }

    impl<'a, 'b> Context<'a, 'b> {
        fn new(
            outer: &'a CFGContext<'a>,
            next_live: &'b BTreeSet<Var>,
            dropped_live: BTreeSet<Var>,
        ) -> Self {
            Context {
                outer,
                next_live,
                dropped_live,
            }
        }
    }

    pub fn block(
        context: &CFGContext,
        final_invariant: &LivenessState,
        command_states: &VecDeque<LivenessState>,
        block: &mut BasicBlock,
    ) {
        let len = block.len();
        let last_cmd = block.get(len - 1).unwrap();
        assert!(
            last_cmd.value.is_terminal(),
            "ICE malformed block. missing jump"
        );
        for idx in 0..len {
            let cmd = block.get_mut(idx).unwrap();
            let cur_data = &command_states.get(idx).unwrap().0;
            let next_data = match command_states.get(idx + 1) {
                Some(s) => &s.0,
                None => &final_invariant.0,
            };

            let dropped_live = cur_data
                .difference(next_data)
                .cloned()
                .collect::<BTreeSet<_>>();
            command(&mut Context::new(context, next_data, dropped_live), cmd)
        }
    }

    #[growing_stack]
    fn command(context: &mut Context, sp!(_, cmd_): &mut Command) {
        use Command_ as C;
        match cmd_ {
            C::Assign(_, ls, e) => {
                lvalues(context, ls);
                exp(context, e);
            }
            C::Mutate(el, er) => {
                exp(context, el);
                exp(context, er)
            }
            C::Return { exp: e, .. }
            | C::Abort(_, e)
            | C::IgnoreAndPop { exp: e, .. }
            | C::JumpIf { cond: e, .. }
            | C::VariantSwitch { subject: e, .. } => exp(context, e),

            C::Jump { .. } => (),
            C::Break(_) | C::Continue(_) => panic!("ICE break/continue not translated to jumps"),
        }
    }

    fn lvalues(context: &mut Context, ls: &mut [LValue]) {
        ls.iter_mut().for_each(|l| lvalue(context, l))
    }

    fn lvalue(context: &mut Context, l: &mut LValue) {
        use LValue_ as L;
        match &mut l.value {
            L::Ignore => (),
            L::Var {
                var: v,
                unused_assignment,
                ..
            } => {
                context.dropped_live.insert(*v);
                if !*unused_assignment && !context.next_live.contains(v) {
                    match display_var(v.value()) {
                        DisplayVar::Tmp => (),
                        DisplayVar::Orig(vstr) | DisplayVar::MatchTmp(vstr) => {
                            if !v.starts_with_underscore() {
                                let msg = format!(
                                    "Unused assignment for variable '{vstr}'. Consider \
                                     removing, replacing with '_', or prefixing with '_' (e.g., \
                                     '_{vstr}')",
                                );
                                context
                                    .outer
                                    .add_diag(diag!(UnusedItem::Assignment, (l.loc, msg)));
                            }
                            *unused_assignment = true;
                        }
                    }
                }
            }
            L::Unpack(_, _, fields) => fields.iter_mut().for_each(|(_, l)| lvalue(context, l)),
            L::UnpackVariant(_, _, _, _, _, fields) => {
                fields.iter_mut().for_each(|(_, l)| lvalue(context, l))
            }
        }
    }

    #[growing_stack]
    fn exp(context: &mut Context, parent_e: &mut Exp) {
        use UnannotatedExp_ as E;
        match &mut parent_e.exp.value {
            E::Unit { .. }
            | E::Value(_)
            | E::Constant(_)
            | E::UnresolvedError
            | E::ErrorConstant { .. } => (),

            E::BorrowLocal(_, var) | E::Move { var, .. } => {
                // remove it from context to prevent accidental dropping in previous usages
                context.dropped_live.remove(var);
            }

            E::Copy { var, from_user } => {
                // Even if not switched to a move:
                // remove it from dropped_live to prevent accidental dropping in previous usages
                let var_is_dead = context.dropped_live.remove(var);
                // Non-references might still be borrowed, but that error will be caught in borrow
                // checking with a specific tip/message
                if var_is_dead && !*from_user {
                    parent_e.exp.value = E::Move {
                        var: *var,
                        annotation: MoveOpAnnotation::InferredLastUsage,
                    }
                }
            }

            E::ModuleCall(mcall) => mcall
                .arguments
                .iter_mut()
                .rev()
                .for_each(|arg| exp(context, arg)),
            E::Vector(_, _, _, args) => args.iter_mut().rev().for_each(|arg| exp(context, arg)),
            E::Freeze(e)
            | E::Dereference(e)
            | E::UnaryExp(_, e)
            | E::Borrow(_, e, _, _)
            | E::Cast(e, _) => exp(context, e),

            E::BinopExp(e1, _, e2) => {
                exp(context, e2);
                exp(context, e1)
            }

            E::Pack(_, _, fields) => fields
                .iter_mut()
                .rev()
                .for_each(|(_, _, e)| exp(context, e)),

            E::PackVariant(_, _, _, fields) => fields
                .iter_mut()
                .rev()
                .for_each(|(_, _, e)| exp(context, e)),

            E::Multiple(es) => es.iter_mut().rev().for_each(|e| exp(context, e)),

            E::Unreachable => panic!("ICE should not analyze dead code"),
        }
    }
}

//**************************************************************************************************
// Refs Refinement
//**************************************************************************************************

/// This refinement releases dead reference values by adding a move + pop. In other words, if a
/// reference `r` is dead, it will insert `_ = move r` after the last usage
///
/// However, due to the previous `last_usage` analysis. Any last usage of a reference is a move.
/// And any unused assignment to a reference holding local is switched to a `Ignore`.
/// Thus the only way a reference could still be dead is if it was live in a loop
/// Additionally, the borrow checker will consider any reference to be released if it was released
/// in any predecessor.
/// As such, the only references that need to be released by an added `_ = move r` are references
/// at the beginning of a block given that
/// (1) The reference is live in the predecessor and the predecessor is a loop
/// (2)  The reference is live in ALL predecessors (otherwise the borrow checker will release them)
///
/// Because of this, `build_forward_intersections` intersects all of the forward post states of
/// predecessors.
/// Then `release_dead_refs_block` adds a release at the beginning of the block if the reference
/// satisfies (1) and (2)

pub fn release_dead_refs(
    context: &super::CFGContext,
    locals_pre_states: &BTreeMap<Label, locals::state::LocalStates>,
    cfg: &mut MutForwardCFG,
) {
    let super::CFGContext {
        locals,
        infinite_loop_starts,
        ..
    } = context;
    let (liveness_pre_states, _per_command_states) = analyze(cfg, infinite_loop_starts);
    let forward_intersections = build_forward_intersections(cfg, &liveness_pre_states);
    for (lbl, block) in cfg.blocks_mut() {
        let locals_pre_state = locals_pre_states.get(lbl).unwrap();
        let liveness_pre_state = liveness_pre_states.get(lbl).unwrap();
        let forward_intersection = forward_intersections.get(lbl).unwrap();
        release_dead_refs_block(
            locals,
            locals_pre_state,
            liveness_pre_state,
            forward_intersection,
            block,
        )
    }
}

fn build_forward_intersections(
    cfg: &MutForwardCFG,
    final_invariants: &FinalInvariants,
) -> ForwardIntersections {
    cfg.blocks()
        .keys()
        .map(|lbl| {
            let mut states = cfg
                .predecessors(*lbl)
                .iter()
                .map(|pred| &final_invariants.get(pred).unwrap().0);
            let intersection = states
                .next()
                .map(|init| states.fold(init.clone(), |acc, s| &acc & s))
                .unwrap_or_else(BTreeSet::new);
            (*lbl, intersection)
        })
        .collect()
}

fn release_dead_refs_block(
    locals: &UniqueMap<Var, (Mutability, SingleType)>,
    locals_pre_state: &locals::state::LocalStates,
    liveness_pre_state: &LivenessState,
    forward_intersection: &BTreeSet<Var>,
    block: &mut BasicBlock,
) {
    if forward_intersection.is_empty() {
        return;
    }

    let cmd_loc = block.front().unwrap().loc;
    let cur_state = {
        let mut s = liveness_pre_state.clone();
        for cmd in block.iter().rev() {
            command(&mut s, cmd);
        }
        s
    };
    // Free references that were live in ALL predecessors and that have a value
    // (could not have a value due to errors)
    let dead_refs = forward_intersection
        .difference(&cur_state.0)
        .filter(|var| locals_pre_state.get_state(var).is_available())
        .map(|var| (var, locals.get(var).unwrap()))
        .filter(is_ref);
    for (dead_ref, (_, ty)) in dead_refs {
        block.push_front(pop_ref(cmd_loc, *dead_ref, ty.clone()));
    }
}

fn is_ref((_local, (_, sp!(_, local_ty_))): &(&Var, &(Mutability, SingleType))) -> bool {
    match local_ty_ {
        SingleType_::Ref(_, _) => true,
        SingleType_::Base(_) => false,
    }
}

fn pop_ref(loc: Loc, var: Var, ty: SingleType) -> Command {
    use Command_ as C;
    use UnannotatedExp_ as E;
    let move_e_ = E::Move {
        annotation: MoveOpAnnotation::InferredLastUsage,
        var,
    };
    let move_e = H::exp(Type_::single(ty), sp(loc, move_e_));
    let pop_ = C::IgnoreAndPop {
        pop_num: 1,
        exp: move_e,
    };
    sp(loc, pop_)
}
