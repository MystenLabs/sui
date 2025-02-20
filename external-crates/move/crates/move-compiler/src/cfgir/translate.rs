// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::{
        self,
        ast::{self as G, BasicBlock, BasicBlocks, BlockInfo},
        cfg::{ImmForwardCFG, MutForwardCFG},
        visitor::{CFGIRVisitor, CFGIRVisitorConstructor, CFGIRVisitorContext},
    },
    diag,
    diagnostics::{warning_filters::WarningFilters, Diagnostic, DiagnosticReporter, Diagnostics},
    expansion::ast::{Attributes, ModuleIdent, Mutability},
    hlir::ast::{self as H, BlockLabel, Label, Value, Value_, Var},
    ice_assert,
    parser::ast::{ConstantName, FunctionName},
    shared::{program_info::TypingProgramInfo, unique_map::UniqueMap, AstDebug, CompilationEnv},
    FullyCompiledProgram,
};
use cfgir::ast::LoopInfo;
use move_core_types::{account_address::AccountAddress as MoveAddress, runtime_value::MoveValue};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use move_symbol_pool::Symbol;
use petgraph::{
    algo::{kosaraju_scc as petgraph_scc, toposort as petgraph_toposort},
    graphmap::DiGraphMap,
};
use rayon::prelude::*;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

//**************************************************************************************************
// Context
//**************************************************************************************************

enum NamedBlockType {
    Loop,
    While,
    Named,
}

pub(super) struct CFGIRDebugFlags {
    #[allow(dead_code)]
    pub(super) print_blocks: bool,
    #[allow(dead_code)]
    pub(super) print_optimized_blocks: bool,
}

struct Context<'env> {
    env: &'env CompilationEnv,
    info: &'env TypingProgramInfo,
    reporter: DiagnosticReporter<'env>,
    current_package: Option<Symbol>,
    label_count: usize,
    named_blocks: UniqueMap<BlockLabel, (Label, Label)>,
    // Used for populating block_info
    loop_bounds: BTreeMap<Label, G::LoopInfo>,
    debug: CFGIRDebugFlags,
}

impl<'env> Context<'env> {
    pub fn new(env: &'env CompilationEnv, info: &'env TypingProgramInfo) -> Self {
        let reporter = env.diagnostic_reporter_at_top_level();
        Context {
            env,
            reporter,
            info,
            current_package: None,
            label_count: 0,
            named_blocks: UniqueMap::new(),
            loop_bounds: BTreeMap::new(),
            debug: CFGIRDebugFlags {
                print_blocks: false,
                print_optimized_blocks: false,
            },
        }
    }

    pub fn add_diag(&self, diag: Diagnostic) {
        self.reporter.add_diag(diag);
    }

    pub fn add_diags(&self, diags: Diagnostics) {
        self.reporter.add_diags(diags);
    }

    pub fn push_warning_filter_scope(&mut self, filters: WarningFilters) {
        self.reporter.push_warning_filter_scope(filters)
    }

    pub fn pop_warning_filter_scope(&mut self) {
        self.reporter.pop_warning_filter_scope()
    }

    fn new_label(&mut self) -> Label {
        let count = self.label_count;
        self.label_count += 1;
        Label(count)
    }

    fn enter_named_block(
        &mut self,
        name: BlockLabel,
        block_type: NamedBlockType,
    ) -> (Label, Label) {
        let start_label = self.new_label();
        let end_label = self.new_label();
        if matches!(block_type, NamedBlockType::Loop | NamedBlockType::While) {
            self.loop_bounds.insert(
                start_label,
                LoopInfo {
                    is_loop_stmt: matches!(block_type, NamedBlockType::Loop),
                    loop_end: G::LoopEnd::Target(end_label),
                },
            );
        }
        self.named_blocks
            .add(name, (start_label, end_label))
            .expect("ICE reused block name");
        (start_label, end_label)
    }

    fn exit_named_block(&mut self, name: &BlockLabel) {
        self.named_blocks.remove(name);
    }

    fn named_block_start_label(&mut self, name: &BlockLabel) -> Label {
        self.named_blocks
            .get(name)
            .expect("ICE named block with no entry")
            .0
    }

    fn named_block_end_label(&mut self, name: &BlockLabel) -> Label {
        self.named_blocks
            .get(name)
            .expect("ICE named block with no entry")
            .1
    }

