// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Constant compilation for CFGIR: the global folding pass (`folded_constants`), which folds
//! every constant in the program up front in the order of the constants' own dependency graph,
//! and the selection pass (`generate_cross_module_constants`), which rewrites cross-module
//! constant references in translated function bodies into references to module-local copies of
//! the referenced constants, synthesized from their already-folded values.
// TODO(cross-module-constants): `compute_dependent_constants` and the selection walker below
// are hand-rolled HLIR walkers; there is no mutable HLIR visitor today. Consider a shared
// walker if another one appears.

use crate::{
    cfgir::{
        ast as G,
        translate::{
            Context, constant as fold_constant, move_value_from_value, outside_compilation_use,
            unfoldable_constant_use_error,
        },
    },
    diag,
    diagnostics::filter::{FilterScope, empty_filter_scope},
    expansion::ast::ModuleIdent,
    hlir::ast as H,
    ice, ice_assert,
    parser::ast::{ConstantName, FunctionName},
    shared::unique_map::UniqueMap,
};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use move_symbol_pool::Symbol;
use petgraph::{algo::kosaraju_scc as petgraph_scc, graphmap::DiGraphMap};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

//**************************************************************************************************
// Types
//**************************************************************************************************

pub(crate) type ConstantId = (ModuleIdent, ConstantName);

type ConstantValues = BTreeMap<ConstantId, H::Value>;

/// The fold state of a constant definition
pub(crate) enum ConstantEntry {
    /// Failed to fold to a value; an error was already reported at the definition
    Failed,
    /// Folded; the value is in the shared constant value map
    Defined {
        loc: Loc,
        signature: Box<H::BaseType>,
    },
}

/// The constant copies synthesized for one module. Note the copies have no counterpart in the
/// typing `ProgramInfo`.
struct NewConstants {
    copies: BTreeMap<ConstantId, ConstantName>,
    copy_defs: Vec<(ConstantName, G::Constant)>,
    copy_names: BTreeSet<Symbol>,
    /// continues after the module's own constants so copies are emitted after them
    next_index: usize,
}

//**************************************************************************************************
// Impls
//**************************************************************************************************

impl NewConstants {
    fn new(constants: &UniqueMap<ConstantName, G::Constant>) -> Self {
        Self {
            copies: BTreeMap::new(),
            copy_defs: vec![],
            copy_names: BTreeSet::new(),
            next_index: constants
                .key_cloned_iter()
                .map(|(_, cdef)| cdef.index + 1)
                .max()
                .unwrap_or(0),
        }
    }

    /// Resolves a cross-module constant use to a module-local copy of the constant, synthesizing
    /// the copy at its first use. Returns `None` if no copy can be made, reporting an error
    /// unless one was already reported at the definition.
    fn constant_copy(
        &mut self,
        context: &mut Context,
        constant_values: &ConstantValues,
        m: ModuleIdent,
        c: ConstantName,
        use_loc: Loc,
    ) -> Option<ConstantName> {
        if let Some(copy_name) = self.copies.get(&(m, c)) {
            return Some(*copy_name);
        }
        match context.constant_defs.get_key_value(&(m, c)) {
            Some((_, ConstantEntry::Defined { loc, signature })) => {
                let defined_loc = *loc;
                let signature = (**signature).clone();
                Some(self.synthesize_constant_copy(
                    context,
                    constant_values,
                    m,
                    c,
                    signature,
                    defined_loc,
                    use_loc,
                ))
            }
            Some(((_, defined), ConstantEntry::Failed)) => {
                let defined_loc = defined.0.loc;
                context.add_diag(unfoldable_constant_use_error(&m, &c, use_loc, defined_loc));
                None
            }
            None => {
                context.add_diag(outside_compilation_use(&m, &c, use_loc));
                None
            }
        }
    }

