// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Folds every constant definition in the compilation to a value. Constants fold in the order of
//! their own dependency graph, so a constant defined in terms of another module's constant sees
//! that constant's value; definition cycles are reported here.

// TODO(cross-module-constants): `compute_dependent_constants` is a hand-rolled HLIR walker; there
// is no HLIR visitor today. Consider a shared walker if another one appears.

use crate::{
    cfgir::{
        self, ast as G,
        cfg::MutForwardCFG,
        constants::{
            ConstantEntry, ConstantId, ConstantValues, Constants, unfoldable_constant_use_error,
        },
        translate::{Context as CfgirContext, lower_body_to_cfg, move_value_from_value},
    },
    diag,
    diagnostics::{Diagnostic, DiagnosticReporter, Diagnostics, filter::FilterScope},
    expansion::ast::{Attributes, ModuleIdent, Mutability},
    hlir::{ast as H, translate::precompiled_constants},
    ice, ice_assert,
    parser::ast::ConstantName,
    shared::unique_map::UniqueMap,
};

use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use move_symbol_pool::Symbol;
use petgraph::{algo::kosaraju_scc as petgraph_scc, graphmap::DiGraphMap};

use std::collections::{BTreeMap, BTreeSet};

//**************************************************************************************************
// Context
//**************************************************************************************************

struct Context<'ctx, 'env> {
    inner_context: &'ctx mut CfgirContext<'env>,
    constants: Constants,
}

impl<'ctx, 'env> Context<'ctx, 'env> {
    fn new(context: &'ctx mut CfgirContext<'env>) -> Self {
        Self {
            inner_context: context,
            constants: Constants::default(),
        }
    }

    /// The folded constants, once every definition has been folded.
    fn into_constants(self) -> Constants {
        self.constants
    }

    fn reporter(&self) -> &DiagnosticReporter<'env> {
        self.inner_context.reporter()
    }

    fn add_diag(&self, diag: Diagnostic) {
        self.inner_context.add_diag(diag);
    }

    fn set_package(&mut self, package: Option<Symbol>) {
        self.inner_context.current_package = package;
    }

    fn push_warning_filter_scope(&mut self, filters: FilterScope) {
        self.inner_context.push_warning_filter_scope(filters);
    }

    fn pop_warning_filter_scope(&mut self) {
        self.inner_context.pop_warning_filter_scope();
    }
}

//**************************************************************************************************
// Entry Point
//**************************************************************************************************

/// Compiles the constants of every module in the compilation, taking them out of `hmodules`.
/// Returns them grouped by defining module, along with the folded values that later passes use
/// to discharge cross-module uses.
pub(crate) fn modules(
    context: &mut CfgirContext,
    hmodules: &mut [(ModuleIdent, H::ModuleDefinition)],
) -> (
    BTreeMap<ModuleIdent, UniqueMap<ConstantName, G::Constant>>,
    Constants,
) {
    let mut ctxt = Context::new(context);
    seed_precompiled_constants(&mut ctxt, hmodules);
    let folded = compute_folded_constants(&mut ctxt, hmodules);
    (folded, ctxt.into_constants())
}

//************************************************
// Pre-compiled constants
//************************************************

/// Constants of modules outside this compilation (e.g. the analyzer's pre-compiled dependencies)
/// were folded when they were compiled; seed their values.
fn seed_precompiled_constants(ctxt: &mut Context, hmodules: &[(ModuleIdent, H::ModuleDefinition)]) {
    let compiled: BTreeSet<ModuleIdent> = hmodules.iter().map(|(mname, _)| *mname).collect();
    // constants are only visible within their package, so only modules from the packages being
    // compiled can contribute
    let packages: BTreeSet<Option<Symbol>> =
        hmodules.iter().map(|(_, mdef)| mdef.package_name).collect();
    let precompiled = precompiled_constants(
        ctxt.reporter(),
        ctxt.inner_context.info,
        &compiled,
        &packages,
    );

    for ((mident, cname), (defined_loc, signature, value)) in precompiled {
        let entry = match value {
            Some(value) => {
                ctxt.constants.values.insert((mident, cname), value);
                ConstantEntry::Defined {
                    loc: defined_loc,
                    signature: Box::new(signature),
                }
            }
            None => ConstantEntry::PrecompiledFailed,
        };
        ctxt.constants.defs.insert((mident, cname), entry);
    }
}

//************************************************
// Constant folding
//************************************************