    fn clear_block_state(&mut self) {
        assert!(self.named_blocks.is_empty());
        self.label_count = 0;
        self.loop_bounds = BTreeMap::new();
    }
}

//**************************************************************************************************
// Entry
//**************************************************************************************************

pub fn program(
    compilation_env: &CompilationEnv,
    _pre_compiled_lib: Option<Arc<FullyCompiledProgram>>,
    prog: H::Program,
) -> G::Program {
    let H::Program {
        modules: hmodules,
        warning_filters_table,
        info,
    } = prog;

    let mut context = Context::new(compilation_env, &info);

    let modules = modules(&mut context, hmodules);
    set_constant_value_types(&info, &modules);

    let mut program = G::Program {
        modules,
        warning_filters_table,
        info: info.clone(),
    };
    visit_program(&mut context, &mut program);
    program
}

fn set_constant_value_types(
    info: &TypingProgramInfo,
    modules: &UniqueMap<ModuleIdent, G::ModuleDefinition>,
) {
    for (mname, mdef) in modules.key_cloned_iter() {
        for (cname, cdef) in mdef.constants.key_cloned_iter() {
            if let Some(value) = &cdef.value {
                info.module(&mname)
                    .constants
                    .get(&cname)
                    .unwrap()
                    .value
                    .set(value.clone())
                    .unwrap();
            }
        }
    }
}

fn modules(
    context: &mut Context,
    hmodules: UniqueMap<ModuleIdent, H::ModuleDefinition>,
) -> UniqueMap<ModuleIdent, G::ModuleDefinition> {
    let modules = hmodules
        .into_iter()
        .map(|(mname, m)| module(context, mname, m));
    UniqueMap::maybe_from_iter(modules).unwrap()
}

fn module(
    context: &mut Context,
    module_ident: ModuleIdent,
    mdef: H::ModuleDefinition,
) -> (ModuleIdent, G::ModuleDefinition) {
    let H::ModuleDefinition {
        warning_filter,
        package_name,
        attributes,
        target_kind,
        dependency_order,
        friends,
        structs,
        enums,
        functions: hfunctions,
        constants: hconstants,
    } = mdef;
    context.current_package = package_name;
    context.push_warning_filter_scope(warning_filter);
    let constants = constants(context, module_ident, hconstants);
    let functions = hfunctions.map(|name, f| function(context, module_ident, name, f));
    context.pop_warning_filter_scope();
    context.current_package = None;
    (
        module_ident,
        G::ModuleDefinition {
            warning_filter,
            package_name,
            attributes,
            target_kind,
            dependency_order,
            friends,
            structs,
            enums,
            constants,
            functions,
        },
    )
}

//**************************************************************************************************
// Functions
//**************************************************************************************************

fn constants(
    context: &mut Context,
    module: ModuleIdent,
    mut consts: UniqueMap<ConstantName, H::Constant>,
) -> UniqueMap<ConstantName, G::Constant> {
    // Traverse the constants and compute the dependency graph between constants: if one mentions
    // another, an edge is added between them.
    let mut graph = DiGraphMap::new();
    for (name, constant) in consts.key_cloned_iter() {
        let deps = dependent_constants(constant);
        if deps.is_empty() {
            graph.add_node(name);
        } else {
            for dep in deps {
                graph.add_edge(dep, name, ());
            }
        }
    }

    // report any cycles we find
    let sccs = petgraph_scc(&graph);
    let mut cycle_nodes = BTreeSet::new();
    for scc in sccs {
        if scc.len() > 1 {
            let names = scc
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let mut diag = diag!(
                CodeGeneration::UnfoldableConstant,
                (
                    *consts.get_loc(&scc[0]).unwrap(),
                    format!("Constant definitions form a circular dependency: {}", names),
                )
            );
            for name in scc.iter().skip(1) {
                diag.add_secondary_label((
                    *consts.get_loc(name).unwrap(),
                    "Cyclic constant defined here",
                ));
            }
            context.add_diag(diag);
            cycle_nodes.append(&mut scc.into_iter().collect());
        }
    }
    // report any node that relies on a node in a cycle but is not iself part of that cycle
    for cycle_node in cycle_nodes.iter() {
        // petgraph retains edges for nodes that have been deleted, so we ensure the node is not
        // part of a cyclle _and_ it's still in the graph
        let neighbors: Vec<_> = graph
            .neighbors(*cycle_node)
            .filter(|node| !cycle_nodes.contains(node) && graph.contains_node(*node))
            .collect();
        for node in neighbors {
            context.add_diag(diag!(
                CodeGeneration::UnfoldableConstant,
                (
                    *consts.get_loc(&node).unwrap(),
                    format!(
                        "Constant uses constant {}, which has a circular dependency",
                        cycle_node
                    )
                )
            ));
            graph.remove_node(node);
        }
        graph.remove_node(*cycle_node);
    }

    // Finally, iterate the remaining constants in dependency order, inlining them into each other
    // via the constant folding optimizer as we process them.

    // petgraph will include nodes in the toposort that only appear in an edge, even if that node
    // has been removed from the graph, so we filter down to only the remaining nodes
    let remaining_nodes: BTreeSet<_> = graph.nodes().collect();
    let sorted: Vec<_> = petgraph_toposort(&graph, None)
        .expect("ICE concstant cycles not removed")
        .into_iter()
        .filter(|node| remaining_nodes.contains(node))
        .collect();

    let mut out_map = UniqueMap::new();
    let mut constant_values = UniqueMap::new();
    for constant_name in sorted.into_iter() {
        let cdef = consts.remove(&constant_name).unwrap();
        let new_cdef = constant(context, &mut constant_values, module, constant_name, cdef);
        out_map
            .add(constant_name, new_cdef)
            .expect("ICE constant name collision");
    }

    out_map
}