    /// Synthesizes a module-local copy of `m::c` from its already-folded value, named
    /// `_{module_id}_{module}_{const}` (e.g. `_3_b_C`). The module id -- the module's position
    /// in the compilation's module list, or a `p`-prefixed id for pre-compiled modules -- keeps
    /// the names collision-free, and the leading `_` avoids user constants. The names appear
    /// only in source maps.
    fn synthesize_constant_copy(
        &mut self,
        context: &mut Context,
        constant_values: &ConstantValues,
        m: ModuleIdent,
        c: ConstantName,
        signature: H::BaseType,
        defined_loc: Loc,
        use_loc: Loc,
    ) -> ConstantName {
        // pre-compiled modules use their own `p`-prefixed id namespace
        let module_id = match context.precompiled_module_ids.get(&m) {
            Some(id) => format!("p{}", id),
            None => match context.module_ids.get(&m) {
                Some(id) => id.to_string(),
                None => {
                    context.add_diag(ice!((use_loc, "module without an id in the compilation")));
                    "0".to_string()
                }
            },
        };
        let symbol: Symbol = format!("_{}_{}_{}", module_id, m.value.module, c).into();
        // the copy carries the original definition's loc so tooling points at the definition
        let copy_name = ConstantName(sp(defined_loc, symbol));
        let Some(value) = constant_values.get(&(m, c)) else {
            context.add_diag(ice!((use_loc, "defined constant with no folded value")));
            return copy_name;
        };
        let cdef = G::Constant {
            warning_filter: empty_filter_scope(),
            index: {
                let index = self.next_index;
                self.next_index += 1;
                index
            },
            attributes: UniqueMap::new(),
            loc: defined_loc,
            signature,
            value: Some(move_value_from_value(value.clone())),
        };
        if self.copy_names.insert(symbol) {
            debug_assert!(self.copies.insert((m, c), copy_name).is_none());
            self.copy_defs.push((copy_name, cdef));
        } else {
            context.add_diag(ice!((
                use_loc,
                format!("mangled constant copy name collision on '{}'", copy_name)
            )));
        }
        copy_name
    }
}

//**************************************************************************************************
// Constant folding
//**************************************************************************************************

/// Folds every constant in the program up front, in the order of the constants' own global
/// dependency graph, cross-module edges included; module order is irrelevant. Cycles (including
/// mutual recursion across modules) are reported here, and neither the constants in a cycle nor
/// their direct dependents fold; they are marked `Failed`. The remaining constants fold via a
/// work queue: a constant is ready once every constant it references has folded. Returns the
/// folded constants grouped by their defining module.
pub(super) fn folded_constants(
    context: &mut Context,
    hmodules: &mut [(ModuleIdent, H::ModuleDefinition)],
    constant_values: &mut ConstantValues,
) -> BTreeMap<ModuleIdent, UniqueMap<ConstantName, G::Constant>> {
    let (mut consts, module_scopes) = take_module_constants(hmodules);
    let dependencies = constant_dependencies(&consts);
    let failed = report_cycles(context, &consts, &dependencies);
    // anything in, or directly dependent on, a cycle does not fold; an error was reported above.
    // Take the key from `consts` so the recorded entry carries the definition's loc, which
    // use-site errors point at
    for node in &failed {
        let (key, _) = consts
            .remove_entry(node)
            .expect("ICE failed constant not in the compilation");
        context.constant_defs.insert(key, ConstantEntry::Failed);
    }

    // A constant waits on each of its (still-folding) dependencies; references to failed
    // constants are not waited on -- folding them reports an error at each use instead
    let mut blocked_on: BTreeMap<ConstantId, usize> = consts.keys().map(|n| (*n, 0)).collect();
    let mut dependents: BTreeMap<ConstantId, Vec<ConstantId>> = BTreeMap::new();
    for (user, deps) in &dependencies {
        if !consts.contains_key(user) {
            continue;
        }
        for dep in deps {
            if consts.contains_key(dep) {
                *blocked_on.get_mut(user).unwrap() += 1;
                dependents.entry(*dep).or_default().push(*user);
            }
        }
    }

    let mut ready: VecDeque<ConstantId> = blocked_on
        .iter()
        .filter_map(|(node, blockers)| (*blockers == 0).then_some(*node))
        .collect();
    let mut folded: BTreeMap<ModuleIdent, UniqueMap<ConstantName, G::Constant>> = BTreeMap::new();
    while let Some(node) = ready.pop_front() {
        let (mname, cname) = node;
        let cdef = consts.remove(&node).unwrap();
        // constants fold in their defining module's diagnostic scope
        let (package, filter) = module_scopes
            .get(&mname)
            .expect("ICE constant from module outside the compilation");
        context.current_package = *package;
        context.push_warning_filter_scope(filter.clone());
        let new_cdef = fold_constant(context, constant_values, mname, cname, cdef);
        context.pop_warning_filter_scope();
        context.current_package = None;
        folded
            .entry(mname)
            .or_default()
            .add(cname, new_cdef)
            .expect("ICE constant name collision");
        for user in dependents.remove(&node).unwrap_or_default() {
            let blockers = blocked_on.get_mut(&user).unwrap();
            *blockers -= 1;
            if *blockers == 0 {
                ready.push_back(user);
            }
        }
    }
    debug_assert!(consts.is_empty(), "ICE constant work queue did not drain");
    folded
}