/// Folds every constant in the program up front, in the order of the constants' own global
/// dependency graph. Constant definition cycles are also reported here.
/// Returns the constants grouped by defining module.
fn compute_folded_constants(
    ctxt: &mut Context,
    hmodules: &mut [(ModuleIdent, H::ModuleDefinition)],
) -> BTreeMap<ModuleIdent, UniqueMap<ConstantName, G::Constant>> {
    let (mut unfolded, module_scopes) = take_module_constants(hmodules);
    let dependencies = constant_dependencies(&unfolded);

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
            report_cycle(ctxt, &unfolded, &scc);
            for node in scc {
                let (key, _) = unfolded
                    .remove_entry(&node)
                    .expect("ICE cycle constant not in the compilation");
                ctxt.constants.defs.insert(key, ConstantEntry::Failed);
            }
            continue;
        }

        let node = scc[0];
        // Referring to a failed constant propagates the failure.
        // We do not issue a second error, however.
        // TODO: We _could_ keep that original diagnostic and add this location as a note?
        let dep_failed = dependencies[&node]
            .iter()
            .any(|dep| matches!(ctxt.constants.defs.get(dep), Some(ConstantEntry::Failed)));
        if dep_failed {
            let (key, _) = unfolded.remove_entry(&node).unwrap();
            ctxt.constants.defs.insert(key, ConstantEntry::Failed);
            continue;
        }

        let (mname, cname) = node;
        let cdef = unfolded.remove(&node).unwrap();
        // constants fold in their defining module's diagnostic scope
        let (package, filter) = module_scopes
            .get(&mname)
            .expect("ICE constant from module outside the compilation");
        ctxt.set_package(*package);

        ctxt.push_warning_filter_scope(filter.clone());
        let new_cdef = fold_constant(ctxt, mname, cname, cdef);
        ctxt.pop_warning_filter_scope();

        ctxt.set_package(None);
        folded
            .entry(mname)
            .or_default()
            .add(cname, new_cdef)
            .expect("ICE constant name collision");
    }

    if !unfolded.is_empty() {
        for c in unfolded.values() {
            ctxt.add_diag(ice!((c.loc, "did not fold constant")));
        }
    }

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
    let mut unfolded = BTreeMap::new();
    let mut module_scopes = BTreeMap::new();

    for (mname, mdef) in hmodules.iter_mut() {
        module_scopes.insert(*mname, (mdef.package_name, mdef.warning_filter.clone()));
        let mconstants = std::mem::replace(&mut mdef.constants, UniqueMap::new());
        for (cname, cdef) in mconstants {
            unfolded.insert((*mname, cname), cdef);
        }
    }

    (unfolded, module_scopes)
}

/// The constants each constant references, limited to constants of this compilation.
/// Precompiled constants are already folded values, so they never contribute to the fold order
fn constant_dependencies(
    unfolded: &BTreeMap<ConstantId, H::Constant>,
) -> BTreeMap<ConstantId, BTreeSet<ConstantId>> {
    unfolded
        .iter()
        .map(|(node, cdef)| {
            let deps = compute_dependent_constants(cdef)
                .into_iter()
                .filter(|dep| unfolded.contains_key(dep))
                .collect();
            (*node, deps)
        })
        .collect()
}