fn dependent_constants(constant: &H::Constant) -> BTreeSet<ConstantName> {
    fn dep_exp(set: &mut BTreeSet<ConstantName>, exp: &H::Exp) {
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
            E::Constant(c) => {
                set.insert(*c);
            }
            _ => panic!("ICE typing should have rejected exp in const"),
        }
    }

    fn dep_cmd(set: &mut BTreeSet<ConstantName>, command: &H::Command_) {
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

    fn dep_stmt(set: &mut BTreeSet<ConstantName>, stmt: &H::Statement_) {
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

    fn dep_block(set: &mut BTreeSet<ConstantName>, block: &H::Block) {
        for entry in block {
            dep_stmt(set, &entry.value);
        }
    }

    let mut output = BTreeSet::new();
    let (_, block) = &constant.value;
    dep_block(&mut output, block);
    output
}

fn constant(
    context: &mut Context,
    constant_values: &mut UniqueMap<ConstantName, Value>,
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

    context.push_warning_filter_scope(warning_filter);
    let final_value = constant_(
        context,
        constant_values,
        module,
        name,
        loc,
        &attributes,
        signature.clone(),
        locals,
        block,
    );
    let value = match final_value {
        Some(H::Exp {
            exp: sp!(_, H::UnannotatedExp_::Value(value)),
            ..
        }) => {
            constant_values
                .add(name, value.clone())
                .expect("ICE constant name collision");
            Some(move_value_from_value(value))
        }
        _ => None,
    };

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

const CANNOT_FOLD: &str =
    "Invalid expression in 'const'. This expression could not be evaluated to a value";

fn constant_(
    context: &mut Context,
    constant_values: &UniqueMap<ConstantName, Value>,
    module: ModuleIdent,
    name: ConstantName,
    full_loc: Loc,
    attributes: &Attributes,
    signature: H::BaseType,
    locals: UniqueMap<Var, (Mutability, H::SingleType)>,
    body: H::Block,
) -> Option<H::Exp> {
    use H::Command_ as C;
    const ICE_MSG: &str = "ICE invalid constant should have been blocked in typing";
    let blocks = block(context, body);
    let (start, mut blocks, block_info) = finalize_blocks(context, blocks);
    context.clear_block_state();

    let binfo = block_info.iter().map(destructure_tuple);
    let (mut cfg, infinite_loop_starts, errors) = MutForwardCFG::new(start, &mut blocks, binfo);
    assert!(infinite_loop_starts.is_empty(), "{}", ICE_MSG);
    assert!(errors.is_empty(), "{}", ICE_MSG);

    let num_previous_errors = context.env.count_diags();
    let fake_signature = H::FunctionSignature {
        type_parameters: vec![],
        parameters: vec![],
        return_type: H::Type_::base(signature),
    };
    let fake_infinite_loop_starts = BTreeSet::new();
    let function_context = super::CFGContext {
        env: context.env,
        reporter: &context.reporter,
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
    cfgir::refine_inference_and_verify(&function_context, &mut cfg);
    ice_assert!(
        context.reporter,
        num_previous_errors == context.env.count_diags(),
        full_loc,
        "{}",
        ICE_MSG
    );
    cfgir::optimize(
        context.env,
        &context.reporter,
        context.current_package,
        &fake_signature,
        &locals,
        constant_values,
        &mut cfg,
    );

    if blocks.len() != 1 {
        context.add_diag(diag!(
            CodeGeneration::UnfoldableConstant,
            (full_loc, CANNOT_FOLD)
        ));
        return None;
    }
    let mut optimized_block = blocks.remove(&start).unwrap();
    let return_cmd = optimized_block.pop_back().unwrap();
    for sp!(cloc, cmd_) in &optimized_block {
        let e = match cmd_ {
            C::IgnoreAndPop { exp, .. } => exp,
            _ => {
                context.add_diag(diag!(
                    CodeGeneration::UnfoldableConstant,
                    (*cloc, CANNOT_FOLD)
                ));
                continue;
            }
        };
        check_constant_value(context, e)
    }

    let result = match return_cmd.value {
        C::Return { exp: e, .. } => e,
        _ => unreachable!(),
    };
    check_constant_value(context, &result);
    Some(result)
}

fn check_constant_value(context: &mut Context, e: &H::Exp) {
    use H::UnannotatedExp_ as E;
    match &e.exp.value {
        E::Value(_) => (),
        _ => context.add_diag(diag!(
            CodeGeneration::UnfoldableConstant,
            (e.exp.loc, CANNOT_FOLD)
        )),
    }
}

pub(crate) fn move_value_from_value(sp!(_, v_): Value) -> MoveValue {
    move_value_from_value_(v_)
}

pub(crate) fn move_value_from_value_(v_: Value_) -> MoveValue {
    use MoveValue as MV;
    use Value_ as V;
    match v_ {
        V::Address(a) => MV::Address(MoveAddress::new(a.into_bytes())),
        V::U8(u) => MV::U8(u),
        V::U16(u) => MV::U16(u),
        V::U32(u) => MV::U32(u),
        V::U64(u) => MV::U64(u),
        V::U128(u) => MV::U128(u),
        V::U256(u) => MV::U256(u),
        V::Bool(b) => MV::Bool(b),
        V::Vector(_, vs) => MV::Vector(vs.into_iter().map(move_value_from_value).collect()),
    }
}

//**************************************************************************************************
// Functions
//**************************************************************************************************

fn function(
    context: &mut Context,
    module: ModuleIdent,
    name: FunctionName,
    f: H::Function,
) -> G::Function {
    let H::Function {
        warning_filter,
        index,
        attributes,
        loc,
        visibility,
        compiled_visibility,
        entry,
        signature,
        body,
    } = f;
    context.push_warning_filter_scope(warning_filter);
    let body = function_body(
        context,
        module,
        name,
        &attributes,
        entry,
        visibility,
        &signature,
        body,
    );
    context.pop_warning_filter_scope();
    G::Function {
        warning_filter,
        index,
        attributes,
        loc,
        visibility,
        compiled_visibility,
        entry,
        signature,
        body,
    }
}

fn function_body(
    context: &mut Context,
    module: ModuleIdent,
    name: FunctionName,
    attributes: &Attributes,
    entry: Option<Loc>,
    visibility: H::Visibility,
    signature: &H::FunctionSignature,
    sp!(loc, tb_): H::FunctionBody,
) -> G::FunctionBody {
    use G::FunctionBody_ as GB;
    use H::FunctionBody_ as HB;
    assert!(context.loop_bounds.is_empty());
    assert!(context.named_blocks.is_empty());
    let b_ = match tb_ {
        HB::Native => GB::Native,
        HB::Defined { locals, body } => {
            let blocks = block(context, body);
            let (start, mut blocks, block_info) = finalize_blocks(context, blocks);
            context.clear_block_state();
            let binfo = block_info.iter().map(destructure_tuple);
            if context.debug.print_blocks {
                for (lbl, block) in &blocks {
                    println!("{lbl}:");
                    for cmd in block {
                        print!("    ");
                        cmd.print_verbose();
                    }
                }
            }
            let (mut cfg, infinite_loop_starts, diags) =
                MutForwardCFG::new(start, &mut blocks, binfo);
            context.add_diags(diags);

            let function_context = super::CFGContext {
                env: context.env,
                reporter: &context.reporter,
                info: context.info,
                package: context.current_package,
                module,
                member: cfgir::MemberName::Function(name.0),
                attributes,
                entry,
                visibility,
                signature,
                locals: &locals,
                infinite_loop_starts: &infinite_loop_starts,
            };
            cfgir::refine_inference_and_verify(&function_context, &mut cfg);
            // do not optimize if there are errors, warnings are okay
            if !context.env.has_errors() {
                cfgir::optimize(
                    context.env,
                    &context.reporter,
                    context.current_package,
                    signature,
                    &locals,
                    &UniqueMap::new(),
                    &mut cfg,
                );
                if context.debug.print_optimized_blocks {
                    for (lbl, block) in &blocks {
                        println!("{lbl}:");
                        for cmd in block {
                            print!("    ");
                            cmd.print_verbose();
                        }
                    }
                }
            }
            let block_info = block_info
                .into_iter()
                .filter(|(lbl, _info)| blocks.contains_key(lbl))
                .collect();
            GB::Defined {
                locals,
                start,
                block_info,
                blocks,
            }
        }
    };
    sp(loc, b_)
}

//**************************************************************************************************
// Statements
//**************************************************************************************************

type BlockList = Vec<(Label, BasicBlock)>;

#[growing_stack]
fn block(context: &mut Context, stmts: H::Block) -> BlockList {
    let (start_block, blocks) = block_(context, stmts);
    [(context.new_label(), start_block)]
        .into_iter()
        .chain(blocks)
        .collect()
}

#[growing_stack]
fn block_(context: &mut Context, stmts: H::Block) -> (BasicBlock, BlockList) {
    let mut current_block: BasicBlock = VecDeque::new();
    let mut blocks = Vec::new();

    for stmt in stmts.into_iter().rev() {
        let (new_current, new_blocks) = statement(context, stmt, current_block);
        blocks = new_blocks.into_iter().chain(blocks.into_iter()).collect();
        current_block = new_current;
    }
    (current_block, blocks)
}

fn finalize_blocks(
    context: &mut Context,
    blocks: BlockList,
) -> (Label, BasicBlocks, Vec<(Label, BlockInfo)>) {
    // Given the in-order vector of blocks we'd like to emit, we do three things:
    // 1. Generate an in-order map from that list.
    // 2. Generate block info for the blocks in order.
    // 3. Discard the in-order vector in favor of a (remapped) BTreeMap for CFG.

    let start_label = blocks[0].0;

    let mut label_map: BTreeMap<Label, Label> = BTreeMap::new();
    let mut label_counter = 0;
    let mut next_label = || {
        let label = Label(label_counter);
        label_counter += 1;
        label
    };

    for (lbl, _) in &blocks {
        label_map.insert(*lbl, next_label());
    }

    let mut block_info: Vec<(Label, BlockInfo)> = vec![];
    for (lbl, _) in &blocks {
        let info = match context.loop_bounds.get(lbl) {
            None => BlockInfo::Other,
            Some(LoopInfo {
                is_loop_stmt,
                loop_end,
            }) => {
                let loop_end = match loop_end {
                    G::LoopEnd::Target(end) if label_map.contains_key(end) => {
                        G::LoopEnd::Target(label_map[end])
                    }
                    G::LoopEnd::Target(_) => G::LoopEnd::Unused,
                    G::LoopEnd::Unused => G::LoopEnd::Unused,
                };
                BlockInfo::LoopHead(LoopInfo {
                    is_loop_stmt: *is_loop_stmt,
                    loop_end,
                })
            }
        };
        block_info.push((label_map[lbl], info));
    }

    let block_map: BasicBlocks = BTreeMap::from_iter(blocks);
    let (out_label, out_blocks) = G::remap_labels(&label_map, start_label, block_map);
    (out_label, out_blocks, block_info)
}

#[growing_stack]
fn statement(
    context: &mut Context,
    sp!(sloc, stmt): H::Statement,
    mut current_block: BasicBlock,
) -> (BasicBlock, BlockList) {
    use H::{Command_ as C, Statement_ as S};
    match stmt {
        S::IfElse {
            cond: test,
            if_block,
            else_block,
        } => {
            let true_label = context.new_label();
            let false_label = context.new_label();
            let phi_label = context.new_label();

            let test_block = VecDeque::from([sp(
                sloc,
                C::JumpIf {
                    cond: *test,
                    if_true: true_label,
                    if_false: false_label,
                },
            )]);

            let (true_entry_block, true_blocks) = block_(
                context,
                with_last(if_block, make_jump(sloc, phi_label, false)),
            );
            let (false_entry_block, false_blocks) = block_(
                context,
                with_last(else_block, make_jump(sloc, phi_label, false)),
            );

            let new_blocks = [(true_label, true_entry_block)]
                .into_iter()
                .chain(true_blocks)
                .chain([(false_label, false_entry_block)])
                .chain(false_blocks)
                .chain([(phi_label, current_block)])
                .collect::<BlockList>();

            (test_block, new_blocks)
        }

        S::VariantMatch {
            subject,
            enum_name,
            arms,
        } => {
            let subject = *subject;

            let phi_label = context.new_label();

            let mut arm_blocks = BlockList::new();

            let arms = arms
                .into_iter()
                .map(|(variant_name, arm_block)| {
                    let arm_label = context.new_label();
                    let (arm_entry_block, arm_entry_blocks) = block_(
                        context,
                        with_last(arm_block, make_jump(sloc, phi_label, false)),
                    );
                    let mut blocks = [(arm_label, arm_entry_block)]
                        .into_iter()
                        .chain(arm_entry_blocks)
                        .collect::<BlockList>();
                    arm_blocks.append(&mut blocks);
                    (variant_name, arm_label)
                })
                .collect::<Vec<_>>();

            arm_blocks.push((phi_label, current_block));

            let test_block = VecDeque::from([sp(
                sloc,
                C::VariantSwitch {
                    subject,
                    enum_name,
                    arms,
                },
            )]);

            (test_block, arm_blocks)
        }

        // We could turn these into loops earlier and elide this case.
        S::While {
            name,
            cond: (test_block, test),
            block: body,
        } => {
            let (start_label, end_label) = context.enter_named_block(name, NamedBlockType::While);
            let body_label = context.new_label();

            let entry_block = VecDeque::from([make_jump(sloc, start_label, false)]);

            let (initial_test_block, test_blocks) = {
                let test_jump = sp(
                    sloc,
                    C::JumpIf {
                        cond: *test,
                        if_true: body_label,
                        if_false: end_label,
                    },
                );
                block_(context, with_last(test_block, test_jump))
            };

            let (body_entry_block, body_blocks) = block_(
                context,
                with_last(body, make_jump(sloc, start_label, false)),
            );

            context.exit_named_block(&name);

            let new_blocks = [(start_label, initial_test_block)]
                .into_iter()
                .chain(test_blocks)
                .chain([(body_label, body_entry_block)])
                .chain(body_blocks)
                .chain([(end_label, current_block)])
                .collect::<BlockList>();

            (entry_block, new_blocks)
        }
        S::Loop {
            name,
            block: body,
            has_break: _,
        } => {
            let (start_label, end_label) = context.enter_named_block(name, NamedBlockType::Loop);

            let entry_block = VecDeque::from([make_jump(sloc, start_label, false)]);

            let (body_entry_block, body_blocks) = block_(
                context,
                with_last(body, make_jump(sloc, start_label, false)),
            );

            context.exit_named_block(&name);

            let new_blocks = [(start_label, body_entry_block)]
                .into_iter()
                .chain(body_blocks)
                .chain([(end_label, current_block)])
                .collect::<BlockList>();

            (entry_block, new_blocks)
        }
        S::NamedBlock { name, block: body } => {
            let (start_label, end_label) = context.enter_named_block(name, NamedBlockType::Named);

            let entry_block = VecDeque::from([make_jump(sloc, start_label, false)]);

            let (body_entry_block, body_blocks) =
                block_(context, with_last(body, make_jump(sloc, end_label, false)));

            context.exit_named_block(&name);

            let new_blocks = [(start_label, body_entry_block)]
                .into_iter()
                .chain(body_blocks)
                .chain([(end_label, current_block)])
                .collect::<BlockList>();

            (entry_block, new_blocks)
        }
        S::Command(sp!(cloc, C::Break(name))) => {
            // Discard the current block because it's dead code.
            let break_jump = make_jump(cloc, context.named_block_end_label(&name), true);
            (VecDeque::from([break_jump]), vec![])
        }
        S::Command(sp!(cloc, C::Continue(name))) => {
            // Discard the current block because it's dead code.
            let jump = make_jump(cloc, context.named_block_start_label(&name), true);
            (VecDeque::from([jump]), vec![])
        }
        S::Command(cmd) if cmd.value.is_terminal() => {
            // Discard the current block because it's dead code.
            (VecDeque::from([cmd]), vec![])
        }
        S::Command(cmd) => {
            current_block.push_front(cmd);
            (current_block, vec![])
        }
    }
}

fn with_last(mut block: H::Block, sp!(loc, cmd): H::Command) -> H::Block {
    match block.iter().last() {
        Some(sp!(_, H::Statement_::Command(cmd))) if cmd.value.is_hlir_terminal() => block,
        _ => {
            let stmt = sp(loc, H::Statement_::Command(sp(loc, cmd)));
            block.push_back(stmt);
            block
        }
    }
}

fn make_jump(loc: Loc, target: Label, from_user: bool) -> H::Command {
    sp(loc, H::Command_::Jump { target, from_user })
}

// Added to dodge a clippy complaint
fn destructure_tuple<T, U>((fst, snd): &(T, U)) -> (&T, &U) {
    (fst, snd)
}

//**************************************************************************************************
// Visitors
//**************************************************************************************************

fn visit_program(context: &mut Context, prog: &mut G::Program) {
    if context.env.visitors().abs_int.is_empty() && context.env.visitors().cfgir.is_empty() {
        return;
    }

    AbsintVisitor.visit(context.env, prog);

    context
        .env
        .visitors()
        .cfgir
        .par_iter()
        .for_each(|v| v.visit(context.env, prog));
}

struct AbsintVisitor;
struct AbsintVisitorContext<'a> {
    env: &'a CompilationEnv,
    reporter: DiagnosticReporter<'a>,
    info: Arc<TypingProgramInfo>,
    current_package: Option<Symbol>,
}

impl CFGIRVisitorConstructor for AbsintVisitor {
    type Context<'a> = AbsintVisitorContext<'a>;

    fn context<'a>(env: &'a CompilationEnv, program: &G::Program) -> Self::Context<'a> {
        let reporter = env.diagnostic_reporter_at_top_level();
        AbsintVisitorContext {
            env,
            reporter,
            info: program.info.clone(),
            current_package: None,
        }
    }
}

