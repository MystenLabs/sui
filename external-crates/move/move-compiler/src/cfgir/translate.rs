// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::{
        self,
        ast::{self as G, BasicBlock, BasicBlocks, BlockInfo},
        cfg::{ImmForwardCFG, MutForwardCFG},
    },
    diag,
    diagnostics::Diagnostics,
    expansion::ast::{AbilitySet, ModuleIdent},
    hlir::ast::{self as H, Label, Value, Value_, Var},
    parser::ast::{ConstantName, FunctionName, StructName},
    shared::{unique_map::UniqueMap, CompilationEnv},
    FullyCompiledProgram,
};
use cfgir::ast::LoopInfo;
use move_core_types::{account_address::AccountAddress as MoveAddress, value::MoveValue};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use petgraph::{
    algo::{kosaraju_scc as petgraph_scc, toposort as petgraph_toposort},
    graphmap::DiGraphMap,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    mem,
};

//**************************************************************************************************
// Context
//**************************************************************************************************

struct Context<'env> {
    env: &'env mut CompilationEnv,
    struct_declared_abilities: UniqueMap<ModuleIdent, UniqueMap<StructName, AbilitySet>>,
    start: Option<Label>,
    loop_begin: Option<Label>,
    loop_end: Option<Label>,
    next_label: Option<Label>,
    label_count: usize,
    blocks: BasicBlocks,
    block_ordering: BTreeMap<Label, usize>,
    // Used for populating block_info
    loop_bounds: BTreeMap<Label, G::LoopInfo>,
    block_info: Vec<(Label, BlockInfo)>,
}

impl<'env> Context<'env> {
    pub fn new(
        env: &'env mut CompilationEnv,
        pre_compiled_lib: Option<&FullyCompiledProgram>,
        modules: &UniqueMap<ModuleIdent, H::ModuleDefinition>,
    ) -> Self {
        let all_modules = modules
            .key_cloned_iter()
            .chain(pre_compiled_lib.iter().flat_map(|pre_compiled| {
                pre_compiled
                    .hlir
                    .modules
                    .key_cloned_iter()
                    .filter(|(mident, _m)| !modules.contains_key(mident))
            }));
        let struct_declared_abilities = UniqueMap::maybe_from_iter(
            all_modules
                .map(|(m, mdef)| (m, mdef.structs.ref_map(|_s, sdef| sdef.abilities.clone()))),
        )
        .unwrap();
        Context {
            env,
            struct_declared_abilities,
            next_label: None,
            loop_begin: None,
            loop_end: None,
            start: None,
            label_count: 0,
            blocks: BasicBlocks::new(),
            block_ordering: BTreeMap::new(),
            block_info: vec![],
            loop_bounds: BTreeMap::new(),
        }
    }

    fn new_label(&mut self) -> Label {
        let count = self.label_count;
        self.label_count += 1;
        Label(count)
    }

    fn insert_block(&mut self, lbl: Label, basic_block: BasicBlock) {
        assert!(self.block_ordering.insert(lbl, self.blocks.len()).is_none());
        assert!(self.blocks.insert(lbl, basic_block).is_none());
        let block_info = match self.loop_bounds.get(&lbl) {
            None => BlockInfo::Other,
            Some(info) => BlockInfo::LoopHead(*info),
        };
        self.block_info.push((lbl, block_info));
    }

