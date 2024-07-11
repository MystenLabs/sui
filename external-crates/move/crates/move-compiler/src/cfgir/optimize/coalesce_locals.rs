// Copyright (c) The Move Contributorstrue
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::{cfg::MutForwardCFG, liveness},
    expansion::ast::Mutability,
    hlir::ast::{
        Command, Command_, Exp, FunctionSignature, LValue, LValue_, Label, SingleType,
        UnannotatedExp_, Var,
    },
    parser::ast::Ability_,
    shared::{string_utils::format_oxford_list, unique_map::UniqueMap, Name},
};
use std::collections::{BTreeMap, BTreeSet};

use heuristic_graph_coloring as coloring;
use move_proc_macros::growing_stack;
use move_symbol_pool::Symbol;

/// DEBUG flag for general printing. Defined as a const so the code will be eliminated by the rustc
/// optimizer when debugging is not required.
const DEBUG_COALESCE: bool = false;

//**************************************************************************************************
// Entry
//**************************************************************************************************

/// returns true if anything changed
pub fn optimize(
    signature: &FunctionSignature,
    infinite_loop_starts: &BTreeSet<Label>,
    locals: UniqueMap<Var, (Mutability, SingleType)>,
    cfg: &mut MutForwardCFG,
) -> Option<UniqueMap<Var, (Mutability, SingleType)>> {
    macro_rules! unique_add_or_error {
        ($step:expr, $map:expr, $var:expr, $mut:expr, $ty:expr) => {{
            if let Err(_) = $map.add($var, ($mut, $ty)) {
                eprintln!(
                    "Error in coalescing locals for local {} at {}",
                    $var.value(),
                    $step
                );
                return None;
            }
        }};
    }

    // Get the conlfict graph
    cfg.recompute(); // Recompute the CFG first because optimizatins may have changed things.
    let (_final_invariants, per_command_states) = liveness::analyze(cfg, infinite_loop_starts);

    let conflict_graph = conflicts::Graph::new(per_command_states, cfg);

    if DEBUG_COALESCE {
        println!("\n\ngraph: ");
        for (entry, conflicts) in conflict_graph.iter() {
            print!("{} -> ", entry.value());
            match conflicts {
                conflicts::Conflicts::Set(vars) => {
                    let var_string = vars
                        .iter()
                        .map(|x| format!("{}", x))
                        .collect::<Vec<_>>()
                        .join(", ");
                    println!("{{{}}}", var_string);
                }
                conflicts::Conflicts::All => println!("FULL"),
            }
        }
    }

    let mut new_locals = UniqueMap::new();

    // Set of things that cannot be coalesced.
    let uncoalesced_set = uncoalescable_vars(&conflict_graph, &signature.parameters, &locals);
    if DEBUG_COALESCE {
        println!(
            "\n\nuncoalescable: {}",
            format_oxford_list!("and", "{}", &uncoalesced_set)
        );
    }

    let mut locals_by_type = BTreeMap::new();
    for (var, (mut_, ty)) in locals.clone().into_iter() {
        // Add the name-retaining variables to the rename map. This also inlcudes all the
        // parameters we've seen.
        if uncoalesced_set.contains(&var) {
            unique_add_or_error!("all_conflicting", new_locals, var, mut_, ty.clone());
            continue;
        } else {
            locals_by_type
                .entry(ty.clone())
                .or_insert(BTreeSet::new())
                .insert(var);
        }
    }

    // If there are no locals to coalesce, return.
    if locals_by_type.is_empty() {
        return None;
    };

    let mut rename_map = BTreeMap::new();

    for (ndx, (ty, local_set)) in locals_by_type.iter().enumerate() {
        let mut graph = coloring::VecVecGraph::new(local_set.len());
        let var_index_map = local_set
            .iter()
            .enumerate()
            .map(|(n, v)| (*v, n))
            .collect::<BTreeMap<_, _>>();
        for (var, conflict) in local_set.iter().filter_map(|var| {
            conflict_graph
                .get_conflicts(var)
                .map(|conflict| (var, conflict))
        }) {
            match conflict {
                conflicts::Conflicts::Set(vars) => {
                    let index = var_index_map.get(var).unwrap();
                    let relevant_vars = vars.intersection(local_set).collect::<BTreeSet<_>>();
                    for var in relevant_vars {
                        graph.add_edge(*index, *var_index_map.get(var).unwrap());
                    }
                }
                conflicts::Conflicts::All => unreachable!(),
            }
        }

        let coloring = coloring::color_greedy_by_degree(&graph);
        let mut color_sets: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
        for (ndx, color) in coloring.iter().enumerate() {
            color_sets.entry(*color).or_default().insert(ndx);
        }

        let index_var_map = var_index_map
            .into_iter()
            .map(|(v, n)| (n, v))
            .collect::<BTreeMap<_, _>>();
        for (color, mut var_indicies) in color_sets {
            if DEBUG_COALESCE {
                println!(
                    "color {color} -> {}",
                    format_oxford_list!(
                        "and",
                        "{}",
                        var_indicies
                            .iter()
                            .map(|ndx| index_var_map.get(ndx).unwrap().value())
                            .collect::<Vec<_>>()
                    )
                );
            }

            let Some(first_ndx) = var_indicies.pop_first() else {
                eprintln!("COMPILER BUG: Found color with no entries during coaslescing");
                continue;
            };
            if var_indicies.is_empty() {
                let var = index_var_map.get(&first_ndx).unwrap();
                let (mut_, ty) = locals.get(var).unwrap();
                unique_add_or_error!("single recolor", new_locals, *var, *mut_, ty.clone());
            } else {
                let first_var: Var = *index_var_map.get(&first_ndx).unwrap();
                let var_name: Symbol = format!("%local#{ndx}#{color}").into();
                unique_add_or_error!(
                    "post-color collection",
                    new_locals,
                    Var(Name::new(first_var.loc(), var_name)),
                    Mutability::Either,
                    ty.clone()
                );
                rename_map.insert(first_var, Var(Name::new(first_var.loc(), var_name)));
                for index in var_indicies {
                    let var: Var = *index_var_map.get(&index).unwrap();
                    let name: Name = Name::new(var.loc(), format!("%local#{ndx}#{color}").into());
                    let None = rename_map.insert(var, Var(name)) else {
                        eprintln!("badness in coalescing variables");
                        continue;
                    };
                }
            }
        }
    }

    if DEBUG_COALESCE {
        println!("Renames: ");
        for (from, to) in &rename_map {
            println!("{} -> {}", from.value(), to.value());
        }

        print!("New locals: ");
        println!(
            "{}",
            format_oxford_list!("and", "{}", new_locals.key_cloned().collect::<Vec<_>>())
        );
    }

    if rename_map.is_empty() {
        if DEBUG_COALESCE {
            println!("-- false ---------------------------");
        }
        None
    } else {
        coalesce(cfg, &rename_map);
        if DEBUG_COALESCE {
            println!("-- done (new locals: {:000})--------", new_locals.len());
        }
        Some(new_locals)
    }
}

