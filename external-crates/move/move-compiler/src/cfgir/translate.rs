// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::{
        self,
        ast::{self as G, BasicBlock, BasicBlocks, BlockInfo},
        cfg::{build_dead_code_error, ImmForwardCFG, MutForwardCFG},
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
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    mem,
};

//**************************************************************************************************
// Context
//**************************************************************************************************

struct Context<'env> {
    env: &'env mut CompilationEnv,
    struct_declared_abilities: UniqueMap<ModuleIdent, UniqueMap<StructName, AbilitySet>>,
    label_count: usize,
    loop_env: Option<LoopEnv>,
    // Used for populating block_info
    loop_bounds: BTreeMap<Label, G::LoopInfo>,
}

struct LoopEnv {
    start_label: Label,
    end_label: Label,
    previous_env: Box<Option<LoopEnv>>,
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
            label_count: 0,
            loop_env: None,
            loop_bounds: BTreeMap::new(),
        }
    }

    fn new_label(&mut self) -> Label {
        let count = self.label_count;
        self.label_count += 1;
        Label(count)
    }

    fn start_loop(&mut self, is_loop_stmt: bool) -> (Label, Label) {
        let start_label = self.new_label();
        let end_label = self.new_label();
        self.loop_bounds.insert(
            start_label,
            LoopInfo {
                is_loop_stmt,
                loop_end: G::LoopEnd::Target(end_label),
            },
        );
        // push a new loop env
        let old_env = mem::take(&mut self.loop_env);
        self.loop_env = Some(LoopEnv {
            start_label,
            end_label,
            previous_env: Box::new(old_env),
        });
        (start_label, end_label)
    }

    fn end_loop(&mut self) {
        // pop the current loop env
        assert!(
            self.loop_env.is_some(),
            "ICE called end_loop while not in a loop"
        );
        let old_env = mem::take(&mut self.loop_env);
        self.loop_env = *old_env.unwrap().previous_env;
    }

    fn clear_block_state(&mut self) {
        assert!(self.loop_env.is_none());
        self.label_count = 0;
        self.loop_bounds = BTreeMap::new();
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
    let constants = hconstants.map(|name, c| constant(context, Some(module_ident), name, c));
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
    let constants = hconstants.map(|name, c| constant(context, None, name, c));
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

fn constant(
    context: &mut Context,
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
    let final_value = constant_(context, module, name, loc, signature.clone(), locals, block);
    let value = final_value.and_then(move_value_from_exp);

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
    module: Option<ModuleIdent>,
    name: ConstantName,
    full_loc: Loc,
    signature: H::BaseType,
    locals: UniqueMap<Var, H::SingleType>,
    body: H::Block,
) -> Option<H::Exp> {
    use H::Command_ as C;
    const ICE_MSG: &str = "ICE invalid constant should have been blocked in typing";
    let blocks = block(context, body);
    let (start, mut blocks, block_info) = finalize_blocks(context, blocks);
    context.clear_block_state();

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
    cfgir::optimize(&fake_signature, &locals, &mut cfg);

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

fn move_value_from_exp(e: H::Exp) -> Option<MoveValue> {
    use H::UnannotatedExp_ as E;
    match e.exp.value {
        E::Value(v) => Some(move_value_from_value(v)),
        _ => None,
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
    assert!(context.loop_bounds.is_empty());
    assert!(context.loop_env.is_none());
    let b_ = match tb_ {
        HB::Native => GB::Native,
        HB::Defined { locals, body } => {
            let blocks = block(context, body);
            let (start, mut blocks, block_info) = finalize_blocks(context, blocks);
            context.clear_block_state();

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
                cfgir::optimize(signature, &locals, &mut cfg);
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

fn block(context: &mut Context, stmts: H::Block) -> BlockList {
    let (start_block, blocks) = block_(context, stmts);
    [(context.new_label(), start_block)]
        .into_iter()
        .chain(blocks)
        .collect()
}

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
    // 1. Generate an in-order mat from that list.
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
        let info = match context.loop_bounds.get(&lbl) {
            None => BlockInfo::Other,
            Some(LoopInfo {
                is_loop_stmt,
                loop_end,
            }) => {
                let loop_end = match loop_end {
                    G::LoopEnd::Target(end) if label_map.contains_key(&end) => {
                        G::LoopEnd::Target(label_map[&end])
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
        block_info.push((label_map[&lbl], info));
    }

    let block_map: BasicBlocks = BTreeMap::from_iter(blocks.into_iter());
    let (out_label, out_blocks) = G::remap_labels(&label_map, start_label, block_map);
    (out_label, out_blocks, block_info)
}

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
                .chain(true_blocks.into_iter())
                .chain([(false_label, false_entry_block)])
                .chain(false_blocks.into_iter())
                .chain([(phi_label, current_block)])
                .collect::<BlockList>();

            (test_block, new_blocks)
        }
        // We could turn these into loops earlier and elide this case.
        S::While {
            cond: (test_block, test),
            block: body,
        } => {
            let (start_label, end_label) = context.start_loop(false);
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

            context.end_loop();

            let new_blocks = [(start_label, initial_test_block)]
                .into_iter()
                .chain(test_blocks.into_iter())
                .chain([(body_label, body_entry_block)])
                .chain(body_blocks.into_iter())
                .chain([(end_label, current_block)])
                .collect::<BlockList>();

            (entry_block, new_blocks)
        }
        S::Loop {
            block: body,
            has_break: _,
        } => {
            let (start_label, end_label) = context.start_loop(true);

            let entry_block = VecDeque::from([make_jump(sloc, start_label, false)]);

            let (body_entry_block, body_blocks) = block_(
                context,
                with_last(body, make_jump(sloc, start_label, false)),
            );

            context.end_loop();

            let new_blocks = [(start_label, body_entry_block)]
                .into_iter()
                .chain(body_blocks.into_iter())
                .chain([(end_label, current_block)])
                .collect::<BlockList>();

            (entry_block, new_blocks)
        }
        S::Command(sp!(cloc, C::Break)) => {
            // Discard the current block because it's dead code.
            dead_code_error(context, &current_block);
            let break_jump = make_jump(cloc, context.loop_env.as_ref().unwrap().end_label, true);
            (VecDeque::from([break_jump]), vec![])
        }
        S::Command(sp!(cloc, C::Continue)) => {
            // Discard the current block because it's dead code.
            dead_code_error(context, &current_block);
            (
                VecDeque::from([make_jump(
                    cloc,
                    context.loop_env.as_ref().unwrap().start_label,
                    true,
                )]),
                vec![],
            )
        }
        S::Command(cmd) if cmd.value.is_terminal() => {
            // Discard the current block because it's dead code.
            dead_code_error(context, &current_block);
            (VecDeque::from([cmd]), vec![])
        }
        S::Command(cmd) => {
            current_block.push_front(cmd);
            (current_block, vec![])
        }
    }
}

fn with_last(mut block: H::Block, sp!(loc, cmd): H::Command) -> H::Block {
    let stmt = sp(loc, H::Statement_::Command(sp(loc, cmd)));
    block.push_back(stmt);
    block
}

fn make_jump(loc: Loc, target: Label, from_user: bool) -> H::Command {
    sp(loc, H::Command_::Jump { target, from_user })
}

fn dead_code_error(context: &mut Context, block: &BasicBlock) {
    match build_dead_code_error(block) {
        Some(diag) => context.env.add_diag(diag),
        None => (),
    }
}

//**************************************************************************************************
// Visitors
//**************************************************************************************************

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
    let G::FunctionBody_::Defined { locals, start, blocks, block_info } = &body.value else {
        return
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