/// Takes the constants out of their modules, remembering each module's package and warning
/// filter so its constants can fold in the module's diagnostic scope
fn take_module_constants(
    hmodules: &mut [(ModuleIdent, H::ModuleDefinition)],
) -> (
    BTreeMap<ConstantId, H::Constant>,
    BTreeMap<ModuleIdent, (Option<Symbol>, FilterScope)>,
) {
    let mut consts = BTreeMap::new();
    let mut module_scopes = BTreeMap::new();
    for (mname, mdef) in hmodules.iter_mut() {
        module_scopes.insert(*mname, (mdef.package_name, mdef.warning_filter.clone()));
        let mconstants = std::mem::replace(&mut mdef.constants, UniqueMap::new());
        for (cname, cdef) in mconstants {
            consts.insert((*mname, cname), cdef);
        }
    }
    (consts, module_scopes)
}

/// The constants each constant references, limited to constants of this compilation.
/// Precompiled constants are already folded values, so they are never waited on
fn constant_dependencies(
    consts: &BTreeMap<ConstantId, H::Constant>,
) -> BTreeMap<ConstantId, BTreeSet<ConstantId>> {
    consts
        .iter()
        .map(|(node, cdef)| {
            let deps = compute_dependent_constants(cdef)
                .into_iter()
                .filter(|dep| consts.contains_key(dep))
                .collect();
            (*node, deps)
        })
        .collect()
}

/// Reports every constant definition cycle (a self-reference, or a circular dependency possibly
/// spanning modules) and every direct dependent of a cycle. Returns all of the involved
/// constants; none of them can fold
fn report_cycles(
    context: &mut Context,
    consts: &BTreeMap<ConstantId, H::Constant>,
    dependencies: &BTreeMap<ConstantId, BTreeSet<ConstantId>>,
) -> BTreeSet<ConstantId> {
    fn display((m, c): &ConstantId) -> String {
        format!("{}::{}", m, c)
    }
    fn defined_loc(consts: &BTreeMap<ConstantId, H::Constant>, node: &ConstantId) -> Loc {
        consts.get_key_value(node).unwrap().0.1.0.loc
    }

    let mut graph = DiGraphMap::<ConstantId, ()>::new();
    for (node, deps) in dependencies {
        graph.add_node(*node);
        for dep in deps {
            graph.add_edge(*dep, *node, ());
        }
    }

    let sccs = petgraph_scc(&graph);
    let mut cycle_nodes = BTreeSet::new();
    for scc in sccs {
        // An SCC of size 1 is only a cycle if the node has a self-edge.
        if scc.len() == 1 && graph.contains_edge(scc[0], scc[0]) {
            let node = scc[0];
            context.add_diag(diag!(
                CodeGeneration::UnfoldableConstant,
                (
                    defined_loc(consts, &node),
                    format!("Constant '{}' references itself", display(&node)),
                )
            ));
            cycle_nodes.insert(node);
        } else if scc.len() > 1 {
            let names = scc.iter().map(display).collect::<Vec<_>>().join(", ");
            let mut diag = diag!(
                CodeGeneration::UnfoldableConstant,
                (
                    defined_loc(consts, &scc[0]),
                    format!("Constant definitions form a circular dependency: {}", names),
                )
            );
            for node in scc.iter().skip(1) {
                diag.add_secondary_label((
                    defined_loc(consts, node),
                    "Cyclic constant defined here",
                ));
            }
            context.add_diag(diag);
            cycle_nodes.append(&mut scc.into_iter().collect());
        }
    }
    // report any node that relies on a node in a cycle but is not itself part of that cycle
    let mut failed = cycle_nodes.clone();
    for cycle_node in cycle_nodes.iter() {
        // petgraph retains edges for nodes that have been deleted, so we ensure the node is not
        // part of a cycle _and_ it's still in the graph
        let neighbors: Vec<_> = graph
            .neighbors(*cycle_node)
            .filter(|node| !cycle_nodes.contains(node) && graph.contains_node(*node))
            .collect();
        for node in neighbors {
            context.add_diag(diag!(
                CodeGeneration::UnfoldableConstant,
                (
                    defined_loc(consts, &node),
                    format!(
                        "Constant uses constant {}, which has a circular dependency",
                        display(cycle_node)
                    )
                )
            ));
            graph.remove_node(node);
            failed.insert(node);
        }
        graph.remove_node(*cycle_node);
    }
    failed
}

