// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Constant compilation:
//! - `seed_precompiled_constants` creates initial values
//! - `folded_constants` computes the folded constant values in dependency order
//! - `generate_cross_module_constants` copies per-module constants to discharge
//!   cross-module references
// TODO(cross-module-constants): `compute_dependent_constants` and the generation walker below
// are hand-rolled HLIR walkers; there is no mutable HLIR visitor today. Consider a shared
// walker if another one appears.

use crate::{
    cfgir::{
        self, ast as G,
        translate::{Context, lower_body_to_cfg, move_value_from_value},
    },
    diag,
    diagnostics::{
        Diagnostic,
        filter::{FilterScope, empty_filter_scope},
    },
    expansion::ast::{Attributes, ModuleIdent, Mutability},
    hlir::{
        ast as H,
        translate::{NEW_NAME_DELIM, precompiled_constants},
    },
    ice, ice_assert,
    parser::ast::{ConstantName, FunctionName},
    shared::unique_map::UniqueMap,
};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use move_symbol_pool::Symbol;
use petgraph::{algo::kosaraju_scc as petgraph_scc, graphmap::DiGraphMap};
use std::collections::{BTreeMap, BTreeSet};

//**************************************************************************************************
// Types
//**************************************************************************************************

type ConstantId = (ModuleIdent, ConstantName);

pub(super) type ConstantValues = BTreeMap<ConstantId, H::Value>;

/// The fold state of a constant definition
enum ConstantEntry {
    /// Constant that failed to fold (due to error, etc)
    Failed,
    /// A precompiled constant with no usable value
    PrecompiledFailed,
    /// Folded; the value is in the shared constant value map
    Defined {
        loc: Loc,
        signature: Box<H::BaseType>,
    },
}

pub(super) struct ConstantContext {
    defs: BTreeMap<ConstantId, ConstantEntry>,
}

/// The constant copies synthesized for one module.
struct NewConstants {
    copies: BTreeMap<ConstantId, ConstantName>,
    copy_defs: Vec<(ConstantName, G::Constant)>,
    /// Continues after the module's own constants so copies are emitted after them.
    /// Computed as `max(index) + 1` rather than `len()`, because failed constants
    /// are absent from this map.
    next_index: usize,
}

//**************************************************************************************************
// Impls
//**************************************************************************************************

impl ConstantContext {
    pub(super) fn new() -> Self {
        Self {
            defs: BTreeMap::new(),
        }
    }
}

impl NewConstants {
    fn new(constants: &UniqueMap<ConstantName, G::Constant>) -> Self {
        Self {
            copies: BTreeMap::new(),
            copy_defs: vec![],
            next_index: constants
                .key_cloned_iter()
                .map(|(_, cdef)| cdef.index + 1)
                .max()
                .unwrap_or(0),
        }
    }

    /// Resolves a cross-module constant use to a module-local copy of the constant, synthesizing
    /// the copy at its first use. Returns `None` if no copy can be made (e.g., the constant failed
    /// during folding, etc).
    fn constant_copy(
        &mut self,
        context: &mut Context,
        constant_context: &mut ConstantContext,
        constant_values: &ConstantValues,
        m: ModuleIdent,
        c: ConstantName,
        use_loc: Loc,
    ) -> Option<ConstantName> {
        if let Some(copy_name) = self.copies.get(&(m, c)) {
            return Some(*copy_name);
        }
        match constant_context.defs.get_key_value(&(m, c)) {
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
            Some(((_, defined), ConstantEntry::PrecompiledFailed)) => {
                let defined_loc = defined.0.loc;
                context.add_diag(unfoldable_constant_use_error(&m, &c, use_loc, defined_loc));
                None
            }
            Some((_, ConstantEntry::Failed)) => {
                // an error was already reported at the definition in this compilation
                ice_assert!(
                    context.reporter(),
                    context.env.has_errors(),
                    use_loc,
                    "cross-module constant use of a failed constant"
                );
                None
            }
            None => {
                // a missing entry means typing already errored on this use (due to visibility
                // or similar)
                ice_assert!(
                    context.reporter(),
                    context.env.has_errors(),
                    use_loc,
                    "cross-module constant use of an unknown constant"
                );
                None
            }
        }
    }