impl AbsintVisitorContext<'_> {
    #[allow(unused)]
    fn add_diag(&self, diag: crate::diagnostics::Diagnostic) {
        self.reporter.add_diag(diag);
    }

    fn add_diags(&self, diags: crate::diagnostics::Diagnostics) {
        self.reporter.add_diags(diags);
    }
}

impl<'a> CFGIRVisitorContext for AbsintVisitorContext<'a> {
    fn push_warning_filter_scope(&mut self, filters: WarningFilters) {
        self.reporter.push_warning_filter_scope(filters)
    }

    fn pop_warning_filter_scope(&mut self) {
        self.reporter.pop_warning_filter_scope()
    }

    fn visit_module_custom(&mut self, _ident: ModuleIdent, mdef: &G::ModuleDefinition) -> bool {
        self.current_package = mdef.package_name;
        false
    }

    fn visit_function_custom(
        &mut self,
        mident: ModuleIdent,
        name: FunctionName,
        fdef: &G::Function,
    ) -> bool {
        let G::Function {
            warning_filter: _,
            index: _,
            attributes,
            loc: _,
            compiled_visibility: _,
            visibility,
            entry,
            signature,
            body,
        } = fdef;
        let G::FunctionBody_::Defined {
            locals,
            start,
            blocks,
            block_info,
        } = &body.value
        else {
            return true;
        };
        let (cfg, infinite_loop_starts) = ImmForwardCFG::new(*start, blocks, block_info.iter());
        let function_context = super::CFGContext {
            env: self.env,
            reporter: &self.reporter,
            info: &self.info,
            package: self.current_package,
            module: mident,
            member: cfgir::MemberName::Function(name.0),
            attributes,
            entry: *entry,
            visibility: *visibility,
            signature,
            locals,
            infinite_loop_starts: &infinite_loop_starts,
        };
        self.env
            .visitors()
            .abs_int
            .par_iter()
            .for_each(|v| self.add_diags(v.verify(&function_context, &cfg)));
        true
    }
}