/// Collect everything that _must_ keep its name, and can't be coalesced. This includes:
/// - All parameters, as they are not locals.
/// - Any local that was marked as Conflits::All in the conflict graph, indicating it is
///   borrowed at some point in the function. We don't have enough information (here) to
///   determine if coalescing is valid, so we simply do not.
/// - Any local that is not used, but whose type does not have `drop`. These are things that
///   should have been dropped by the function, but were not for some reason. In some cases this
///   is acceptable (if the block ends in an abort), but coalescing will result in attempting to
///   store to a local with a non-droppable value, which is an error.
fn uncoalescable_vars(
    conflict_graph: &conflicts::Graph,
    parameters: &[(Mutability, Var, SingleType)],
    locals: &UniqueMap<Var, (Mutability, SingleType)>,
) -> BTreeSet<Var> {
    let param_set = parameters
        .iter()
        .map(|(_, var, _)| var)
        .collect::<BTreeSet<_>>();

    let is_param = |var: &Var| param_set.contains(var);
    let is_borrowed = |var: &Var| {
        matches!(
            conflict_graph.get_conflicts(var),
            Some(conflicts::Conflicts::All)
        )
    };
    let is_unused_without_drop = |var: &Var, ty: &SingleType| {
        let result =
            !conflict_graph.used(var) && !ty.value.abilities(ty.loc).has_ability_(Ability_::Drop);
        if DEBUG_COALESCE && result {
            println!("{} is unused without drop", var);
        }
        result
    };

    // Set of things that cannot be coalesced.
    locals
        .key_cloned_iter()
        .filter(|(var, (_mut, ty))| {
            is_param(var) || is_borrowed(var) || is_unused_without_drop(var, ty)
        })
        .map(|(var, _)| var)
        .collect::<BTreeSet<_>>()
}

//**************************************************************************************************
// Conflict Graph
//**************************************************************************************************

mod conflicts {
    use move_proc_macros::growing_stack;
    use std::collections::{BTreeMap, BTreeSet};

    use crate::{
        cfgir::{cfg::MutForwardCFG, liveness::PerCommandStates},
        hlir::ast::{Command, Command_, Exp, LValue, LValue_, UnannotatedExp_, UnpackType, Var},
    };

    #[derive(Debug)]
    pub enum Conflicts {
        /// Set of variables in conflict with a variable
        Set(BTreeSet<Var>),
        /// Any variable that has references used is marked as conflicting with everything
        All,
    }