    /// Synthesizes a module-local copy of `m::c` from its already-folded value, named
    /// `const#{module}#{const}` (e.g. `const#b#C`).
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
        let symbol: Symbol = format!(
            "const{NEW_NAME_DELIM}{}{NEW_NAME_DELIM}{}",
            m.value.module, c
        )
        .into();
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
        let prev = self.copies.insert((m, c), copy_name);
        ice_assert!(
            context.reporter(),
            prev.is_none(),
            use_loc,
            "duplicate cross-module constant copy"
        );
        self.copy_defs.push((copy_name, cdef));
        copy_name
    }
}

//**************************************************************************************************
// Pre-compiled constants
//**************************************************************************************************

/// Constants of modules outside this compilation (e.g. the analyzer's pre-compiled dependencies)
/// were folded when they were compiled; seed their values.
pub(super) fn seed_precompiled_constants(
    context: &mut Context,
    constant_context: &mut ConstantContext,
    hmodules: &[(ModuleIdent, H::ModuleDefinition)],
    constant_values: &mut ConstantValues,
) {
    let compiled: BTreeSet<ModuleIdent> = hmodules.iter().map(|(mname, _)| *mname).collect();
    // constants are only visible within their package, so only modules from the packages being
    // compiled can contribute
    let packages: BTreeSet<Option<Symbol>> =
        hmodules.iter().map(|(_, mdef)| mdef.package_name).collect();
    let precompiled = precompiled_constants(context.reporter(), context.info, &compiled, &packages);
    for ((mident, cname), (defined_loc, signature, value)) in precompiled {
        let entry = match value {
            Some(value) => {
                constant_values.insert((mident, cname), value);
                ConstantEntry::Defined {
                    loc: defined_loc,
                    signature: Box::new(signature),
                }
            }
            None => ConstantEntry::PrecompiledFailed,
        };
        constant_context.defs.insert((mident, cname), entry);
    }
}

//**************************************************************************************************
// Constant folding
//**************************************************************************************************