    // Returns the blocks inserted in insertion ordering
    pub fn finish_blocks(&mut self) -> (Label, BasicBlocks, Vec<(Label, BlockInfo)>) {
        self.next_label = None;
        let start = self.start.take();
        let blocks = mem::take(&mut self.blocks);
        let block_ordering = mem::take(&mut self.block_ordering);
        let block_info = mem::take(&mut self.block_info);
        self.loop_bounds = BTreeMap::new();
        self.label_count = 0;
        self.loop_begin = None;
        self.loop_end = None;

        // Blocks will eventually be ordered and outputted to bytecode the label. But labels are
        // initially created depth first
        // So the labels need to be remapped based on the insertion order of the block
        // This preserves the original layout of the code as specified by the user (since code is
        // finshed+inserted into the map in original code order)
        let remapping = block_ordering
            .into_iter()
            .map(|(lbl, ordering)| (lbl, Label(ordering)))
            .collect();
        let (start, blocks) = G::remap_labels(&remapping, start.unwrap(), blocks);
        let block_info = block_info
            .into_iter()
            .map(|(lbl, info)| {
                let info = match info {
                    BlockInfo::Other => BlockInfo::Other,
                    BlockInfo::LoopHead(G::LoopInfo {
                        is_loop_stmt,
                        loop_end,
                    }) => {
                        let loop_end = match loop_end {
                            G::LoopEnd::Unused => G::LoopEnd::Unused,
                            G::LoopEnd::Target(end) if remapping.contains_key(&end) => {
                                G::LoopEnd::Target(remapping[&end])
                            }
                            G::LoopEnd::Target(_end) => G::LoopEnd::Unused,
                        };
                        BlockInfo::LoopHead(G::LoopInfo {
                            is_loop_stmt,
                            loop_end,
                        })
                    }
                };
                (remapping[&lbl], info)
            })
            .collect();
        (start, blocks, block_info)
    }
}

//**************************************************************************************************
// Entry
//**************************************************************************************************

pub fn program(
    compilation_env: &mut CompilationEnv,
    pre_compiled_lib: Option<&FullyCompiledProgram>,
    prog: H::Program,
) -> G::Program {
    let H::Program {
        modules: hmodules,
        scripts: hscripts,
    } = prog;

    let mut context = Context::new(compilation_env, pre_compiled_lib, &hmodules);

    let modules = modules(&mut context, hmodules);
    let scripts = scripts(&mut context, hscripts);

    let program = G::Program { modules, scripts };
    visit_program(&mut context, &program);
    program
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
        is_source_module,
        dependency_order,
        friends,
        structs,
        functions: hfunctions,
        constants: hconstants,
    } = mdef;

    context.env.add_warning_filter_scope(warning_filter.clone());
    let constants = constants(context, Some(module_ident), hconstants);
    let functions = hfunctions.map(|name, f| function(context, Some(module_ident), name, f));
    context.env.pop_warning_filter_scope();
    (
        module_ident,
        G::ModuleDefinition {
            warning_filter,
            package_name,
            attributes,
            is_source_module,
            dependency_order,
            friends,
            structs,
            constants,
            functions,
        },
    )
}

fn scripts(
    context: &mut Context,
    hscripts: BTreeMap<Symbol, H::Script>,
) -> BTreeMap<Symbol, G::Script> {
    hscripts
        .into_iter()
        .map(|(n, s)| (n, script(context, s)))
        .collect()
}

fn script(context: &mut Context, hscript: H::Script) -> G::Script {
    let H::Script {
        warning_filter,
        package_name,
        attributes,
        loc,
        constants: hconstants,
        function_name,
        function: hfunction,
    } = hscript;
    context.env.add_warning_filter_scope(warning_filter.clone());
    let constants = constants(context, None, hconstants);
    let function = function(context, None, function_name, hfunction);
    context.env.pop_warning_filter_scope();
    G::Script {
        warning_filter,
        package_name,
        attributes,
        loc,
        constants,
        function_name,
        function,
    }
}

//**************************************************************************************************
// Functions
//**************************************************************************************************

