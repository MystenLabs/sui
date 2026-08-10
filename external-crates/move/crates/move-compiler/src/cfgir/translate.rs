// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    PreCompiledProgramInfo,
    cfgir::{
        self,
        ast::{self as G, BasicBlock, BasicBlocks, BlockInfo},
        cfg::{ImmForwardCFG, MutForwardCFG},
        constants::{ConstantEntry, folded_constants, generate_cross_module_constants},
        visitor::{CFGIRVisitor, CFGIRVisitorConstructor, CFGIRVisitorContext},
    },
    diag,
    diagnostics::{Diagnostic, DiagnosticReporter, Diagnostics, filter::FilterScope},
    expansion::ast::{Attributes, ModuleIdent, Mutability},
    hlir::{
        ast::{self as H, BlockLabel, Label, Value, Value_, Var},
        translate::precompiled_constants,
    },
    ice_assert,
    parser::ast::{ConstantName, FunctionName},
    shared::{AstDebug, CompilationEnv, program_info::TypingProgramInfo, unique_map::UniqueMap},
};
use cfgir::ast::LoopInfo;
use move_core_types::{account_address::AccountAddress as MoveAddress, runtime_value::MoveValue};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use move_symbol_pool::Symbol;
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

pub(super) struct Context<'env> {
    pub(super) env: &'env CompilationEnv,
    pub(super) info: &'env TypingProgramInfo,
    reporter: DiagnosticReporter<'env>,
    pub(super) current_package: Option<Symbol>,
    /// the fold state of every constant in the program, populated by the global constant pass
    pub(super) constant_defs: BTreeMap<(ModuleIdent, ConstantName), ConstantEntry>,
    /// stable per-compilation module ids used in constant copy names
    pub(super) module_ids: BTreeMap<ModuleIdent, usize>,
    pub(super) precompiled_module_ids: BTreeMap<ModuleIdent, usize>,
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
            constant_defs: BTreeMap::new(),
            module_ids: BTreeMap::new(),
            precompiled_module_ids: BTreeMap::new(),
            label_count: 0,
            named_blocks: UniqueMap::new(),
            loop_bounds: BTreeMap::new(),
            debug: CFGIRDebugFlags {
                print_blocks: false,
                print_optimized_blocks: false,
            },
        }
    }

    pub(super) fn reporter(&self) -> &DiagnosticReporter<'env> {
        &self.reporter
    }

    pub fn add_diag(&self, diag: Diagnostic) {
        self.reporter.add_diag(diag);
    }

    pub fn add_diags(&self, diags: Diagnostics) {
        self.reporter.add_diags(diags);
    }

    pub fn push_warning_filter_scope(&mut self, filters: FilterScope) {
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
    pre_compiled_program: Option<Arc<PreCompiledProgramInfo>>,
    prog: H::Program,
) -> G::Program {
    let H::Program {
        modules: hmodules,
        info,
    } = prog;

    let mut context = Context::new(compilation_env, &info);
    let modules = modules(&mut context, hmodules);
    set_constant_value_types(&info, &modules);

    let mut program = G::Program {
        modules,
        info: info.clone(),
    };
    visit_program(&mut context, pre_compiled_program, &mut program);
    program
}

fn set_constant_value_types(
    info: &TypingProgramInfo,
    modules: &UniqueMap<ModuleIdent, G::ModuleDefinition>,
) {
    for (mname, mdef) in modules.key_cloned_iter() {
        for (cname, cdef) in mdef.constants.key_cloned_iter() {
            // synthesized copies of cross-module constants have no typing-info counterpart
            let Some(info_cdef) = info.module(&mname).constants.get(&cname) else {
                continue;
            };
            if let Some(value) = &cdef.value {
                info_cdef.value.set(value.clone()).unwrap();
            }
        }
    }
}

fn modules(
    context: &mut Context,
    hmodules: UniqueMap<ModuleIdent, H::ModuleDefinition>,
) -> UniqueMap<ModuleIdent, G::ModuleDefinition> {
    let mut hmodules = hmodules.into_iter().collect::<Vec<_>>();
    // module ids used in constant copy names; the position in the (sorted) module list is stable
    // and unique, unlike dependency order, which is only assigned to modules with dependencies
    for (id, (mname, _)) in hmodules.iter().enumerate() {
        context.module_ids.insert(*mname, id);
    }
    let mut constant_values: BTreeMap<(ModuleIdent, ConstantName), Value> = BTreeMap::new();
    seed_precompiled_constants(context, &hmodules, &mut constant_values);
    // All constants are folded up front, in the order of their own dependency graph; module
    // order is irrelevant
    let mut folded_constants = folded_constants(context, &mut hmodules, &mut constant_values);
    let constant_values = &constant_values;
    let modules = hmodules.into_iter().map(|(mname, m)| {
        let constants = folded_constants.remove(&mname).unwrap_or_default();
        module(context, constant_values, constants, mname, m)
    });
    UniqueMap::maybe_from_iter(modules).unwrap()
}