/// Folds every constant in the program up front, in the order of the constants' own global
/// dependency graph. Constant definition cycles are also reported here.
/// Returns the constants grouped by defining module.
pub(super) fn compute_folded_constants(
    context: &mut Context,
    constant_context: &mut ConstantContext,
    hmodules: &mut [(ModuleIdent, H::ModuleDefinition)],
    constant_values: &mut ConstantValues,
) -> BTreeMap<ModuleIdent, UniqueMap<ConstantName, G::Constant>> {
    let (mut consts, module_scopes) = take_module_constants(hmodules);
    let dependencies = constant_dependencies(&consts);
    // dependency graph: an edge dep -> user means `user` folds after `dep`
    let mut graph = DiGraphMap::<ConstantId, ()>::new();
    for (node, deps) in &dependencies {
        graph.add_node(*node);
        for dep in deps {
            graph.add_edge(*dep, *node, ());
        }
    }
    // Kosaraju returns in reverse-topological order. We traverse them in reverse so we always
    // handle any dependencies before their usage.
    let sccs = petgraph_scc(&graph);
    let mut folded: BTreeMap<ModuleIdent, UniqueMap<ConstantName, G::Constant>> = BTreeMap::new();
    for scc in sccs.into_iter().rev() {
        if scc.len() > 1 || graph.contains_edge(scc[0], scc[0]) {
            report_cycle(context, &consts, &scc);
            for node in scc {
                let (key, _) = consts
                    .remove_entry(&node)
                    .expect("ICE cycle constant not in the compilation");
                constant_context.defs.insert(key, ConstantEntry::Failed);
            }
            continue;
        }
        let node = scc[0];
        // Referring to a failed constant propagates the failure.
        // We do not issue a second error, however.
        // TODO: We _could_ keep that original diagnostic and add this location as a note?
        let dep_failed = dependencies[&node]
            .iter()
            .any(|dep| matches!(constant_context.defs.get(dep), Some(ConstantEntry::Failed)));
        if dep_failed {
            let (key, _) = consts.remove_entry(&node).unwrap();
            constant_context.defs.insert(key, ConstantEntry::Failed);
            continue;
        }
        let (mname, cname) = node;
        let cdef = consts.remove(&node).unwrap();
        // constants fold in their defining module's diagnostic scope
        let (package, filter) = module_scopes
            .get(&mname)
            .expect("ICE constant from module outside the compilation");
        context.current_package = *package;
        context.push_warning_filter_scope(filter.clone());
        let new_cdef = fold_constant(
            context,
            constant_context,
            constant_values,
            mname,
            cname,
            cdef,
        );
        context.pop_warning_filter_scope();
        context.current_package = None;
        folded
            .entry(mname)
            .or_default()
            .add(cname, new_cdef)
            .expect("ICE constant name collision");
    }
    debug_assert!(
        consts.is_empty(),
        "ICE constant fold did not visit all constants"
    );
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
/// Precompiled constants are already folded values, so they never contribute to the fold order
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

fn fold_constant(
    context: &mut Context,
    constant_context: &mut ConstantContext,
    constant_values: &mut ConstantValues,
    module: ModuleIdent,
    name: ConstantName,
    c: H::Constant,
) -> G::Constant {
    let H::Constant {
        warning_filter,
        index,
        attributes,
        loc,
        signature,
        value: (locals, block),
    } = c;

    context.push_warning_filter_scope(warning_filter.clone());
    let final_value = fold_constant_impl(
        context,
        constant_context,
        constant_values,
        module,
        name,
        loc,
        &attributes,
        signature.clone(),
        locals,
        block,
    );
    let entry = match final_value {
        Some(H::Exp {
            exp: sp!(_, H::UnannotatedExp_::Value(value)),
            ..
        }) => {
            let prev = constant_values.insert((module, name), value.clone());
            ice_assert!(
                context.reporter(),
                prev.is_none(),
                loc,
                "constant name collision"
            );
            (
                ConstantEntry::Defined {
                    loc,
                    signature: Box::new(signature.clone()),
                },
                Some(move_value_from_value(value)),
            )
        }
        // an error was reported during folding, at this definition or an earlier one
        _ => (ConstantEntry::Failed, None),
    };
    let (entry, value) = entry;
    let prev_entry = constant_context.defs.insert((module, name), entry);
    debug_assert!(prev_entry.is_none());

    context.pop_warning_filter_scope();
    G::Constant {
        warning_filter,
        index,
        attributes,
        loc,
        signature,
        value,
    }
}

fn fold_constant_impl(
    context: &mut Context,
    constant_context: &ConstantContext,
    constant_values: &ConstantValues,
    module: ModuleIdent,
    name: ConstantName,
    full_loc: Loc,
    attributes: &Attributes,
    signature: H::BaseType,
    locals: UniqueMap<H::Var, (Mutability, H::SingleType)>,
    body: H::Block,
) -> Option<H::Exp> {
    use H::Command_ as C;
    const ICE_MSG: &str = "ICE invalid constant should have been blocked in typing";
    let (start, mut blocks, _block_info) = lower_body_to_cfg(
        context,
        body,
        |context, cfg, infinite_loop_starts, errors| {
            assert!(infinite_loop_starts.is_empty(), "{}", ICE_MSG);
            assert!(errors.is_empty(), "{}", ICE_MSG);

            let num_previous_errors = context.env.count_diags();
            let fake_signature = H::FunctionSignature {
                type_parameters: vec![],
                parameters: vec![],
                return_type: H::Type_::base(signature),
            };
            let fake_infinite_loop_starts = BTreeSet::new();
            let function_context = cfgir::CFGContext {
                env: context.env,
                pre_compiled_program: None,
                reporter: context.reporter(),
                info: context.info,
                package: context.current_package,
                module,
                member: cfgir::MemberName::Constant(name.0),
                attributes,
                entry: None,
                visibility: H::Visibility::Internal,
                signature: &fake_signature,
                locals: &locals,
                infinite_loop_starts: &fake_infinite_loop_starts,
            };
            cfgir::refine_inference_and_verify(&function_context, cfg);
            ice_assert!(
                context.reporter(),
                num_previous_errors == context.env.count_diags(),
                full_loc,
                "{}",
                ICE_MSG
            );
            cfgir::optimize(
                context.env,
                context.reporter(),
                context.current_package,
                &fake_signature,
                &locals,
                constant_values,
                cfg,
            );
        },
    );

    if blocks.len() != 1 {
        let exps = blocks
            .values()
            .flat_map(|block| block.iter())
            .flat_map(|cmd| command_exps(&cmd.value));
        report_cannot_fold(context, constant_context, module, full_loc, exps);
        return None;
    }
    let mut optimized_block = blocks.remove(&start).unwrap();
    let return_cmd = optimized_block.pop_back().unwrap();
    for sp!(cloc, cmd_) in &optimized_block {
        let e = match cmd_ {
            C::IgnoreAndPop { exp, .. } => exp,
            _ => {
                report_cannot_fold(context, constant_context, module, *cloc, command_exps(cmd_));
                continue;
            }
        };
        check_constant_value(context, constant_context, module, e)
    }

    let result = match return_cmd.value {
        C::Return { exp: e, .. } => e,
        _ => unreachable!(),
    };
    check_constant_value(context, constant_context, module, &result);
    Some(result)
}

fn check_constant_value(
    context: &mut Context,
    constant_context: &ConstantContext,
    module: ModuleIdent,
    e: &H::Exp,
) {
    use H::UnannotatedExp_ as E;
    match &e.exp.value {
        E::Value(_) => (),
        _ => report_cannot_fold(
            context,
            constant_context,
            module,
            e.exp.loc,
            std::iter::once(e),
        ),
    }
}

fn command_exps(cmd_: &H::Command_) -> impl Iterator<Item = &H::Exp> {
    use H::Command_ as C;
    let exps: Vec<&H::Exp> = match cmd_ {
        C::IgnoreAndPop { exp, .. }
        | C::Return { exp, .. }
        | C::Abort(_, exp)
        | C::Assign(_, _, exp)
        | C::JumpIf { cond: exp, .. }
        | C::VariantSwitch { subject: exp, .. } => vec![exp],
        C::Mutate(lhs, rhs) => vec![lhs, rhs],
        C::Break(_) | C::Continue(_) | C::Jump { .. } => vec![],
    };
    exps.into_iter()
}

/// Collects cross-module constant references that failed to fold or are outside the current
/// compilation
#[growing_stack]
fn unresolved_cross_module_constants(
    constant_context: &ConstantContext,
    module: ModuleIdent,
    e: &H::Exp,
    unresolved: &mut Vec<(ModuleIdent, ConstantName, Loc)>,
) {
    use H::UnannotatedExp_ as E;
    match &e.exp.value {
        E::Constant(m, c) => {
            if *m != module
                && !matches!(
                    constant_context.defs.get(&(*m, *c)),
                    Some(ConstantEntry::Defined { .. })
                )
            {
                unresolved.push((*m, *c, e.exp.loc));
            }
        }
        E::UnaryExp(_, rhs) => {
            unresolved_cross_module_constants(constant_context, module, rhs, unresolved)
        }
        E::BinopExp(lhs, _, rhs) => {
            unresolved_cross_module_constants(constant_context, module, lhs, unresolved);
            unresolved_cross_module_constants(constant_context, module, rhs, unresolved)
        }
        E::Cast(base, _) => {
            unresolved_cross_module_constants(constant_context, module, base, unresolved)
        }
        E::Vector(_, _, _, args) | E::Multiple(args) => {
            for arg in args {
                unresolved_cross_module_constants(constant_context, module, arg, unresolved);
            }
        }
        // other forms cannot appear in constant bodies
        _ => (),
    }
}

//**************************************************************************************************
// Cross-module Constant Generation
//**************************************************************************************************

pub(super) fn generate_cross_module_constants(
    context: &mut Context,
    constant_context: &mut ConstantContext,
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
                command(
                    context,
                    constant_context,
                    &mut new_constants,
                    constant_values,
                    module,
                    cmd,
                );
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
    constant_context: &mut ConstantContext,
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
        | C::VariantSwitch { subject: e, .. } => exp(
            context,
            constant_context,
            new_constants,
            constant_values,
            module,
            e,
        ),
        C::Mutate(lhs, rhs) => {
            exp(
                context,
                constant_context,
                new_constants,
                constant_values,
                module,
                lhs,
            );
            exp(
                context,
                constant_context,
                new_constants,
                constant_values,
                module,
                rhs,
            )
        }
        C::Break(_) | C::Continue(_) | C::Jump { .. } => (),
    }
}