fn constants(
    context: &mut Context,
    module: Option<ModuleIdent>,
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
                BytecodeGeneration::UnfoldableConstant,
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
            context.env.add_diag(diag);
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
            context.env.add_diag(diag!(
                BytecodeGeneration::UnfoldableConstant,
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
            C::Abort(exp) | C::Assign(_, exp) => dep_exp(set, exp),
            C::Mutate(lhs, rhs) => {
                dep_exp(set, lhs);
                dep_exp(set, rhs)
            }
            C::Break | C::Continue | C::Jump { .. } | C::JumpIf { .. } => (),
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
            S::While {
                cond: (cond_block, cond_exp),
                block,
            } => {
                dep_block(set, cond_block);
                dep_exp(set, cond_exp);
                dep_block(set, block)
            }
            S::Loop { block, .. } => dep_block(set, block),
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
    module: Option<ModuleIdent>,
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

    context.env.add_warning_filter_scope(warning_filter.clone());
    let final_value = constant_(
        context,
        constant_values,
        module,
        name,
        loc,
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

    context.env.pop_warning_filter_scope();
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
    module: Option<ModuleIdent>,
    name: ConstantName,
    full_loc: Loc,
    signature: H::BaseType,
    locals: UniqueMap<Var, H::SingleType>,
    block: H::Block,
) -> Option<H::Exp> {
    use H::Command_ as C;
    const ICE_MSG: &str = "ICE invalid constant should have been blocked in typing";

    initial_block(context, block);
    let (start, mut blocks, block_info) = context.finish_blocks();

    let binfo = block_info.iter().map(|(lbl, info)| (lbl, info));
    let (mut cfg, infinite_loop_starts, errors) = MutForwardCFG::new(start, &mut blocks, binfo);
    assert!(infinite_loop_starts.is_empty(), "{}", ICE_MSG);
    assert!(errors.is_empty(), "{}", ICE_MSG);

    let num_previous_errors = context.env.count_diags();
    let fake_signature = H::FunctionSignature {
        type_parameters: vec![],
        parameters: vec![],
        return_type: H::Type_::base(signature),
    };
    let fake_acquires = BTreeMap::new();
    let fake_infinite_loop_starts = BTreeSet::new();
    let function_context = super::CFGContext {
        module,
        member: cfgir::MemberName::Constant(name.0),
        struct_declared_abilities: &context.struct_declared_abilities,
        signature: &fake_signature,
        acquires: &fake_acquires,
        locals: &locals,
        infinite_loop_starts: &fake_infinite_loop_starts,
    };
    cfgir::refine_inference_and_verify(context.env, &function_context, &mut cfg);
    assert!(
        num_previous_errors == context.env.count_diags(),
        "{}",
        ICE_MSG
    );
    cfgir::optimize(&fake_signature, &locals, constant_values, &mut cfg);

    if blocks.len() != 1 {
        context.env.add_diag(diag!(
            BytecodeGeneration::UnfoldableConstant,
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
                context.env.add_diag(diag!(
                    BytecodeGeneration::UnfoldableConstant,
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
        _ => context.env.add_diag(diag!(
            BytecodeGeneration::UnfoldableConstant,
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
    module: Option<ModuleIdent>,
    name: FunctionName,
    f: H::Function,
) -> G::Function {
    let H::Function {
        warning_filter,
        index,
        attributes,
        visibility,
        entry,
        signature,
        acquires,
        body,
    } = f;
    context.env.add_warning_filter_scope(warning_filter.clone());
    let body = function_body(context, module, name, &signature, &acquires, body);
    context.env.pop_warning_filter_scope();
    G::Function {
        warning_filter,
        index,
        attributes,
        visibility,
        entry,
        signature,
        acquires,
        body,
    }
}

fn function_body(
    context: &mut Context,
    module: Option<ModuleIdent>,
    name: FunctionName,
    signature: &H::FunctionSignature,
    acquires: &BTreeMap<StructName, Loc>,
    sp!(loc, tb_): H::FunctionBody,
) -> G::FunctionBody {
    use G::FunctionBody_ as GB;
    use H::FunctionBody_ as HB;
    assert!(context.next_label.is_none());
    assert!(context.start.is_none());
    assert!(context.blocks.is_empty());
    assert!(context.block_ordering.is_empty());
    assert!(context.block_info.is_empty());
    assert!(context.loop_bounds.is_empty());
    assert!(context.loop_begin.is_none());
    assert!(context.loop_end.is_none());
    let b_ = match tb_ {
        HB::Native => GB::Native,
        HB::Defined { locals, body } => {
            initial_block(context, body);
            let (start, mut blocks, block_info) = context.finish_blocks();

            let binfo = block_info.iter().map(|(lbl, info)| (lbl, info));
            let (mut cfg, infinite_loop_starts, diags) =
                MutForwardCFG::new(start, &mut blocks, binfo);
            context.env.add_diags(diags);

            let function_context = super::CFGContext {
                module,
                member: cfgir::MemberName::Function(name.0),
                struct_declared_abilities: &context.struct_declared_abilities,
                signature,
                acquires,
                locals: &locals,
                infinite_loop_starts: &infinite_loop_starts,
            };
            cfgir::refine_inference_and_verify(context.env, &function_context, &mut cfg);
            // do not optimize if there are errors, warnings are okay
            if !context.env.has_errors() {
                cfgir::optimize(signature, &locals, &UniqueMap::new(), &mut cfg);
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

fn initial_block(context: &mut Context, blocks: H::Block) {
    let start = context.new_label();
    context.start = Some(start);
    block(context, start, blocks)
}

fn block(context: &mut Context, mut cur_label: Label, blocks: H::Block) {
    use H::Command_ as C;

    assert!(!blocks.is_empty());
    let loc = blocks.back().unwrap().loc;
    let mut basic_block = block_(context, &mut cur_label, blocks);

    // return if we ended with did not end with a command
    if basic_block.is_empty() {
        return;
    }

    match context.next_label {
        Some(next) if !basic_block.back().unwrap().value.is_terminal() => {
            basic_block.push_back(sp(
                loc,
                C::Jump {
                    target: next,
                    from_user: false,
                },
            ));
        }
        _ => (),
    }
    context.insert_block(cur_label, basic_block);
}

fn block_(context: &mut Context, cur_label: &mut Label, blocks: H::Block) -> BasicBlock {
    use H::{Command_ as C, Statement_ as S};

    assert!(!blocks.is_empty());
    let mut basic_block = BasicBlock::new();

    macro_rules! finish_block {
        (next_label: $next_label:expr) => {{
            let lbl = mem::replace(cur_label, $next_label);
            let bb = mem::take(&mut basic_block);
            context.insert_block(lbl, bb);
        }};
    }

    macro_rules! loop_block {
        (begin: $begin:expr, end: $end:expr, body: $body:expr, $block:expr) => {{
            let begin = $begin;
            let old_begin = mem::replace(&mut context.loop_begin, Some(begin));
            let old_end = mem::replace(&mut context.loop_end, Some($end));
            let old_next = mem::replace(&mut context.next_label, Some(begin));
            block(context, $body, $block);
            context.next_label = old_next;
            context.loop_end = old_end;
            context.loop_begin = old_begin;
        }};
    }

    for sp!(loc, stmt_) in blocks {
        match stmt_ {
            S::Command(mut cmd) => {
                command(context, &mut cmd);
                let is_terminal = cmd.value.is_terminal();
                basic_block.push_back(cmd);
                if is_terminal {
                    finish_block!(next_label: context.new_label());
                }
            }
            S::IfElse {
                cond,
                if_block,
                else_block,
            } => {
                let if_true = context.new_label();
                let if_false = context.new_label();
                let next_label = context.new_label();

                // If cond
                let jump_if = C::JumpIf {
                    cond: *cond,
                    if_true,
                    if_false,
                };
                basic_block.push_back(sp(loc, jump_if));
                finish_block!(next_label: next_label);

                // If branches
                let old_next = mem::replace(&mut context.next_label, Some(next_label));
                block(context, if_true, if_block);
                block(context, if_false, else_block);
                context.next_label = old_next;
            }
            S::While {
                cond: (hcond_block, cond),
                block: loop_block,
            } => {
                let loop_cond = context.new_label();
                let loop_body = context.new_label();
                let loop_end = context.new_label();

                context.loop_bounds.insert(
                    loop_cond,
                    LoopInfo {
                        is_loop_stmt: false,
                        loop_end: G::LoopEnd::Target(loop_end),
                    },
                );

                // Jump to loop condition
                basic_block.push_back(sp(
                    loc,
                    C::Jump {
                        target: loop_cond,
                        from_user: false,
                    },
                ));
                finish_block!(next_label: loop_cond);

                // Loop condition and case to jump into loop or end
                if !hcond_block.is_empty() {
                    assert!(basic_block.is_empty());
                    basic_block = block_(context, cur_label, hcond_block);
                }
                let jump_if = C::JumpIf {
                    cond: *cond,
                    if_true: loop_body,
                    if_false: loop_end,
                };
                basic_block.push_back(sp(loc, jump_if));
                finish_block!(next_label: loop_end);

                // Loop body
                loop_block!(begin: loop_cond, end: loop_end, body: loop_body, loop_block)
            }

            S::Loop {
                block: loop_block, ..
            } => {
                let loop_body = context.new_label();
                let loop_end = context.new_label();
                assert!(cur_label.0 < loop_body.0);
                assert!(loop_body.0 < loop_end.0);

                context.loop_bounds.insert(
                    loop_body,
                    LoopInfo {
                        is_loop_stmt: true,
                        loop_end: G::LoopEnd::Target(loop_end),
                    },
                );

                // Jump to loop
                basic_block.push_back(sp(
                    loc,
                    C::Jump {
                        target: loop_body,
                        from_user: false,
                    },
                ));
                finish_block!(next_label: loop_end);

                // Loop body
                loop_block!(begin: loop_body, end: loop_end, body: loop_body, loop_block)
            }
        }
    }

    basic_block
}

fn command(context: &Context, sp!(_, hc_): &mut H::Command) {
    use H::Command_ as C;
    match hc_ {
        C::Assign(_, _)
        | C::Mutate(_, _)
        | C::Abort(_)
        | C::Return { .. }
        | C::IgnoreAndPop { .. } => {}
        C::Continue => {
            *hc_ = C::Jump {
                target: context.loop_begin.unwrap(),
                from_user: true,
            }
        }
        C::Break => {
            *hc_ = C::Jump {
                target: context.loop_end.unwrap(),
                from_user: true,
            }
        }
        C::Jump { .. } | C::JumpIf { .. } => {
            panic!("ICE unexpected jump before translation to jumps")
        }
    }
}

fn visit_program(context: &mut Context, prog: &G::Program) {
    if context.env.visitors().abs_int.is_empty() {
        return;
    }

    for (mident, mdef) in prog.modules.key_cloned_iter() {
        visit_module(context, prog, mident, mdef)
    }

    for script in prog.scripts.values() {
        visit_script(context, prog, script)
    }
}

fn visit_module(
    context: &mut Context,
    prog: &G::Program,
    mident: ModuleIdent,
    mdef: &G::ModuleDefinition,
) {
    context
        .env
        .add_warning_filter_scope(mdef.warning_filter.clone());
    for (name, fdef) in mdef.functions.key_cloned_iter() {
        visit_function(context, prog, Some(mident), name, fdef)
    }
    context.env.pop_warning_filter_scope();
}

fn visit_script(context: &mut Context, prog: &G::Program, script: &G::Script) {
    context
        .env
        .add_warning_filter_scope(script.warning_filter.clone());
    visit_function(context, prog, None, script.function_name, &script.function);
    context.env.pop_warning_filter_scope();
}

fn visit_function(
    context: &mut Context,
    prog: &G::Program,
    mident: Option<ModuleIdent>,
    name: FunctionName,
    fdef: &G::Function,
) {
    let G::Function {
        warning_filter,
        index: _,
        attributes: _,
        visibility: _,
        entry: _,
        signature,
        acquires,
        body,
    } = fdef;
    let G::FunctionBody_::Defined {
        locals,
        start,
        blocks,
        block_info,
    } = &body.value
    else {
        return;
    };
    context.env.add_warning_filter_scope(warning_filter.clone());
    let (cfg, infinite_loop_starts) = ImmForwardCFG::new(*start, blocks, block_info.iter());
    let function_context = super::CFGContext {
        module: mident,
        member: cfgir::MemberName::Function(name.0),
        struct_declared_abilities: &context.struct_declared_abilities,
        signature,
        acquires,
        locals,
        infinite_loop_starts: &infinite_loop_starts,
    };
    let mut ds = Diagnostics::new();
    for visitor in &context.env.visitors().abs_int {
        let mut v = visitor.borrow_mut();
        ds.extend(v.verify(context.env, prog, &function_context, &cfg));
    }
    context.env.add_diags(ds);
    context.env.pop_warning_filter_scope();
}