    #[derive(Debug)]
    pub struct Graph {
        graph: BTreeMap<Var, Conflicts>,
        used: BTreeSet<Var>,
    }

    impl Conflicts {
        fn add_conflict(&mut self, var: Var) {
            match self {
                Conflicts::Set(vars) => {
                    let _ = vars.insert(var);
                }
                Conflicts::All => (),
            }
        }

        fn set_to_all(&mut self) {
            *self = Conflicts::All
        }
    }

    impl Graph {
        pub fn new(per_command_states: PerCommandStates, cfg: &mut MutForwardCFG) -> Self {
            let mut graph = Graph {
                graph: BTreeMap::new(),
                used: BTreeSet::new(),
            };

            for (lbl, commands) in cfg.blocks() {
                let per_command_states = per_command_states.get(lbl).unwrap();
                assert_eq!(commands.len(), per_command_states.len());
                for (cmd, lives) in commands.iter().zip(per_command_states) {
                    command(&mut graph, &lives.live_set, cmd);
                }
            }
            for (_, states) in per_command_states {
                for state in states {
                    for var in &state.live_set {
                        graph.add_conflicts(*var, &state.live_set);
                    }
                }
            }
            graph
        }

        fn add_conflict(&mut self, x: Var, y: Var) {
            if x == y {
                return;
            } // Variables are implicitly in conflict with themselves.
            let x_conflicts = self
                .graph
                .entry(x)
                .or_insert(Conflicts::Set(BTreeSet::new()));
            x_conflicts.add_conflict(y);
            let y_conflicts = self
                .graph
                .entry(y)
                .or_insert(Conflicts::Set(BTreeSet::new()));
            y_conflicts.add_conflict(x);
        }

        fn add_conflicts(&mut self, var: Var, vars: &BTreeSet<Var>) {
            for other in vars {
                self.add_conflict(var, *other);
            }
        }

        /// Marks a variable as a reference (ALL conflicted). Also marks usage.
        pub fn mark_referenced(&mut self, var: Var) {
            self.mark_usage(var);
            self.graph.entry(var).or_insert(Conflicts::All).set_to_all()
        }

        /// Marks a variable as  used.
        fn mark_usage(&mut self, var: Var) {
            self.used.insert(var);
        }

        pub fn iter(&self) -> std::collections::btree_map::Iter<'_, Var, Conflicts> {
            self.graph.iter()
        }

        pub fn get_conflicts(&self, var: &Var) -> Option<&Conflicts> {
            self.graph.get(var)
        }