#[growing_stack]
fn exp(
    context: &mut Context,
    constant_context: &mut ConstantContext,
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
                new_constants.constant_copy(context, constant_context, constant_values, m, c, eloc)
            {
                *e_ = E::Constant(module, copy_name);
            } else {
                // an error was reported at the constant's definition, at this use, or by typing
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
                exp(
                    context,
                    constant_context,
                    new_constants,
                    constant_values,
                    module,
                    arg,
                );
            }
        }
        E::Freeze(base) | E::Dereference(base) | E::UnaryExp(_, base) | E::Cast(base, _) => exp(
            context,
            constant_context,
            new_constants,
            constant_values,
            module,
            base,
        ),
        E::Borrow(_, base, _, _) => exp(
            context,
            constant_context,
            new_constants,
            constant_values,
            module,
            base,
        ),
        E::BinopExp(lhs, _, rhs) => {
            exp(
                context,
                constant_context,
                new_constants,
                constant_values,
                module,
                lhs,
            );
            exp(
                context,
                constant_context,
                new_constants,
                constant_values,
                module,
                rhs,
            )
        }
        E::Pack(_, _, fields) | E::PackVariant(_, _, _, fields) => {
            for (_, _, fe) in fields {
                exp(
                    context,
                    constant_context,
                    new_constants,
                    constant_values,
                    module,
                    fe,
                );
            }
        }
        E::Vector(_, _, _, args) | E::Multiple(args) => {
            for arg in args {
                exp(
                    context,
                    constant_context,
                    new_constants,
                    constant_values,
                    module,
                    arg,
                );
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

//**************************************************************************************************
// Constant dependencies
//**************************************************************************************************

/// Error message for unfoldable constants
const INVALID_CONST_EXP: &str =
    "Invalid expression in 'const'. This expression could not be evaluated to a value";

/// Report cyclic constant definition
fn report_cycle(
    context: &mut Context,
    consts: &BTreeMap<ConstantId, H::Constant>,
    scc: &[ConstantId],
) {
    fn display((m, c): &ConstantId) -> String {
        format!("{}::{}", m, c)
    }
    fn defined_loc(consts: &BTreeMap<ConstantId, H::Constant>, node: &ConstantId) -> Loc {
        consts.get_key_value(node).unwrap().0.1.0.loc
    }

    if let [node] = scc {
        context.add_diag(diag!(
            CodeGeneration::UnfoldableConstant,
            (
                defined_loc(consts, node),
                format!("Constant '{}' references itself", display(node)),
            )
        ));
        return;
    }
    let names = scc.iter().map(display).collect::<Vec<_>>().join(", ");
    let mut diag = diag!(
        CodeGeneration::UnfoldableConstant,
        (
            defined_loc(consts, &scc[0]),
            format!("Constant definitions form a circular dependency: {}", names),
        )
    );
    for node in scc.iter().skip(1) {
        diag.add_secondary_label((defined_loc(consts, node), "Cyclic constant defined here"));
    }
    context.add_diag(diag);
}

/// Reports a constant that could not be folded into a value.
fn report_cannot_fold<'a>(
    context: &mut Context,
    constant_context: &ConstantContext,
    module: ModuleIdent,
    loc: Loc,
    exps: impl Iterator<Item = &'a H::Exp>,
) {
    let mut unresolved = vec![];
    for e in exps {
        unresolved_cross_module_constants(constant_context, module, e, &mut unresolved);
    }
    if unresolved.is_empty() {
        context.add_diag(diag!(
            CodeGeneration::UnfoldableConstant,
            (loc, INVALID_CONST_EXP)
        ));
        return;
    }
    for (m, c, use_loc) in unresolved {
        match constant_context.defs.get_key_value(&(m, c)) {
            Some(((_, defined), ConstantEntry::PrecompiledFailed)) => {
                let defined_loc = defined.0.loc;
                context.add_diag(unfoldable_constant_use_error(&m, &c, use_loc, defined_loc));
            }
            // an error was already reported at the definition in this compilation
            Some((_, ConstantEntry::Failed)) => (),
            // A missing entry here means typing already errored on this use (due to visibility
            // or similar).
            None => (),
            Some((_, ConstantEntry::Defined { .. })) => {
                context.add_diag(ice!((use_loc, "defined constant reported as unresolved")));
            }
        }
    }
}

/// The error reported at each use of a pre-compiled constant whose own compilation could not
/// evaluate it to a value; the use site is the only place this compilation can put the error
fn unfoldable_constant_use_error(
    m: &ModuleIdent,
    c: &ConstantName,
    use_loc: Loc,
    defined_loc: Loc,
) -> Diagnostic {
    let msg = format!(
        "Invalid use of constant '{}::{}'. Its value could not be computed",
        m, c
    );
    let mut diag = diag!(CodeGeneration::UnfoldableConstant, (use_loc, msg));
    diag.add_secondary_label((defined_loc, format!("'{}' is defined here", c)));
    diag
}