//**************************************************************************************************
// Constant selection
//**************************************************************************************************

pub(super) fn generate_cross_module_constants(
    context: &mut Context,
    constant_values: &ConstantValues,
    module: ModuleIdent,
    constants: &mut UniqueMap<ConstantName, G::Constant>,
    functions: &mut UniqueMap<FunctionName, G::Function>,
) {
    let mut new_constants = NewConstants::new(constants);
    for (_, _, fdef) in functions.iter_mut() {
        let G::FunctionBody_::Defined { blocks, .. } = &mut fdef.body.value else {
            continue;
        };
        for (_, block) in blocks.iter_mut() {
            for cmd in block.iter_mut() {
                command(context, &mut new_constants, constant_values, module, cmd);
            }
        }
    }
    for (name, cdef) in new_constants.copy_defs {
        constants
            .add(name, cdef)
            .expect("ICE synthesized constant name collision");
    }
}

fn command(
    context: &mut Context,
    new_constants: &mut NewConstants,
    constant_values: &ConstantValues,
    module: ModuleIdent,
    sp!(_, cmd_): &mut H::Command,
) {
    use H::Command_ as C;
    match cmd_ {
        C::IgnoreAndPop { exp: e, .. }
        | C::Return { exp: e, .. }
        | C::Abort(_, e)
        | C::Assign(_, _, e)
        | C::JumpIf { cond: e, .. }
        | C::VariantSwitch { subject: e, .. } => {
            exp(context, new_constants, constant_values, module, e)
        }
        C::Mutate(lhs, rhs) => {
            exp(context, new_constants, constant_values, module, lhs);
            exp(context, new_constants, constant_values, module, rhs)
        }
        C::Break(_) | C::Continue(_) | C::Jump { .. } => (),
    }
}

#[growing_stack]
fn exp(
    context: &mut Context,
    new_constants: &mut NewConstants,
    constant_values: &ConstantValues,
    module: ModuleIdent,
    e: &mut H::Exp,
) {
    use H::UnannotatedExp_ as E;
    let eloc = e.exp.loc;
    match &mut e.exp.value {
        e_ @ E::Constant(_, _) => {
            let E::Constant(m, c) = e_ else {
                unreachable!()
            };
            let (m, c) = (*m, *c);
            if m == module {
                return;
            }
            if let Some(copy_name) =
                new_constants.constant_copy(context, constant_values, m, c, eloc)
            {
                *e_ = E::Constant(module, copy_name);
            } else {
                // an error was reported either at the constant's definition or at this use
                ice_assert!(
                    context.reporter(),
                    context.env.has_errors(),
                    eloc,
                    "cross-module constant use failed to resolve to a module-local copy"
                );
                *e_ = E::UnresolvedError;
            }
        }

        E::Unit { .. }
        | E::Value(_)
        | E::Move { .. }
        | E::Copy { .. }
        | E::ErrorConstant { .. }
        | E::BorrowLocal(_, _)
        | E::UnresolvedError
        | E::Unreachable => (),

        E::ModuleCall(mcall) => {
            for arg in &mut mcall.arguments {
                exp(context, new_constants, constant_values, module, arg);
            }
        }
        E::Freeze(base) | E::Dereference(base) | E::UnaryExp(_, base) | E::Cast(base, _) => {
            exp(context, new_constants, constant_values, module, base)
        }
        E::Borrow(_, base, _, _) => exp(context, new_constants, constant_values, module, base),
        E::BinopExp(lhs, _, rhs) => {
            exp(context, new_constants, constant_values, module, lhs);
            exp(context, new_constants, constant_values, module, rhs)
        }
        E::Pack(_, _, fields) | E::PackVariant(_, _, _, fields) => {
            for (_, _, fe) in fields {
                exp(context, new_constants, constant_values, module, fe);
            }
        }
        E::Vector(_, _, _, args) | E::Multiple(args) => {
            for arg in args {
                exp(context, new_constants, constant_values, module, arg);
            }
        }
    }
}