        pub fn used(&self, var: &Var) -> bool {
            self.used.contains(var)
        }
    }

    #[growing_stack]
    fn command(conflict_graph: &mut Graph, live_set: &BTreeSet<Var>, sp!(_, cmd_): &Command) {
        use Command_ as C;
        match cmd_ {
            C::Assign(_, ls, e) => {
                lvalues(conflict_graph, live_set, e, ls);
                exp(conflict_graph, live_set, e);
            }
            C::Mutate(el, er) => {
                exp(conflict_graph, live_set, er);
                exp(conflict_graph, live_set, el)
            }
            C::Return { exp: e, .. }
            | C::Abort(e)
            | C::IgnoreAndPop { exp: e, .. }
            | C::JumpIf { cond: e, .. }
            | C::VariantSwitch { subject: e, .. } => exp(conflict_graph, live_set, e),

            C::Jump { .. } => (),
            C::Break(_) | C::Continue(_) => panic!("ICE break/continue not translated to jumps"),
        }
    }

    fn lvalues(conflict_graph: &mut Graph, live_set: &BTreeSet<Var>, rhs: &Exp, ls: &[LValue]) {
        let lvalue_vars = ls
            .iter()
            .flat_map(|l| lvalue_vars(conflict_graph, rhs, l))
            .collect::<BTreeSet<Var>>();
        let all_vars = live_set
            .iter()
            .cloned()
            .chain(lvalue_vars)
            .collect::<BTreeSet<_>>();
        for var in &all_vars {
            conflict_graph.add_conflicts(*var, &all_vars);
        }
    }

    fn lvalue_vars(conflict_graph: &mut Graph, rhs: &Exp, sp!(_, l_): &LValue) -> BTreeSet<Var> {
        use LValue_ as L;
        match l_ {
            L::Ignore => BTreeSet::new(),
            L::Var { var, .. } => BTreeSet::from([*var]),
            L::Unpack(_, _, fields) => fields
                .iter()
                .flat_map(|(_, l)| lvalue_vars(conflict_graph, rhs, l))
                .collect::<BTreeSet<_>>(),
            L::UnpackVariant(_, _, unpack_type, _, _, fields) => {
                let unpack_vars = fields
                    .iter()
                    .flat_map(|(_, l)| lvalue_vars(conflict_graph, rhs, l))
                    .collect::<BTreeSet<_>>();
                match unpack_type {
                    UnpackType::ByValue => (),
                    // The unpack implicitly borrows everything from the the right-hand side
                    // expression. NB: this is pessimistic, but sufficient. Also, match compilation
                    // is the only thing that generates these forms, and every RHS _should_ be a
                    // variable.
                    UnpackType::ByImmRef | UnpackType::ByMutRef => {
                        let rhs_frees = rhs.free_vars();
                        for var in &unpack_vars {
                            conflict_graph.add_conflicts(*var, &rhs_frees);
                        }
                    }
                };
                unpack_vars
            }
        }
    }

    #[growing_stack]
    fn exp(conflict_graph: &mut Graph, live_set: &BTreeSet<Var>, parent_e: &Exp) {
        use UnannotatedExp_ as E;
        match &parent_e.exp.value {
            E::Unit { .. }
            | E::Value(_)
            | E::Constant(_)
            | E::UnresolvedError
            | E::ErrorConstant { .. } => (),

            E::BorrowLocal(_, var) => {
                // If we borrow a local, we mark it as referenced.
                conflict_graph.mark_referenced(*var);
            }
            // NB: Copy does not indicate 'usage', as we only care about usage when we're  dealing
            // with undroppable values.
            E::Copy { var, .. } => conflict_graph.add_conflicts(*var, live_set),
            E::Move { var, .. } => {
                conflict_graph.mark_usage(*var);
                conflict_graph.add_conflicts(*var, live_set);
            }
            E::ModuleCall(mcall) => mcall
                .arguments
                .iter()
                .for_each(|e| exp(conflict_graph, live_set, e)),
            E::Vector(_, _, _, args) => args.iter().for_each(|e| exp(conflict_graph, live_set, e)),
            E::Freeze(e)
            | E::Dereference(e)
            | E::UnaryExp(_, e)
            | E::Borrow(_, e, _, _)
            | E::Cast(e, _) => exp(conflict_graph, live_set, e),

            E::BinopExp(e1, _, e2) => {
                exp(conflict_graph, live_set, e1);
                exp(conflict_graph, live_set, e2)
            }

            E::Pack(_, _, fields) => fields
                .iter()
                .for_each(|(_, _, e)| exp(conflict_graph, live_set, e)),
            E::PackVariant(_, _, _, fields) => fields
                .iter()
                .for_each(|(_, _, e)| exp(conflict_graph, live_set, e)),

            E::Multiple(es) => es.iter().for_each(|e| exp(conflict_graph, live_set, e)),

            E::Unreachable => panic!("ICE should not analyze dead code"),
        }
    }
}

//**************************************************************************************************
// Coalesce
//**************************************************************************************************

struct Context<'a> {
    var_map: &'a BTreeMap<Var, Var>,
}

fn coalesce(cfg: &mut MutForwardCFG, var_map: &BTreeMap<Var, Var>) {
    let context = &Context { var_map };
    for block in cfg.blocks_mut().values_mut() {
        for cmd in block {
            command(context, cmd);
        }
    }
}

#[growing_stack]
fn command(context: &Context, sp!(_, cmd_): &mut Command) {
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
        | C::Abort(e)
        | C::IgnoreAndPop { exp: e, .. }
        | C::JumpIf { cond: e, .. }
        | C::VariantSwitch { subject: e, .. } => exp(context, e),

        C::Jump { .. } => (),
        C::Break(_) | C::Continue(_) => panic!("ICE break/continue not translated to jumps"),
    }
}

fn lvalues(context: &Context, ls: &mut [LValue]) {
    ls.iter_mut().for_each(|l| lvalue(context, l))
}

fn lvalue(context: &Context, l: &mut LValue) {
    use LValue_ as L;
    match &mut l.value {
        L::Ignore => (),
        L::Var { var, .. } => {
            if let Some(new_var) = context.var_map.get(var) {
                *var = *new_var;
            }
        }
        L::Unpack(_, _, fields) => fields.iter_mut().for_each(|(_, l)| lvalue(context, l)),
        L::UnpackVariant(_, _, _, _, _, fields) => {
            fields.iter_mut().for_each(|(_, l)| lvalue(context, l))
        }
    }
}

#[growing_stack]
fn exp(context: &Context, e: &mut Exp) {
    use UnannotatedExp_ as E;
    match &mut e.exp.value {
        E::Unit { .. }
        | E::Value(_)
        | E::Constant(_)
        | E::UnresolvedError
        | E::ErrorConstant { .. } => (),

        E::BorrowLocal(_, var) | E::Move { var, .. } | E::Copy { var, .. } => {
            if let Some(new_var) = context.var_map.get(var) {
                *var = *new_var;
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