fn fold_constant(
    ctxt: &mut Context,
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

    ctxt.push_warning_filter_scope(warning_filter.clone());
    let final_value = fold_constant_impl(
        ctxt,
        module,
        name,
        loc,
        &attributes,
        signature.clone(),
        locals,
        block,
    );
    let (entry, value) = match final_value {
        Some(H::Exp {
            exp: sp!(_, H::UnannotatedExp_::Value(value)),
            ..
        }) => {
            let prev = ctxt.constants.values.insert((module, name), value.clone());
            ice_assert!(
                ctxt.reporter(),
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

    let prev_entry = ctxt.constants.defs.insert((module, name), entry);
    ice_assert!(
        ctxt.reporter(),
        prev_entry.is_none(),
        loc,
        "constant name collision"
    );

    ctxt.pop_warning_filter_scope();

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
    ctxt: &mut Context,
    module: ModuleIdent,
    name: ConstantName,
    full_loc: Loc,
    attributes: &Attributes,
    signature: H::BaseType,
    locals: UniqueMap<H::Var, (Mutability, H::SingleType)>,
    body: H::Block,
) -> Option<H::Exp> {
    use H::Command_ as C;

    let values = &ctxt.constants.values;
    let (start, mut blocks, _block_info) = lower_body_to_cfg(
        ctxt.inner_context,
        body,
        |context, cfg, infinite_loop_starts, errors| {
            verify_and_optimize(
                context,
                values,
                module,
                name,
                full_loc,
                attributes,
                signature,
                &locals,
                cfg,
                infinite_loop_starts,
                errors,
            )
        },
    );

    if blocks.len() != 1 {
        let exps = blocks
            .values()
            .flat_map(|block| block.iter())
            .flat_map(|cmd| command_exps(&cmd.value));
        report_cannot_fold(ctxt, module, full_loc, exps);
        return None;
    }

    let mut optimized_block = blocks.remove(&start).unwrap();
    let return_cmd = optimized_block.pop_back().unwrap();
    for sp!(cloc, cmd_) in &optimized_block {
        let e = match cmd_ {
            C::IgnoreAndPop { exp, .. } => exp,
            _ => {
                report_cannot_fold(ctxt, module, *cloc, command_exps(cmd_));
                continue;
            }
        };
        check_constant_value(ctxt, module, e)
    }

    let result = match return_cmd.value {
        C::Return { exp: e, .. } => e,
        _ => unreachable!(),
    };
    check_constant_value(ctxt, module, &result);

    Some(result)
}

/// Verifies a constant body's lowered CFG and optimizes it, folding it towards a value. The
/// constant is checked as a parameterless function returning its own type.
#[allow(clippy::too_many_arguments)]
fn verify_and_optimize(
    context: &mut CfgirContext,
    constant_values: &ConstantValues,
    module: ModuleIdent,
    name: ConstantName,
    full_loc: Loc,
    attributes: &Attributes,
    signature: H::BaseType,
    locals: &UniqueMap<H::Var, (Mutability, H::SingleType)>,
    cfg: &mut MutForwardCFG,
    infinite_loop_starts: &BTreeSet<H::Label>,
    errors: Diagnostics,
) {
    const ICE_MSG: &str = "ICE invalid constant should have been blocked in typing";

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
        locals,
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
        locals,
        constant_values,
        cfg,
    );
}

fn check_constant_value(ctxt: &mut Context, module: ModuleIdent, e: &H::Exp) {
    use H::UnannotatedExp_ as E;
    if !matches!(e.exp.value, E::Value(_)) {
        report_cannot_fold(ctxt, module, e.exp.loc, std::iter::once(e));
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
    constants: &Constants,
    module: ModuleIdent,
    e: &H::Exp,
    unresolved: &mut Vec<(ModuleIdent, ConstantName, Loc)>,
) {
    use H::UnannotatedExp_ as E;

    match &e.exp.value {
        E::Constant(m, c) => {
            if *m != module
                && !matches!(
                    constants.defs.get(&(*m, *c)),
                    Some(ConstantEntry::Defined { .. })
                )
            {
                unresolved.push((*m, *c, e.exp.loc));
            }
        }
        E::UnaryExp(_, rhs) => {
            unresolved_cross_module_constants(constants, module, rhs, unresolved)
        }
        E::BinopExp(lhs, _, rhs) => {
            unresolved_cross_module_constants(constants, module, lhs, unresolved);
            unresolved_cross_module_constants(constants, module, rhs, unresolved)
        }
        E::Cast(base, _) => unresolved_cross_module_constants(constants, module, base, unresolved),
        E::Vector(_, _, _, args) | E::Multiple(args) => {
            for arg in args {
                unresolved_cross_module_constants(constants, module, arg, unresolved);
            }
        }
        // other forms cannot appear in constant bodies
        _ => (),
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
// Errors
//**************************************************************************************************

/// Error message for unfoldable constants
const INVALID_CONST_EXP: &str =
    "Invalid expression in 'const'. This expression could not be evaluated to a value";

/// Report cyclic constant definition
fn report_cycle(
    ctxt: &mut Context,
    unfolded: &BTreeMap<ConstantId, H::Constant>,
    scc: &[ConstantId],
) {
    fn display((m, c): &ConstantId) -> String {
        format!("{}::{}", m, c)
    }
    fn defined_loc(unfolded: &BTreeMap<ConstantId, H::Constant>, node: &ConstantId) -> Loc {
        unfolded.get_key_value(node).unwrap().0.1.0.loc
    }

    if let [node] = scc {
        ctxt.add_diag(diag!(
            CodeGeneration::UnfoldableConstant,
            (
                defined_loc(unfolded, node),
                format!("Constant '{}' references itself", display(node)),
            )
        ));
        return;
    }

    let names = scc.iter().map(display).collect::<Vec<_>>().join(", ");
    let mut diag = diag!(
        CodeGeneration::UnfoldableConstant,
        (
            defined_loc(unfolded, &scc[0]),
            format!("Constant definitions form a circular dependency: {}", names),
        )
    );

    for node in scc.iter().skip(1) {
        diag.add_secondary_label((defined_loc(unfolded, node), "Cyclic constant defined here"));
    }

    ctxt.add_diag(diag);
}

/// Reports a constant that could not be folded into a value.
fn report_cannot_fold<'a>(
    ctxt: &mut Context,
    module: ModuleIdent,
    loc: Loc,
    exps: impl Iterator<Item = &'a H::Exp>,
) {
    let mut unresolved = vec![];
    for e in exps {
        unresolved_cross_module_constants(&ctxt.constants, module, e, &mut unresolved);
    }

    if unresolved.is_empty() {
        ctxt.add_diag(diag!(
            CodeGeneration::UnfoldableConstant,
            (loc, INVALID_CONST_EXP)
        ));
        return;
    }

    for (m, c, use_loc) in unresolved {
        match ctxt.constants.defs.get_key_value(&(m, c)) {
            Some(((_, defined), ConstantEntry::PrecompiledFailed)) => {
                let defined_loc = defined.0.loc;
                ctxt.add_diag(unfoldable_constant_use_error(&m, &c, use_loc, defined_loc));
            }
            // an error was already reported at the definition in this compilation
            Some((_, ConstantEntry::Failed)) => (),
            // A missing entry here means typing already errored on this use (due to visibility
            // or similar).
            None => (),
            Some((_, ConstantEntry::Defined { .. })) => {
                ctxt.add_diag(ice!((use_loc, "defined constant reported as unresolved")));
            }
        }
    }
}