//**************************************************************************************************
// Constant dependencies
//**************************************************************************************************

fn compute_dependent_constants(constant: &H::Constant) -> BTreeSet<ConstantId> {
    fn dep_exp(set: &mut BTreeSet<ConstantId>, exp: &H::Exp) {
        use H::UnannotatedExp_ as E;
        match &exp.exp.value {
            E::UnresolvedError
            | E::Unreachable
            | E::Unit { .. }
            | E::Value(_)
            | E::Move { .. }
            | E::Copy { .. } => (),
            E::UnaryExp(_, rhs) => dep_exp(set, rhs),
            E::BinopExp(lhs, _, rhs) => {
                dep_exp(set, lhs);
                dep_exp(set, rhs)
            }
            E::Cast(base, _) => dep_exp(set, base),
            E::Vector(_, _, _, args) | E::Multiple(args) => {
                for arg in args {
                    dep_exp(set, arg);
                }
            }
            E::Constant(m, c) => {
                set.insert((*m, *c));
            }
            _ => panic!("ICE typing should have rejected exp in const"),
        }
    }

    fn dep_cmd(set: &mut BTreeSet<ConstantId>, command: &H::Command_) {
        use H::Command_ as C;
        match command {
            C::IgnoreAndPop { exp, .. } => dep_exp(set, exp),
            C::Return { exp, .. } => dep_exp(set, exp),
            C::Abort(_, exp) | C::Assign(_, _, exp) => dep_exp(set, exp),
            C::Mutate(lhs, rhs) => {
                dep_exp(set, lhs);
                dep_exp(set, rhs)
            }
            C::Break(_)
            | C::Continue(_)
            | C::Jump { .. }
            | C::JumpIf { .. }
            | C::VariantSwitch { .. } => (),
        }
    }

    fn dep_stmt(set: &mut BTreeSet<ConstantId>, stmt: &H::Statement_) {
        use H::Statement_ as S;
        match stmt {
            S::Command(cmd) => dep_cmd(set, &cmd.value),
            S::IfElse {
                cond,
                if_block,
                else_block,
            } => {
                dep_exp(set, cond);
                dep_block(set, if_block);
                dep_block(set, else_block)
            }
            S::VariantMatch {
                subject,
                enum_name: _,
                arms,
            } => {
                dep_exp(set, subject);
                for (_, arm) in arms {
                    dep_block(set, arm);
                }
            }
            S::While {
                cond: (cond_block, cond_exp),
                block,
                ..
            } => {
                dep_block(set, cond_block);
                dep_exp(set, cond_exp);
                dep_block(set, block)
            }
            S::Loop { block, .. } => dep_block(set, block),
            S::NamedBlock { block, .. } => dep_block(set, block),
        }
    }

    fn dep_block(set: &mut BTreeSet<ConstantId>, block: &H::Block) {
        for entry in block {
            dep_stmt(set, &entry.value);
        }
    }

    let mut output = BTreeSet::new();
    let (_, block) = &constant.value;
    dep_block(&mut output, block);
    output
}