fn module(
    context: &mut Context,
    constant_values: &BTreeMap<(ModuleIdent, ConstantName), Value>,
    mut constants: UniqueMap<ConstantName, G::Constant>,
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
    assert!(
        hconstants.is_empty(),
        "ICE constants should have been taken by the global constant pass"
    );
    context.current_package = package_name;
    context.push_warning_filter_scope(warning_filter.clone());
    let mut functions = hfunctions.map(|name, f| function(context, module_ident, name, f));
    generate_cross_module_constants(
        context,
        constant_values,
        module_ident,
        &mut constants,
        &mut functions,
    );
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
// Constants
//**************************************************************************************************

/// Constants of modules outside this compilation (e.g. the analyzer's pre-compiled dependencies)
/// were folded when they were compiled; seed their values so they can be folded into constant
/// definitions and copied into function bodies like any other constant. The conversion into HLIR
/// form lives with the rest of type lowering in `hlir::translate`; the map is transient to this
/// pass.
fn seed_precompiled_constants(
    context: &mut Context,
    hmodules: &[(ModuleIdent, H::ModuleDefinition)],
    constant_values: &mut BTreeMap<(ModuleIdent, ConstantName), Value>,
) {
    let compiled: BTreeSet<ModuleIdent> = hmodules.iter().map(|(mname, _)| *mname).collect();
    // constants are only visible within their package, so only modules from the packages being
    // compiled can contribute
    let packages: BTreeSet<Option<Symbol>> =
        hmodules.iter().map(|(_, mdef)| mdef.package_name).collect();
    let precompiled = precompiled_constants(&context.reporter, context.info, &compiled, &packages);
    for ((mident, cname), (defined_loc, signature, value)) in precompiled {
        let next_id = context.precompiled_module_ids.len();
        context
            .precompiled_module_ids
            .entry(mident)
            .or_insert(next_id);
        let entry = match value {
            Some(value) => {
                constant_values.insert((mident, cname), value);
                ConstantEntry::Defined {
                    loc: defined_loc,
                    signature: Box::new(signature),
                }
            }
            None => ConstantEntry::Failed,
        };
        context.constant_defs.insert((mident, cname), entry);
    }
}

pub(super) fn constant(
    context: &mut Context,
    constant_values: &mut BTreeMap<(ModuleIdent, ConstantName), Value>,
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
    let entry = match final_value {
        Some(H::Exp {
            exp: sp!(_, H::UnannotatedExp_::Value(value)),
            ..
        }) => {
            let prev = constant_values.insert((module, name), value.clone());
            ice_assert!(
                context.reporter,
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
        // an error was reported during folding
        _ => (ConstantEntry::Failed, None),
    };
    let (entry, value) = entry;
    let prev_entry = context.constant_defs.insert((module, name), entry);
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

const CANNOT_FOLD: &str =
    "Invalid expression in 'const'. This expression could not be evaluated to a value";

fn constant_(
    context: &mut Context,
    constant_values: &BTreeMap<(ModuleIdent, ConstantName), Value>,
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
        pre_compiled_program: None,
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
        let exps = blocks
            .values()
            .flat_map(|block| block.iter())
            .flat_map(|cmd| command_exps(&cmd.value));
        report_cannot_fold(context, module, full_loc, exps);
        return None;
    }
    let mut optimized_block = blocks.remove(&start).unwrap();
    let return_cmd = optimized_block.pop_back().unwrap();
    for sp!(cloc, cmd_) in &optimized_block {
        let e = match cmd_ {
            C::IgnoreAndPop { exp, .. } => exp,
            _ => {
                report_cannot_fold(context, module, *cloc, command_exps(cmd_));
                continue;
            }
        };
        check_constant_value(context, module, e)
    }

    let result = match return_cmd.value {
        C::Return { exp: e, .. } => e,
        _ => unreachable!(),
    };
    check_constant_value(context, module, &result);
    Some(result)
}

fn check_constant_value(context: &mut Context, module: ModuleIdent, e: &H::Exp) {
    use H::UnannotatedExp_ as E;
    match &e.exp.value {
        E::Value(_) => (),
        _ => report_cannot_fold(context, module, e.exp.loc, std::iter::once(e)),
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

/// Reports a constant definition that could not be folded to a value: cross-module references to
/// constants that themselves failed to fold (or are outside the current compilation) are reported
/// as the root cause, otherwise the generic unfoldable-constant error is reported at `loc`.
fn report_cannot_fold<'a>(
    context: &mut Context,
    module: ModuleIdent,
    loc: Loc,
    exps: impl Iterator<Item = &'a H::Exp>,
) {
    let mut unresolved = vec![];
    for e in exps {
        unresolved_cross_module_constants(context, module, e, &mut unresolved);
    }
    if unresolved.is_empty() {
        context.add_diag(diag!(
            CodeGeneration::UnfoldableConstant,
            (loc, CANNOT_FOLD)
        ));
        return;
    }
    for (m, c, use_loc) in unresolved {
        let diag = match context.constant_defs.get_key_value(&(m, c)) {
            Some(((_, defined), _)) => {
                unfoldable_constant_use_error(&m, &c, use_loc, defined.0.loc)
            }
            None => outside_compilation_use(&m, &c, use_loc),
        };
        context.add_diag(diag);
    }
}

/// The error reported at each use of a constant whose definition could not be evaluated to a value
pub(super) fn unfoldable_constant_use_error(
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

/// The error reported at each use of a constant defined outside the current compilation
pub(super) fn outside_compilation_use(
    m: &ModuleIdent,
    c: &ConstantName,
    use_loc: Loc,
) -> Diagnostic {
    let msg = format!(
        "Invalid access of '{}::{}'. Constants defined in modules outside of the current \
         compilation cannot be accessed from other modules",
        m, c
    );
    diag!(TypeSafety::Visibility, (use_loc, msg))
}

/// Collects cross-module constant references that failed to fold or are outside the current
/// compilation
#[growing_stack]
fn unresolved_cross_module_constants(
    context: &Context,
    module: ModuleIdent,
    e: &H::Exp,
    unresolved: &mut Vec<(ModuleIdent, ConstantName, Loc)>,
) {
    use H::UnannotatedExp_ as E;
    match &e.exp.value {
        E::Constant(m, c) => {
            if *m != module
                && matches!(
                    context.constant_defs.get(&(*m, *c)),
                    Some(ConstantEntry::Failed) | None
                )
            {
                unresolved.push((*m, *c, e.exp.loc));
            }
        }
        E::UnaryExp(_, rhs) => unresolved_cross_module_constants(context, module, rhs, unresolved),
        E::BinopExp(lhs, _, rhs) => {
            unresolved_cross_module_constants(context, module, lhs, unresolved);
            unresolved_cross_module_constants(context, module, rhs, unresolved)
        }
        E::Cast(base, _) => unresolved_cross_module_constants(context, module, base, unresolved),
        E::Vector(_, _, _, args) | E::Multiple(args) => {
            for arg in args {
                unresolved_cross_module_constants(context, module, arg, unresolved);
            }
        }
        // other forms cannot appear in constant bodies
        _ => (),
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
    context.push_warning_filter_scope(warning_filter.clone());
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
            // cross-module constant uses are rewritten to module-local copies after translation,
            // by the constant selection pass (see `cfgir::constants`)
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
                pre_compiled_program: None,
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
                // TODO thread through constants
                let constants = &UniqueMap::new();
                cfgir::optimize(
                    context.env,
                    &context.reporter,
                    context.current_package,
                    signature,
                    &locals,
                    &BTreeMap::new(),
                    &mut cfg,
                );
                cfgir::report_always_erroring_operations(&context.reporter, constants, &cfg);
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

fn visit_program(
    context: &mut Context,
    pre_compiled_program: Option<Arc<PreCompiledProgramInfo>>,
    prog: &mut G::Program,
) {
    if context.env.visitors().abs_int.is_empty() && context.env.visitors().cfgir.is_empty() {
        return;
    }

    AbsintVisitor.visit(context.env, pre_compiled_program.clone(), prog);

    context
        .env
        .visitors()
        .cfgir
        .par_iter()
        .for_each(|v| v.visit(context.env, pre_compiled_program.clone(), prog));
}

struct AbsintVisitor;
struct AbsintVisitorContext<'a> {
    env: &'a CompilationEnv,
    pre_compiled_program: Option<Arc<PreCompiledProgramInfo>>,
    reporter: DiagnosticReporter<'a>,
    info: Arc<TypingProgramInfo>,
    current_package: Option<Symbol>,
}

impl CFGIRVisitorConstructor for AbsintVisitor {
    type Context<'a> = AbsintVisitorContext<'a>;

    fn context<'a>(
        env: &'a CompilationEnv,
        pre_compiled_program: Option<Arc<PreCompiledProgramInfo>>,
        program: &G::Program,
    ) -> Self::Context<'a> {
        let reporter = env.diagnostic_reporter_at_top_level();
        AbsintVisitorContext {
            env,
            pre_compiled_program,
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

impl CFGIRVisitorContext for AbsintVisitorContext<'_> {
    fn push_warning_filter_scope(&mut self, filters: FilterScope) {
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
            pre_compiled_program: self.pre_compiled_program.clone(),
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
