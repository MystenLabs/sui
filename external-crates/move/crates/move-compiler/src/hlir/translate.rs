// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    debug_display, debug_display_verbose, diag,
    diagnostics::{warning_filters::WarningFilters, Diagnostic, DiagnosticReporter, Diagnostics},
    editions::{FeatureGate, Flavor},
    expansion::ast::{self as E, Fields, ModuleIdent, Mutability},
    hlir::{
        ast::{self as H, Block, BlockLabel, MoveOpAnnotation, UnpackType},
        detect_dead_code::program as detect_dead_code_analysis,
        match_compilation,
    },
    ice,
    naming::ast as N,
    parser::ast::{
        Ability_, BinOp, BinOp_, ConstantName, DatatypeName, Field, FunctionName, TargetKind,
        VariantName,
    },
    shared::{
        matching::{new_match_var_name, MatchContext, MATCH_TEMP_PREFIX},
        program_info::TypingProgramInfo,
        string_utils::debug_print,
        unique_map::UniqueMap,
        *,
    },
    sui_mode::ID_FIELD_NAME,
    typing::ast as T,
    FullyCompiledProgram,
};

use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use move_symbol_pool::Symbol;
use once_cell::sync::Lazy;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    convert::TryInto,
    sync::Arc,
};

//**************************************************************************************************
// Vars
//**************************************************************************************************

pub const NEW_NAME_DELIM: &str = "#";

fn translate_var(sp!(loc, v_): N::Var) -> H::Var {
    let N::Var_ {
        name,
        id: depth,
        color,
    } = v_;
    let s = format!(
        "{}{}{}{}{}",
        name, NEW_NAME_DELIM, depth, NEW_NAME_DELIM, color
    )
    .into();
    H::Var(sp(loc, s))
}

fn translate_block_label(lbl: N::BlockLabel) -> H::BlockLabel {
    let N::BlockLabel {
        label: sp!(loc, v_),
        ..
    } = lbl;
    let N::Var_ {
        name,
        id: depth,
        color,
    } = v_;
    let s = format!("{name}{NEW_NAME_DELIM}{depth}{NEW_NAME_DELIM}{color}").into();
    H::BlockLabel(sp(loc, s))
}

const TEMP_PREFIX: &str = "%";
static TEMP_PREFIX_SYMBOL: Lazy<Symbol> = Lazy::new(|| TEMP_PREFIX.into());

fn new_temp_name(context: &mut Context) -> Symbol {
    format!(
        "{}{}{}",
        *TEMP_PREFIX_SYMBOL,
        NEW_NAME_DELIM,
        context.counter_next()
    )
    .into()
}

pub fn is_temp_name(s: Symbol) -> bool {
    s.starts_with(TEMP_PREFIX)
}

pub fn is_match_temp_name(s: Symbol) -> bool {
    s.starts_with(MATCH_TEMP_PREFIX)
}

pub enum DisplayVar {
    Orig(String),
    Tmp,
    MatchTmp(String),
}

pub fn display_var(s: Symbol) -> DisplayVar {
    if is_temp_name(s) {
        DisplayVar::Tmp
    } else if is_match_temp_name(s) {
        DisplayVar::MatchTmp(s.to_string())
    } else {
        let mut orig = s.as_str().to_string();
        if let Some(i) = orig.find(NEW_NAME_DELIM) {
            orig.truncate(i)
        }
        DisplayVar::Orig(orig)
    }
}

//**************************************************************************************************
// Context
//**************************************************************************************************

pub(super) struct HLIRDebugFlags {
    #[allow(dead_code)]
    pub(super) match_variant_translation: bool,
    #[allow(dead_code)]
    pub(super) match_translation: bool,
    #[allow(dead_code)]
    pub(super) match_specialization: bool,
    #[allow(dead_code)]
    pub(super) function_translation: bool,
    #[allow(dead_code)]
    pub(super) eval_order: bool,
}

pub(super) struct Context<'env> {
    pub env: &'env CompilationEnv,
    pub info: Arc<TypingProgramInfo>,
    #[allow(dead_code)]
    pub debug: HLIRDebugFlags,
    pub reporter: DiagnosticReporter<'env>,
    current_package: Option<Symbol>,
    function_locals: UniqueMap<H::Var, (Mutability, H::SingleType)>,
    signature: Option<H::FunctionSignature>,
    tmp_counter: usize,
    named_block_binders: UniqueMap<H::BlockLabel, Vec<H::LValue>>,
    named_block_types: UniqueMap<H::BlockLabel, H::Type>,
    /// collects all struct fields used in the current module
    pub used_fields: BTreeMap<Symbol, BTreeSet<Symbol>>,
}

impl<'env> Context<'env> {
    pub fn new(
        env: &'env CompilationEnv,
        _pre_compiled_lib_opt: Option<Arc<FullyCompiledProgram>>,
        prog: &T::Program,
    ) -> Self {
        let debug = HLIRDebugFlags {
            match_variant_translation: false,
            function_translation: false,
            eval_order: false,
            match_translation: false,
            match_specialization: false,
        };
        let reporter = env.diagnostic_reporter_at_top_level();
        Context {
            env,
            reporter,
            info: prog.info.clone(),
            debug,
            current_package: None,
            function_locals: UniqueMap::new(),
            signature: None,
            tmp_counter: 0,
            used_fields: BTreeMap::new(),
            named_block_binders: UniqueMap::new(),
            named_block_types: UniqueMap::new(),
        }
    }

    pub fn add_diag(&self, diag: Diagnostic) {
        self.reporter.add_diag(diag);
    }

    #[allow(unused)]
    pub fn add_diags(&self, diags: Diagnostics) {
        self.reporter.add_diags(diags);
    }

    pub fn push_warning_filter_scope(&mut self, filters: WarningFilters) {
        self.reporter.push_warning_filter_scope(filters)
    }

    pub fn pop_warning_filter_scope(&mut self) {
        self.reporter.pop_warning_filter_scope()
    }
    pub fn has_empty_locals(&self) -> bool {
        self.function_locals.is_empty()
    }

    pub fn extract_function_locals(&mut self) -> UniqueMap<H::Var, (Mutability, H::SingleType)> {
        self.tmp_counter = 0;
        std::mem::replace(&mut self.function_locals, UniqueMap::new())
    }

    pub fn new_temp(&mut self, loc: Loc, t: H::SingleType) -> H::Var {
        let new_var = H::Var(sp(loc, new_temp_name(self)));
        self.function_locals
            .add(new_var, (Mutability::Either, t))
            .unwrap();

        new_var
    }

    pub fn bind_local(&mut self, mut_: Mutability, v: N::Var, t: H::SingleType) {
        let symbol = translate_var(v);
        // We may reuse a name if it appears on both sides of an `or` pattern
        if let Some((cur_mut, cur_t)) = self.function_locals.get(&symbol) {
            assert!(cur_t == &t);
            assert!(
                cur_mut == &mut_,
                "{:?} changed mutability from {:?} to {:?}",
                v,
                cur_mut,
                mut_
            );
        } else {
            self.function_locals.add(symbol, (mut_, t)).unwrap();
        }
    }

    pub fn enter_named_block(
        &mut self,
        block_name: H::BlockLabel,
        binders: Vec<H::LValue>,
        ty: H::Type,
    ) {
        self.named_block_binders
            .add(block_name, binders)
            .expect("ICE reused block name");
        self.named_block_types
            .add(block_name, ty)
            .expect("ICE reused named block name");
    }

    pub fn lookup_named_block_binders(&mut self, block_name: &H::BlockLabel) -> Vec<H::LValue> {
        self.named_block_binders
            .get(block_name)
            .expect("ICE named block with no binders")
            .clone()
    }

    pub fn exit_named_block(&mut self, block_name: H::BlockLabel) {
        self.named_block_binders
            .remove(&block_name)
            .expect("Tried to leave an unnkown block");
        self.named_block_types
            .remove(&block_name)
            .expect("Tried to leave an unnkown block");
    }

    pub fn lookup_named_block_type(&mut self, block_name: &H::BlockLabel) -> Option<H::Type> {
        self.named_block_types.get(block_name).cloned()
    }

    fn counter_next(&mut self) -> usize {
        self.tmp_counter += 1;
        self.tmp_counter
    }

    fn exit_function(&mut self) {
        self.signature = None;
        self.named_block_binders = UniqueMap::new();
        self.named_block_types = UniqueMap::new();
    }
}

impl MatchContext<true> for Context<'_> {
    fn env(&self) -> &CompilationEnv {
        self.env
    }

    fn reporter(&self) -> &DiagnosticReporter {
        &self.reporter
    }

    /// Makes a new `naming/ast.rs` variable. Does _not_ record it as a function local, since this
    /// should only be called in match compilation, which will have its body processed in HLIR
    /// translation after expansion.
    fn new_match_var(&mut self, name: String, loc: Loc) -> N::Var {
        let id = self.counter_next();
        let name = new_match_var_name(&name, id);
        // NOTE: this color is "wrong" insofar as it really should reflect whatever the current
        // color scope is. Since these are only used as match temporaries, however, and they have
        // names that may not be written as input, it's impossible for these to shadow macro
        // argument names.
        sp(
            loc,
            N::Var_ {
                name,
                id: id as u16,
                color: 0,
            },
        )
    }

    fn program_info(&self) -> &program_info::ProgramInfo<true> {
        self.info.as_ref()
    }
}

//**************************************************************************************************
// Entry
//**************************************************************************************************

pub fn program(
    compilation_env: &CompilationEnv,
    pre_compiled_lib: Option<Arc<FullyCompiledProgram>>,
    prog: T::Program,
) -> H::Program {
    detect_dead_code_analysis(compilation_env, &prog);

    let mut context = Context::new(compilation_env, pre_compiled_lib, &prog);
    let T::Program {
        modules: tmodules,
        warning_filters_table,
        info,
    } = prog;
    let modules = modules(&mut context, tmodules);

    H::Program {
        modules,
        warning_filters_table,
        info,
    }
}

fn modules(
    context: &mut Context,
    modules: UniqueMap<ModuleIdent, T::ModuleDefinition>,
) -> UniqueMap<ModuleIdent, H::ModuleDefinition> {
    let hlir_modules = modules
        .into_iter()
        .map(|(mname, m)| module(context, mname, m));
    UniqueMap::maybe_from_iter(hlir_modules).unwrap()
}

fn module(
    context: &mut Context,
    module_ident: ModuleIdent,
    mdef: T::ModuleDefinition,
) -> (ModuleIdent, H::ModuleDefinition) {
    let T::ModuleDefinition {
        doc: _,
        loc: _,
        warning_filter,
        package_name,
        attributes,
        target_kind,
        dependency_order,
        immediate_neighbors: _,
        used_addresses: _,
        use_funs: _,
        syntax_methods: _,
        friends,
        structs: tstructs,
        enums: tenums,
        functions: tfunctions,
        constants: tconstants,
    } = mdef;
    context.current_package = package_name;
    context.push_warning_filter_scope(warning_filter);
    let structs = tstructs.map(|name, s| struct_def(context, name, s));
    let enums = tenums.map(|name, s| enum_def(context, name, s));

    let constants = tconstants.map(|name, c| constant(context, name, c));
    let functions = tfunctions.filter_map(|name, f| {
        if f.macro_.is_none() {
            Some(function(context, name, f))
        } else {
            None
        }
    });

    gen_unused_warnings(context, target_kind, &structs);

    context.current_package = None;
    context.pop_warning_filter_scope();
    (
        module_ident,
        H::ModuleDefinition {
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

fn function(context: &mut Context, _name: FunctionName, f: T::Function) -> H::Function {
    assert!(context.has_empty_locals());
    assert!(context.tmp_counter == 0);
    let T::Function {
        doc: _,
        warning_filter,
        index,
        attributes,
        loc,
        compiled_visibility: tcompiled_visibility,
        visibility: tvisibility,
        entry,
        macro_,
        signature,
        body,
    } = f;
    assert!(macro_.is_none(), "ICE macros filtered above");
    context.push_warning_filter_scope(warning_filter);
    let signature = function_signature(context, signature);
    let body = function_body(context, &signature, _name, body);
    context.pop_warning_filter_scope();
    H::Function {
        warning_filter,
        index,
        attributes,
        loc,
        compiled_visibility: visibility(tcompiled_visibility),
        visibility: visibility(tvisibility),
        entry,
        signature,
        body,
    }
}

fn function_signature(context: &mut Context, sig: N::FunctionSignature) -> H::FunctionSignature {
    let type_parameters = sig.type_parameters;
    let parameters = sig
        .parameters
        .into_iter()
        .map(|(mut_, v, tty)| {
            let ty = single_type(context, tty);
            context.bind_local(mut_, v, ty.clone());
            (mut_, translate_var(v), ty)
        })
        .collect();
    let return_type = type_(context, sig.return_type);
    H::FunctionSignature {
        type_parameters,
        parameters,
        return_type,
    }
}

fn function_body(
    context: &mut Context,
    sig: &H::FunctionSignature,
    _name: FunctionName,
    sp!(loc, tb_): T::FunctionBody,
) -> H::FunctionBody {
    use H::FunctionBody_ as HB;
    use T::FunctionBody_ as TB;
    let b_ = match tb_ {
        TB::Native => {
            context.extract_function_locals();
            HB::Native
        }
        TB::Defined((_, seq)) => {
            debug_print!(context.debug.function_translation,
                         (msg format!("-- {} ----------------", _name)),
                         (lines "body" => &seq; verbose));
            let (locals, body) = function_body_defined(context, sig, loc, seq);
            debug_print!(context.debug.function_translation,
                         (msg "--------"),
                         (lines "body" => &body; verbose));
            HB::Defined { locals, body }
        }
        TB::Macro => unreachable!("ICE macros filtered above"),
    };
    sp(loc, b_)
}

fn function_body_defined(
    context: &mut Context,
    signature: &H::FunctionSignature,
    loc: Loc,
    seq: VecDeque<T::SequenceItem>,
) -> (UniqueMap<H::Var, (Mutability, H::SingleType)>, Block) {
    context.signature = Some(signature.clone());
    let (mut body, final_value) = { body(context, Some(&signature.return_type), loc, seq) };
    if let Some(ret_exp) = final_value {
        let ret_loc = ret_exp.exp.loc;
        let ret_command = H::Command_::Return {
            from_user: false,
            exp: ret_exp,
        };
        body.push_back(make_command(ret_loc, ret_command));
    }

    let locals = context.extract_function_locals();
    context.exit_function();
    (locals, body)
}

fn visibility(evisibility: E::Visibility) -> H::Visibility {
    match evisibility {
        E::Visibility::Internal => H::Visibility::Internal,
        E::Visibility::Friend(loc) => H::Visibility::Friend(loc),
        // We added any friends we needed during typing, so we convert this over.
        E::Visibility::Package(loc) => H::Visibility::Friend(loc),
        E::Visibility::Public(loc) => H::Visibility::Public(loc),
    }
}

//**************************************************************************************************
// Constants
//**************************************************************************************************

fn constant(context: &mut Context, _name: ConstantName, cdef: T::Constant) -> H::Constant {
    let T::Constant {
        doc: _,
        warning_filter,
        index,
        attributes,
        loc,
        signature: tsignature,
        value: tvalue,
    } = cdef;
    context.push_warning_filter_scope(warning_filter);
    let signature = base_type(context, tsignature);
    let eloc = tvalue.exp.loc;
    let tseq = {
        let mut v = VecDeque::new();
        v.push_back(sp(eloc, T::SequenceItem_::Seq(Box::new(tvalue))));
        v
    };
    let function_signature = H::FunctionSignature {
        type_parameters: vec![],
        parameters: vec![],
        return_type: H::Type_::base(signature.clone()),
    };
    let (locals, body) = function_body_defined(context, &function_signature, loc, tseq);
    context.pop_warning_filter_scope();
    H::Constant {
        warning_filter,
        index,
        attributes,
        loc,
        signature,
        value: (locals, body),
    }
}

//**************************************************************************************************
// Structs
//**************************************************************************************************

fn struct_def(
    context: &mut Context,
    _name: DatatypeName,
    sdef: N::StructDefinition,
) -> H::StructDefinition {
    let N::StructDefinition {
        doc: _,
        warning_filter,
        index,
        loc: _loc,
        attributes,
        abilities,
        type_parameters,
        fields,
    } = sdef;
    context.push_warning_filter_scope(warning_filter);
    let fields = struct_fields(context, fields);
    context.pop_warning_filter_scope();
    H::StructDefinition {
        warning_filter,
        index,
        attributes,
        abilities,
        type_parameters,
        fields,
    }
}

fn struct_fields(context: &mut Context, tfields: N::StructFields) -> H::StructFields {
    let tfields_map = match tfields {
        N::StructFields::Native(loc) => return H::StructFields::Native(loc),
        N::StructFields::Defined(_, m) => m,
    };
    let mut indexed_fields = tfields_map
        .into_iter()
        .map(|(f, (idx, (_doc, t)))| (idx, (f, base_type(context, t))))
        .collect::<Vec<_>>();
    indexed_fields.sort_by(|(idx1, _), (idx2, _)| idx1.cmp(idx2));
    H::StructFields::Defined(indexed_fields.into_iter().map(|(_, f_ty)| f_ty).collect())
}

//**************************************************************************************************
// Enums
//**************************************************************************************************

fn enum_def(
    context: &mut Context,
    _name: DatatypeName,
    edef: N::EnumDefinition,
) -> H::EnumDefinition {
    let N::EnumDefinition {
        doc: _,
        warning_filter,
        index,
        loc: _loc,
        attributes,
        abilities,
        type_parameters,
        variants,
    } = edef;
    context.push_warning_filter_scope(warning_filter);
    let variants = variants.map(|_, defn| H::VariantDefinition {
        index: defn.index,
        loc: defn.loc,
        fields: variant_fields(context, defn.fields),
    });
    context.pop_warning_filter_scope();
    H::EnumDefinition {
        warning_filter,
        index,
        attributes,
        abilities,
        type_parameters,
        variants,
    }
}

fn variant_fields(context: &mut Context, tfields: N::VariantFields) -> Vec<(Field, H::BaseType)> {
    let tfields_map = match tfields {
        N::VariantFields::Empty => return vec![],
        N::VariantFields::Defined(_, m) => m,
    };
    let mut indexed_fields = tfields_map
        .into_iter()
        .map(|(f, (idx, (_doc, t)))| (idx, (f, base_type(context, t))))
        .collect::<Vec<_>>();
    indexed_fields.sort_by(|(idx1, _), (idx2, _)| idx1.cmp(idx2));
    indexed_fields.into_iter().map(|(_, f_ty)| f_ty).collect()
}

//**************************************************************************************************
// Types
//**************************************************************************************************

fn type_name(_context: &Context, sp!(loc, ntn_): N::TypeName) -> H::TypeName {
    use H::TypeName_ as HT;
    use N::TypeName_ as NT;
    let tn_ = match ntn_ {
        NT::Multiple(_) => panic!(
            "ICE type constraints failed {}:{}-{}",
            loc.file_hash(),
            loc.start(),
            loc.end()
        ),
        NT::Builtin(bt) => HT::Builtin(bt),
        NT::ModuleType(m, s) => HT::ModuleType(m, s),
    };
    sp(loc, tn_)
}

fn base_types<R: std::iter::FromIterator<H::BaseType>>(
    context: &mut Context,
    tys: impl IntoIterator<Item = N::Type>,
) -> R {
    tys.into_iter().map(|t| base_type(context, t)).collect()
}

fn base_type(context: &mut Context, sp!(loc, nb_): N::Type) -> H::BaseType {
    use H::BaseType_ as HB;
    use N::Type_ as NT;
    let b_ = match nb_ {
        NT::Var(_) => {
            context.add_diag(ice!((
                loc,
                format!(
                    "ICE type inf. var not expanded: {}",
                    debug_display_verbose!(nb_)
                )
            )));
            return error_base_type(loc);
        }
        NT::Apply(None, _, _) => {
            context.add_diag(ice!((
                loc,
                format!("ICE kind not expanded: {}", debug_display_verbose!(nb_))
            )));
            return error_base_type(loc);
        }
        NT::Apply(Some(k), n, nbs) => HB::Apply(k, type_name(context, n), base_types(context, nbs)),
        NT::Param(tp) => HB::Param(tp),
        NT::UnresolvedError => HB::UnresolvedError,
        NT::Anything => HB::Unreachable,
        NT::Ref(_, _) | NT::Unit | NT::Fun(_, _) => {
            context.add_diag(ice!((
                loc,
                format!(
                    "ICE base type constraint failed: {}",
                    debug_display_verbose!(nb_)
                )
            )));
            return error_base_type(loc);
        }
    };
    sp(loc, b_)
}

fn expected_types(context: &mut Context, loc: Loc, nss: Vec<Option<N::Type>>) -> H::Type {
    let any = || {
        sp(
            loc,
            H::SingleType_::Base(sp(loc, H::BaseType_::UnresolvedError)),
        )
    };
    let ss = nss
        .into_iter()
        .map(|sopt| sopt.map(|s| single_type(context, s)).unwrap_or_else(any))
        .collect::<Vec<_>>();
    H::Type_::from_vec(loc, ss)
}

fn single_types(context: &mut Context, ss: Vec<N::Type>) -> Vec<H::SingleType> {
    ss.into_iter().map(|s| single_type(context, s)).collect()
}

fn single_type(context: &mut Context, sp!(loc, ty_): N::Type) -> H::SingleType {
    use H::SingleType_ as HS;
    use N::Type_ as NT;
    let s_ = match ty_ {
        NT::Ref(mut_, nb) => HS::Ref(mut_, base_type(context, *nb)),
        _ => HS::Base(base_type(context, sp(loc, ty_))),
    };
    sp(loc, s_)
}

fn type_(context: &mut Context, sp!(loc, ty_): N::Type) -> H::Type {
    use H::Type_ as HT;
    use N::{TypeName_ as TN, Type_ as NT};
    let t_ = match ty_ {
        NT::Unit => HT::Unit,
        NT::Apply(None, _, _) => {
            context.add_diag(ice!((
                loc,
                format!("ICE kind not expanded: {}", debug_display_verbose!(ty_))
            )));
            return error_type(loc);
        }
        NT::Apply(Some(_), sp!(_, TN::Multiple(_)), ss) => HT::Multiple(single_types(context, ss)),
        _ => HT::Single(single_type(context, sp(loc, ty_))),
    };
    sp(loc, t_)
}

fn error_base_type(loc: Loc) -> H::BaseType {
    sp(loc, H::BaseType_::UnresolvedError)
}

fn error_type(loc: Loc) -> H::Type {
    H::Type_::base(error_base_type(loc))
}

//**************************************************************************************************
// Expression Processing
//**************************************************************************************************

macro_rules! make_block {
    () => { VecDeque::new() };
    ($($elems:expr),+) => { VecDeque::from([$($elems),*]) };
}

// -------------------------------------------------------------------------------------------------
// Tail Position
// -------------------------------------------------------------------------------------------------

fn body(
    context: &mut Context,
    expected_type: Option<&H::Type>,
    loc: Loc,
    seq: VecDeque<T::SequenceItem>,
) -> (Block, Option<H::Exp>) {
    if seq.is_empty() {
        (make_block!(), Some(unit_exp(loc)))
    } else {
        let mut block = make_block!();
        let final_exp = tail_block(context, &mut block, expected_type, seq);
        (block, final_exp)
    }
}

#[growing_stack]
fn tail(
    context: &mut Context,
    block: &mut Block,
    expected_type: Option<&H::Type>,
    e: T::Exp,
) -> Option<H::Exp> {
    if is_statement(&e) {
        let result = if is_unit_statement(&e) {
            Some(unit_exp(e.exp.loc))
        } else {
            None
        };
        statement(context, block, e);
        return result;
    }

    use H::Statement_ as S;
    use T::UnannotatedExp_ as E;
    let T::Exp {
        ty: ref in_type,
        exp: sp!(eloc, e_),
    } = e;
    let out_type = type_(context, in_type.clone());

    match e_ {
        // -----------------------------------------------------------------------------------------
        // control flow statements
        // -----------------------------------------------------------------------------------------
        E::IfElse(test, conseq, alt_opt) => {
            let cond = value(context, block, Some(&tbool(eloc)), *test);
            let mut if_block = make_block!();
            let conseq_exp = tail(context, &mut if_block, Some(&out_type), *conseq);
            let mut else_block = make_block!();
            let alt = alt_opt.unwrap_or_else(|| Box::new(typing_unit_exp(eloc)));
            let alt_exp = tail(context, &mut else_block, Some(&out_type), *alt);

            let (binders, bound_exp) = make_binders(context, eloc, out_type.clone());

            let arms_unreachable = conseq_exp.is_none() && alt_exp.is_none();

            if let Some(conseq_exp) = conseq_exp {
                bind_value_in_block(
                    context,
                    binders.clone(),
                    Some(out_type.clone()),
                    &mut if_block,
                    conseq_exp,
                );
            }
            if let Some(alt_exp) = alt_exp {
                bind_value_in_block(context, binders, Some(out_type), &mut else_block, alt_exp);
            }
            let if_else = S::IfElse {
                cond: Box::new(cond),
                if_block,
                else_block,
            };
            block.push_back(sp(eloc, if_else));
            if arms_unreachable {
                None
            } else {
                Some(maybe_freeze(
                    context,
                    block,
                    expected_type.cloned(),
                    bound_exp,
                ))
            }
        }

        E::Match(subject, arms) => {
            debug_print!(context.debug.match_translation,
                ("subject" => subject),
                ("type" => in_type),
                (lines "arms" => &arms.value)
            );
            let compiled = match_compilation::compile_match(context, in_type, *subject, arms);
            debug_print!(context.debug.match_translation, ("compiled" => compiled; verbose));
            let result = tail(context, block, expected_type, compiled);
            debug_print!(context.debug.match_variant_translation,
                         (lines "block" => block; verbose),
                         (opt "result" => &result));
            result
        }

        E::VariantMatch(subject, (_module, enum_name), arms) => {
            let subject = Box::new(value(context, block, None, *subject));

            let (binders, bound_exp) = make_binders(context, eloc, out_type.clone());

            let mut arms_unreachable = true;
            let arms = arms
                .into_iter()
                .map(|(variant, rhs)| {
                    let mut arm_block = make_block!();
                    let arm_exp = tail(context, &mut arm_block, Some(&out_type), rhs);
                    if let Some(arm_exp) = arm_exp {
                        arms_unreachable = false;
                        bind_value_in_block(
                            context,
                            binders.clone(),
                            Some(out_type.clone()),
                            &mut arm_block,
                            arm_exp,
                        );
                    }
                    (variant, arm_block)
                })
                .collect::<Vec<_>>();
            let variant_switch = S::VariantMatch {
                subject,
                enum_name,
                arms,
            };
            block.push_back(sp(eloc, variant_switch));
            let result = if arms_unreachable {
                None
            } else {
                Some(maybe_freeze(
                    context,
                    block,
                    expected_type.cloned(),
                    bound_exp,
                ))
            };
            debug_print!(context.debug.match_variant_translation,
                         (lines "block" => block; verbose),
                         (opt "result" => &result));
            result
        }

        // While loops can't yield values, so we treat them as statements with no binders.
        e_ @ E::While(_, _, _) => {
            statement(context, block, T::exp(in_type.clone(), sp(eloc, e_)));
            Some(trailing_unit_exp(eloc))
        }
        E::Loop {
            name,
            has_break: true,
            body,
        } => {
            let name = translate_block_label(name);
            let (binders, bound_exp) = make_binders(context, eloc, out_type.clone());
            let result = if binders.is_empty() {
                // need to swap the implicit unit out for a trailing unit in tail position
                trailing_unit_exp(eloc)
            } else {
                maybe_freeze(context, block, expected_type.cloned(), bound_exp)
            };
            context.enter_named_block(name, binders, out_type.clone());
            let (loop_body, has_break) = process_loop_body(context, &name, *body);
            block.push_back(sp(
                eloc,
                S::Loop {
                    name,
                    has_break,
                    block: loop_body,
                },
            ));
            context.exit_named_block(name);
            if has_break {
                Some(result)
            } else {
                None
            }
        }
        e_ @ E::Loop { .. } => {
            // A loop wthout a break has no concrete type for its binders, but since we'll never
            // find a break we won't need binders anyway. We just treat it like a statement.
            statement(context, block, T::exp(in_type.clone(), sp(eloc, e_)));
            None
        }
        E::NamedBlock(name, (_, seq)) => {
            let name = translate_block_label(name);
            let (binders, bound_exp) = make_binders(context, eloc, out_type.clone());
            let result = if binders.is_empty() {
                // need to swap the implicit unit out for a trailing unit in tail position
                trailing_unit_exp(eloc)
            } else {
                maybe_freeze(context, block, expected_type.cloned(), bound_exp)
            };
            context.enter_named_block(name, binders.clone(), out_type.clone());
            let mut body_block = make_block!();
            let final_exp = tail_block(context, &mut body_block, Some(&out_type), seq);
            if let Some(exp) = final_exp {
                bind_value_in_block(context, binders, Some(out_type), &mut body_block, exp);
            }
            block.push_back(sp(
                eloc,
                S::NamedBlock {
                    name,
                    block: body_block,
                },
            ));
            context.exit_named_block(name);
            Some(result)
        }
        E::Block((_, seq)) => tail_block(context, block, expected_type, seq),

        // -----------------------------------------------------------------------------------------
        //  statements that need to be hoisted out
        // -----------------------------------------------------------------------------------------
        E::Return(_)
        | E::Abort(_)
        | E::Give(_, _)
        | E::Continue(_)
        | E::Assign(_, _, _)
        | E::Mutate(_, _) => {
            context.add_diag(ice!((eloc, "ICE statement mishandled in HLIR lowering")));
            None
        }

        // -----------------------------------------------------------------------------------------
        //  value-like expression
        // -----------------------------------------------------------------------------------------
        e_ => {
            let e = T::Exp {
                ty: in_type.clone(),
                exp: sp(eloc, e_),
            };
            Some(value(context, block, expected_type, e))
        }
    }
}

fn tail_block(
    context: &mut Context,
    block: &mut Block,
    expected_type: Option<&H::Type>,
    mut seq: VecDeque<T::SequenceItem>,
) -> Option<H::Exp> {
    use T::SequenceItem_ as S;
    let last_exp = seq.pop_back();
    statement_block(context, block, seq);
    match last_exp {
        None => None,
        Some(sp!(_, S::Seq(last))) => tail(context, block, expected_type, *last),
        Some(sp!(loc, _)) => {
            context.add_diag(ice!((loc, "ICE statement mishandled in HLIR lowering")));
            None
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Value Position
// -------------------------------------------------------------------------------------------------

#[growing_stack]
fn value(
    context: &mut Context,
    block: &mut Block,
    expected_type: Option<&H::Type>,
    e: T::Exp,
) -> H::Exp {
    use H::{Command_ as C, Statement_ as S, UnannotatedExp_ as HE};
    use T::UnannotatedExp_ as E;

    // we pull outthese cases because it's easier to process them without destructuring `e` first.
    if is_statement(&e) {
        let result = if is_unit_statement(&e) {
            unit_exp(e.exp.loc)
        } else {
            H::exp(type_(context, e.ty.clone()), sp(e.exp.loc, HE::Unreachable))
        };
        statement(context, block, e);
        return result;
    } else if is_binop(&e) {
        let out_type = type_(context, e.ty.clone());
        let out_exp = process_binops(context, block, out_type, e);
        return maybe_freeze(context, block, expected_type.cloned(), out_exp);
    } else if is_exp_list(&e) {
        let out_type = type_(context, e.ty.clone());
        let eloc = e.exp.loc;
        let out_vec = value_list(context, block, Some(&out_type), e);
        return maybe_freeze(
            context,
            block,
            expected_type.cloned(),
            H::exp(out_type, sp(eloc, HE::Multiple(out_vec))),
        );
    }

    let T::Exp {
        ty: ref in_type,
        exp: sp!(eloc, e_),
    } = e;
    let out_type = type_(context, in_type.clone());
    let make_exp = |exp| H::exp(out_type.clone(), sp(eloc, exp));

    let preresult: H::Exp = match e_ {
        // ---------------------------------------------------------------------------------------
        // Expansion-y things
        // These could likely be discharged during expansion instead.
        //
        E::Builtin(bt, arguments) if matches!(&*bt, sp!(_, T::BuiltinFunction_::Assert(None))) => {
            use T::ExpListItem as TI;
            let [cond_item, code_item]: [TI; 2] = match arguments.exp.value {
                E::ExpList(arg_list) => arg_list.try_into().unwrap(),
                _ => {
                    context.add_diag(ice!((eloc, "ICE type checking assert failed")));
                    return error_exp(eloc);
                }
            };
            let (econd, ecode) = match (cond_item, code_item) {
                (TI::Single(econd, _), TI::Single(ecode, _)) => (econd, ecode),
                _ => {
                    context.add_diag(ice!((eloc, "ICE type checking assert failed")));
                    return error_exp(eloc);
                }
            };
            let cond_value = value(context, block, Some(&tbool(eloc)), econd);
            let code_value = value(context, block, None, ecode);
            let cond = bind_exp(context, block, cond_value);
            let code = bind_exp(context, block, code_value);
            let if_block = make_block!();
            let else_block = make_block!(make_command(eloc, C::Abort(code.exp.loc, code)));
            block.push_back(sp(
                eloc,
                S::IfElse {
                    cond: Box::new(cond),
                    if_block,
                    else_block,
                },
            ));
            unit_exp(eloc)
        }
        E::Builtin(bt, arguments)
            if matches!(&*bt, sp!(_, T::BuiltinFunction_::Assert(Some(_)))) =>
        {
            use T::ExpListItem as TI;
            let [cond_item, code_item]: [TI; 2] = match arguments.exp.value {
                E::ExpList(arg_list) => arg_list.try_into().unwrap(),
                _ => {
                    context.add_diag(ice!((eloc, "ICE type checking assert failed")));
                    return error_exp(eloc);
                }
            };
            let (econd, ecode) = match (cond_item, code_item) {
                (TI::Single(econd, _), TI::Single(ecode, _)) => (econd, ecode),
                _ => {
                    context.add_diag(ice!((eloc, "ICE type checking assert failed")));
                    return error_exp(eloc);
                }
            };
            let cond = value(context, block, Some(&tbool(eloc)), econd);
            let mut else_block = make_block!();
            let code = value(context, &mut else_block, None, ecode);
            let if_block = make_block!();
            else_block.push_back(make_command(eloc, C::Abort(code.exp.loc, code)));
            block.push_back(sp(
                eloc,
                S::IfElse {
                    cond: Box::new(cond),
                    if_block,
                    else_block,
                },
            ));
            unit_exp(eloc)
        }

        // -----------------------------------------------------------------------------------------
        // control flow statements
        // -----------------------------------------------------------------------------------------
        E::IfElse(test, conseq, alt_opt) => {
            let cond = value(context, block, Some(&tbool(eloc)), *test);
            let mut if_block = make_block!();
            let conseq_exp = value(context, &mut if_block, Some(&out_type), *conseq);
            let mut else_block = make_block!();
            let alt = alt_opt.unwrap_or_else(|| Box::new(typing_unit_exp(eloc)));
            let alt_exp = value(context, &mut else_block, Some(&out_type), *alt);
            let (binders, bound_exp) = make_binders(context, eloc, out_type.clone());

            let arms_unreachable = conseq_exp.is_unreachable() && alt_exp.is_unreachable();

            bind_value_in_block(
                context,
                binders.clone(),
                Some(out_type.clone()),
                &mut if_block,
                conseq_exp,
            );
            bind_value_in_block(
                context,
                binders,
                Some(out_type.clone()),
                &mut else_block,
                alt_exp,
            );

            let if_else = S::IfElse {
                cond: Box::new(cond),
                if_block,
                else_block,
            };
            block.push_back(sp(eloc, if_else));
            if arms_unreachable {
                make_exp(HE::Unreachable)
            } else {
                bound_exp
            }
        }

        E::Match(subject, arms) => {
            debug_print!(context.debug.match_translation,
                ("subject" => subject),
                ("type" => in_type),
                (lines "arms" => &arms.value)
            );
            let compiled = match_compilation::compile_match(context, in_type, *subject, arms);
            debug_print!(context.debug.match_translation, ("compiled" => compiled; verbose));
            let result = value(context, block, None, compiled);
            debug_print!(context.debug.match_variant_translation, ("result" => &result));
            result
        }

        E::VariantMatch(subject, (_module, enum_name), arms) => {
            let subject_out_type = type_(context, subject.ty.clone());
            let subject = Box::new(value(context, block, Some(&subject_out_type), *subject));

            let (binders, bound_exp) = make_binders(context, eloc, out_type.clone());

            let mut arms_unreachable = true;
            let arms = arms
                .into_iter()
                .map(|(variant, rhs)| {
                    let mut arm_block = make_block!();
                    let arm_exp = value(context, &mut arm_block, Some(&out_type), rhs);
                    arms_unreachable = arms_unreachable && arm_exp.is_unreachable();
                    bind_value_in_block(
                        context,
                        binders.clone(),
                        Some(out_type.clone()),
                        &mut arm_block,
                        arm_exp,
                    );
                    (variant, arm_block)
                })
                .collect::<Vec<_>>();
            let variant_switch = S::VariantMatch {
                subject,
                enum_name,
                arms,
            };
            block.push_back(sp(eloc, variant_switch));
            let result = if arms_unreachable {
                make_exp(HE::Unreachable)
            } else {
                bound_exp
            };
            debug_print!(context.debug.match_variant_translation,
                         (lines "block" => block.iter(); verbose),
                         ("result" => &result));
            result
        }

        // While loops can't yield values, so we treat them as statements with no binders.
        e_ @ E::While(_, _, _) => {
            statement(context, block, T::exp(in_type.clone(), sp(eloc, e_)));
            unit_exp(eloc)
        }
        E::Loop {
            name,
            has_break: true,
            body,
        } => {
            let name = translate_block_label(name);
            let (binders, bound_exp) = make_binders(context, eloc, out_type.clone());
            context.enter_named_block(name, binders, out_type.clone());
            let (loop_body, has_break) = process_loop_body(context, &name, *body);
            block.push_back(sp(
                eloc,
                S::Loop {
                    name,
                    has_break,
                    block: loop_body,
                },
            ));
            let result = if has_break {
                bound_exp
            } else {
                make_exp(HE::Unreachable)
            };
            context.exit_named_block(name);
            result
        }
        e_ @ E::Loop { .. } => {
            statement(context, block, T::exp(in_type.clone(), sp(eloc, e_)));
            make_exp(HE::Unreachable)
        }
        E::NamedBlock(name, (_, seq)) => {
            let name = translate_block_label(name);
            let (binders, bound_exp) = make_binders(context, eloc, out_type.clone());
            context.enter_named_block(name, binders.clone(), out_type.clone());
            let mut body_block = make_block!();
            let final_exp = value_block(context, &mut body_block, Some(&out_type), eloc, seq);
            bind_value_in_block(context, binders, Some(out_type), &mut body_block, final_exp);
            block.push_back(sp(
                eloc,
                S::NamedBlock {
                    name,
                    block: body_block,
                },
            ));
            context.exit_named_block(name);
            bound_exp
        }
        E::Block((_, seq)) => value_block(context, block, Some(&out_type), eloc, seq),

        // -----------------------------------------------------------------------------------------
        //  calls
        // -----------------------------------------------------------------------------------------
        E::ModuleCall(call) => {
            let T::ModuleCall {
                module,
                name,
                type_arguments,
                arguments,
                parameter_types,
                method_name: _,
            } = *call;
            let htys = base_types(context, type_arguments);
            let expected_type = H::Type_::from_vec(eloc, single_types(context, parameter_types));
            let arguments = value_list(context, block, Some(&expected_type), *arguments);
            let call = H::ModuleCall {
                module,
                name,
                type_arguments: htys,
                arguments,
            };
            make_exp(HE::ModuleCall(Box::new(call)))
        }
        E::Builtin(bt, args) => make_exp(builtin(context, block, eloc, *bt, args)),

        // -----------------------------------------------------------------------------------------
        // nested expressions
        // -----------------------------------------------------------------------------------------
        E::Vector(vec_loc, size, vty, args) => {
            let values = value_list(context, block, None, *args);
            make_exp(HE::Vector(
                vec_loc,
                size,
                Box::new(base_type(context, *vty)),
                values,
            ))
        }
        E::Dereference(ev) => {
            let value = value(context, block, None, *ev);
            make_exp(HE::Dereference(Box::new(value)))
        }
        E::UnaryExp(op, operand) => {
            let operand = value(context, block, None, *operand);
            make_exp(HE::UnaryExp(op, Box::new(operand)))
        }

        E::Pack(module_ident, struct_name, arg_types, fields) => {
            // all fields of a packed struct type are used
            context
                .used_fields
                .entry(struct_name.value())
                .or_default()
                .extend(fields.iter().map(|(_, name, _)| *name));

            let base_types = base_types(context, arg_types);

            let decl_fields = context.info.struct_fields(&module_ident, &struct_name);

            let mut texp_fields: Vec<(usize, Field, usize, N::Type, T::Exp)> =
                if let Some(ref field_map) = decl_fields {
                    fields
                        .into_iter()
                        .map(|(f, (exp_idx, (bt, tf)))| {
                            (*field_map.get(&f).unwrap(), f, exp_idx, bt, tf)
                        })
                        .collect()
                } else {
                    // If no field map, compiler error in typing.
                    fields
                        .into_iter()
                        .enumerate()
                        .map(|(ndx, (f, (exp_idx, (bt, tf))))| (ndx, f, exp_idx, bt, tf))
                        .collect()
                };
            texp_fields.sort_by(|(_, _, eidx1, _, _), (_, _, eidx2, _, _)| eidx1.cmp(eidx2));

            let reorder_fields = texp_fields
                .iter()
                .any(|(decl_idx, _, exp_idx, _, _)| decl_idx != exp_idx);

            let fields = if !reorder_fields {
                let mut fields = vec![];
                let field_exps = texp_fields
                    .into_iter()
                    .map(|(_, f, _, bt, te)| {
                        let bt = base_type(context, bt);
                        fields.push((f, bt.clone()));
                        let t = H::Type_::base(bt);
                        (te, Some(t))
                    })
                    .collect();
                let field_exps = value_evaluation_order(context, block, field_exps);
                assert!(
                    fields.len() == field_exps.len(),
                    "ICE exp_evaluation_order changed arity"
                );
                field_exps
                    .into_iter()
                    .zip(fields)
                    .map(|(e, (f, bt))| (f, bt, e))
                    .collect()
            } else {
                let num_fields = decl_fields.as_ref().map(|m| m.len()).unwrap_or(0);
                let mut fields = (0..num_fields).map(|_| None).collect::<Vec<_>>();
                for (decl_idx, field, _exp_idx, bt, tf) in texp_fields {
                    // Might have too many arguments, there will be an error from typing
                    if decl_idx >= fields.len() {
                        debug_assert!(context.env.has_errors());
                        break;
                    }
                    let base_ty = base_type(context, bt);
                    let t = H::Type_::base(base_ty.clone());
                    let field_expr = value(context, block, Some(&t), tf);
                    assert!(fields.get(decl_idx).unwrap().is_none());
                    let move_tmp = bind_exp(context, block, field_expr);
                    fields[decl_idx] = Some((field, base_ty, move_tmp))
                }
                // Might have too few arguments, there will be an error from typing if so
                fields
                    .into_iter()
                    .filter_map(|o| {
                        // if o is None, context should have errors
                        debug_assert!(o.is_some() || context.env.has_errors());
                        o
                    })
                    .collect()
            };
            make_exp(HE::Pack(struct_name, base_types, fields))
        }

        E::PackVariant(module_ident, enum_name, variant_name, arg_types, fields) => {
            let base_types = base_types(context, arg_types);

            let decl_fields =
                context
                    .info
                    .enum_variant_fields(&module_ident, &enum_name, &variant_name);

            let mut texp_fields: Vec<(usize, Field, usize, N::Type, T::Exp)> =
                if let Some(ref field_map) = decl_fields {
                    fields
                        .into_iter()
                        .map(|(f, (exp_idx, (bt, tf)))| {
                            (*field_map.get(&f).unwrap(), f, exp_idx, bt, tf)
                        })
                        .collect()
                } else {
                    // If no field map, compiler error in typing.
                    fields
                        .into_iter()
                        .enumerate()
                        .map(|(ndx, (f, (exp_idx, (bt, tf))))| (ndx, f, exp_idx, bt, tf))
                        .collect()
                };
            texp_fields.sort_by(|(_, _, eidx1, _, _), (_, _, eidx2, _, _)| eidx1.cmp(eidx2));

            let reorder_fields = texp_fields
                .iter()
                .any(|(decl_idx, _, exp_idx, _, _)| decl_idx != exp_idx);

            let fields = if !reorder_fields {
                let mut fields = vec![];
                let field_exps = texp_fields
                    .into_iter()
                    .map(|(_, f, _, bt, te)| {
                        let bt = base_type(context, bt);
                        fields.push((f, bt.clone()));
                        let t = H::Type_::base(bt);
                        (te, Some(t))
                    })
                    .collect();
                let field_exps = value_evaluation_order(context, block, field_exps);
                assert!(
                    fields.len() == field_exps.len(),
                    "ICE exp_evaluation_order changed arity"
                );
                field_exps
                    .into_iter()
                    .zip(fields)
                    .map(|(e, (f, bt))| (f, bt, e))
                    .collect()
            } else {
                let num_fields = decl_fields.as_ref().map(|m| m.len()).unwrap_or(0);
                let mut fields = (0..num_fields).map(|_| None).collect::<Vec<_>>();
                for (decl_idx, field, _exp_idx, bt, tf) in texp_fields {
                    // Might have too many arguments, there will be an error from typing
                    if decl_idx >= fields.len() {
                        debug_assert!(context.env.has_errors());
                        break;
                    }
                    let base_ty = base_type(context, bt);
                    let t = H::Type_::base(base_ty.clone());
                    let field_expr = value(context, block, Some(&t), tf);
                    debug_assert!(fields.get(decl_idx).unwrap().is_none());
                    let move_tmp = bind_exp(context, block, field_expr);
                    fields[decl_idx] = Some((field, base_ty, move_tmp))
                }
                // Might have too few arguments, there will be an error from typing if so
                fields
                    .into_iter()
                    .filter_map(|o| {
                        // if o is None, context should have errors
                        debug_assert!(o.is_some() || context.env.has_errors());
                        o
                    })
                    .collect()
            };
            make_exp(HE::PackVariant(enum_name, variant_name, base_types, fields))
        }

        E::Borrow(mut_, base_exp, field) => {
            let exp = value(context, block, None, *base_exp);
            if let Some(struct_name) = struct_name(&exp.ty) {
                context
                    .used_fields
                    .entry(struct_name.value())
                    .or_default()
                    .insert(field.value());
            }
            make_exp(HE::Borrow(mut_, Box::new(exp), field, None))
        }
        E::TempBorrow(mut_, base_exp) => {
            let exp = value(context, block, None, *base_exp);
            let bound_exp = bind_exp(context, block, exp);
            let tmp = match bound_exp.exp.value {
                HE::Move {
                    annotation: MoveOpAnnotation::InferredLastUsage,
                    var,
                } => var,
                _ => {
                    context.add_diag(ice!((
                        eloc,
                        format!(
                            "ICE invalid bind_exp for single value: {}",
                            debug_display!(bound_exp)
                        )
                    )));
                    return error_exp(eloc);
                }
            };
            make_exp(HE::BorrowLocal(mut_, tmp))
        }
        E::BorrowLocal(mut_, var) => make_exp(HE::BorrowLocal(mut_, translate_var(var))),
        E::Cast(base, rhs_ty) => {
            use N::BuiltinTypeName_ as BT;
            let new_base = value(context, block, None, *base);
            let bt = match rhs_ty.value.builtin_name() {
                Some(bt @ sp!(_, BT::U8))
                | Some(bt @ sp!(_, BT::U16))
                | Some(bt @ sp!(_, BT::U32))
                | Some(bt @ sp!(_, BT::U64))
                | Some(bt @ sp!(_, BT::U128))
                | Some(bt @ sp!(_, BT::U256)) => *bt,
                _ => {
                    context.add_diag(ice!((
                        eloc,
                        format!(
                            "ICE typing failed for cast: {} : {}",
                            debug_display_verbose!(new_base),
                            debug_display_verbose!(rhs_ty)
                        )
                    )));
                    return error_exp(eloc);
                }
            };
            make_exp(HE::Cast(Box::new(new_base), bt))
        }
        E::Annotate(base, rhs_ty) => {
            let annotated_type = type_(context, *rhs_ty);
            value(context, block, Some(&annotated_type), *base)
        }

        // -----------------------------------------------------------------------------------------
        // value-based expressions without subexpressions -- translate these directly
        // -----------------------------------------------------------------------------------------
        E::Unit { trailing } => {
            let new_unit = HE::Unit {
                case: if trailing {
                    H::UnitCase::Trailing
                } else {
                    H::UnitCase::FromUser
                },
            };
            make_exp(new_unit)
        }
        E::Value(ev) => make_exp(HE::Value(process_value(context, ev))),
        E::Constant(_m, c) => make_exp(HE::Constant(c)), // only private constants (for now)
        E::ErrorConstant {
            line_number_loc,
            error_constant,
        } => make_exp(HE::ErrorConstant {
            line_number_loc,
            error_constant,
        }),
        E::Move { from_user, var } => {
            let annotation = if from_user {
                MoveOpAnnotation::FromUser
            } else {
                MoveOpAnnotation::InferredNoCopy
            };
            let var = translate_var(var);
            make_exp(HE::Move { annotation, var })
        }
        E::Copy { from_user, var } => {
            let var = translate_var(var);
            make_exp(HE::Copy { from_user, var })
        }

        // -----------------------------------------------------------------------------------------
        //  matches that handled earlier
        // -----------------------------------------------------------------------------------------
        E::BinopExp(_, _, _, _)
        | E::ExpList(_)
        | E::Return(_)
        | E::Abort(_)
        | E::Give(_, _)
        | E::Continue(_)
        | E::Assign(_, _, _)
        | E::Mutate(_, _) => {
            context.add_diag(ice!((eloc, "ICE statement mishandled in HLIR lowering")));
            error_exp(eloc)
        }

        // -----------------------------------------------------------------------------------------
        // odds and ends -- things we need to deal with but that don't do much
        // -----------------------------------------------------------------------------------------
        E::Use(_) => {
            context.add_diag(ice!((eloc, "ICE unexpanded use")));
            error_exp(eloc)
        }
        E::UnresolvedError => {
            assert!(context.env.has_errors() || context.env.ide_mode());
            make_exp(HE::UnresolvedError)
        }
    };
    maybe_freeze(context, block, expected_type.cloned(), preresult)
}

fn value_block(
    context: &mut Context,
    block: &mut Block,
    expected_type: Option<&H::Type>,
    seq_loc: Loc,
    mut seq: VecDeque<T::SequenceItem>,
) -> H::Exp {
    use T::SequenceItem_ as S;
    let last_exp = seq.pop_back();
    statement_block(context, block, seq);
    match last_exp {
        Some(sp!(_, S::Seq(last))) => value(context, block, expected_type, *last),
        Some(sp!(loc, _)) => {
            context.add_diag(ice!((loc, "ICE last sequence item should be an exp")));
            error_exp(loc)
        }
        None => {
            context.add_diag(ice!((seq_loc, "ICE empty sequence in value position")));
            error_exp(seq_loc)
        }
    }
}

fn value_list(
    context: &mut Context,
    result: &mut Block,
    ty: Option<&H::Type>,
    e: T::Exp,
) -> Vec<H::Exp> {
    use T::UnannotatedExp_ as TE;
    // The main difference is that less-optimized version does conversion and binding, then
    // freezing; the optimized will inline freezing when possible to avoid some bndings.
    if context
        .env
        .supports_feature(context.current_package, FeatureGate::Move2024Optimizations)
    {
        value_list_opt(context, result, ty, e)
    } else if let TE::ExpList(items) = e.exp.value {
        // clippy insisted on this if structure!
        value_list_items_to_vec(context, result, ty, e.exp.loc, items)
    } else if let TE::Unit { .. } = e.exp.value {
        vec![]
    } else {
        vec![value(context, result, ty, e)]
    }
}

fn value_list_items_to_vec(
    context: &mut Context,
    result: &mut Block,
    ty: Option<&H::Type>,
    loc: Loc,
    items: Vec<T::ExpListItem>,
) -> Vec<H::Exp> {
    use H::{Type_ as HT, UnannotatedExp_ as HE};
    assert!(!items.is_empty());
    let mut tys = vec![];
    let mut tes = vec![];

    for item in items.into_iter() {
        match item {
            T::ExpListItem::Single(te, ts) => {
                let t = single_type(context, *ts);
                tys.push(t.clone());
                tes.push((te, Some(sp(t.loc, HT::Single(t)))));
            }
            T::ExpListItem::Splat(_, _, _) => panic!("ICE spalt is unsupported."),
        }
    }

    let es = value_evaluation_order(context, result, tes);
    assert!(
        es.len() == tys.len(),
        "ICE exp_evaluation_order changed arity"
    );

    // Because we previously froze subpoints of ExpLists as its own binding expression for that
    // ExpList, we need to process this possible vector the same way.

    if let Some(expected_ty @ sp!(tloc, HT::Multiple(etys))) = ty {
        // We have to check that the arity of the expected type matches because some ill-typed
        // programs flow through this code. In those cases, the error has already been reported and
        // we bail.
        if etys.len() == tys.len() {
            let current_ty = sp(*tloc, HT::Multiple(tys));
            match needs_freeze(context, &current_ty, expected_ty) {
                Freeze::NotNeeded => es,
                Freeze::Point => unreachable!(),
                Freeze::Sub(_) => {
                    let current_exp = H::Exp {
                        ty: current_ty,
                        exp: sp(loc, HE::Multiple(es)),
                    };
                    let (mut freeze_block, frozen) = freeze(context, expected_ty, current_exp);
                    result.append(&mut freeze_block);
                    match frozen.exp.value {
                        HE::Multiple(final_es) => final_es,
                        _ => unreachable!(),
                    }
                }
            }
        } else {
            es
        }
    } else {
        es
    }
}

// optimized version, which inlines freezes when possible

fn value_list_opt(
    context: &mut Context,
    block: &mut Block,
    ty: Option<&H::Type>,
    e: T::Exp,
) -> Vec<H::Exp> {
    use H::Type_ as HT;
    use T::UnannotatedExp_ as TE;
    if let TE::ExpList(items) = e.exp.value {
        assert!(!items.is_empty());
        let mut tys = vec![];
        let mut item_exprs = vec![];
        let expected_tys: Vec<_> = if let Some(sp!(tloc, HT::Multiple(ts))) = ty {
            ts.iter()
                .map(|t| Some(sp(*tloc, HT::Single(t.clone()))))
                .collect()
        } else {
            items.iter().map(|_| None).collect()
        };
        for (item, expected_ty) in items.into_iter().zip(expected_tys) {
            match item {
                T::ExpListItem::Single(te, ts) => {
                    let t = single_type(context, *ts);
                    tys.push(t);
                    item_exprs.push((te, expected_ty));
                }
                T::ExpListItem::Splat(_, _, _) => panic!("ICE splat is unsupported."),
            }
        }
        let exprs = value_evaluation_order(context, block, item_exprs);
        assert!(
            exprs.len() == tys.len(),
            "ICE value_evaluation_order changed arity"
        );
        exprs
    } else if let TE::Unit { .. } = e.exp.value {
        vec![]
    } else {
        vec![value(context, block, ty, e)]
    }
}

fn error_exp(loc: Loc) -> H::Exp {
    H::exp(
        H::Type_::base(sp(loc, H::BaseType_::UnresolvedError)),
        sp(loc, H::UnannotatedExp_::UnresolvedError),
    )
}

// -------------------------------------------------------------------------------------------------
// Statement Position
// -------------------------------------------------------------------------------------------------

#[growing_stack]
fn statement(context: &mut Context, block: &mut Block, e: T::Exp) {
    use H::{Command_ as C, Statement_ as S};
    use T::UnannotatedExp_ as E;

    let T::Exp {
        ty,
        exp: sp!(eloc, e_),
    } = e;

    let make_exp = |e_| T::Exp {
        ty: ty.clone(),
        exp: sp(eloc, e_),
    };
    match e_ {
        // -----------------------------------------------------------------------------------------
        // control flow statements
        // -----------------------------------------------------------------------------------------
        E::IfElse(test, conseq, alt_opt) => {
            let cond = value(context, block, Some(&tbool(eloc)), *test);
            let mut if_block = make_block!();
            statement(context, &mut if_block, *conseq);
            let mut else_block = make_block!();
            let alt = alt_opt.unwrap_or_else(|| Box::new(typing_unit_exp(eloc)));
            statement(context, &mut else_block, *alt);
            block.push_back(sp(
                eloc,
                S::IfElse {
                    cond: Box::new(cond),
                    if_block,
                    else_block,
                },
            ));
        }
        E::Match(subject, arms) => {
            debug_print!(context.debug.match_translation,
                ("subject" => subject),
                (lines "arms" => &arms.value)
            );
            let subject_type = subject.ty.clone();
            let compiled = match_compilation::compile_match(context, &subject_type, *subject, arms);
            debug_print!(context.debug.match_translation, ("compiled" => compiled; verbose));
            statement(context, block, compiled);
            debug_print!(context.debug.match_variant_translation, (lines "block" => block));
        }
        E::VariantMatch(subject, (_module, enum_name), arms) => {
            let subject = Box::new(value(context, block, None, *subject));
            let arms = arms
                .into_iter()
                .map(|(variant, rhs)| {
                    let mut arm_block = make_block!();
                    statement(context, &mut arm_block, rhs);
                    (variant, arm_block)
                })
                .collect::<Vec<_>>();
            let variant_switch = S::VariantMatch {
                subject,
                enum_name,
                arms,
            };
            block.push_back(sp(eloc, variant_switch));
            debug_print!(context.debug.match_variant_translation,
                         (lines "block" => block; verbose));
        }
        E::While(name, test, body) => {
            let mut cond_block = make_block!();
            let cond_exp = value(context, &mut cond_block, Some(&tbool(eloc)), *test);
            let cond = (cond_block, Box::new(cond_exp));
            let name = translate_block_label(name);
            // While loops can still use break and continue so we build them dummy binders.
            context.enter_named_block(name, vec![], tunit(eloc));
            let mut body_block = make_block!();
            statement(context, &mut body_block, *body);
            block.push_back(sp(
                eloc,
                S::While {
                    cond,
                    name,
                    block: body_block,
                },
            ));
            context.exit_named_block(name);
        }
        E::Loop { name, body, .. } => {
            let name = translate_block_label(name);
            let out_type = type_(context, ty.clone());
            let (binders, bound_exp) = make_binders(context, eloc, out_type.clone());
            context.enter_named_block(name, binders, out_type);
            let (loop_body, has_break) = process_loop_body(context, &name, *body);
            block.push_back(sp(
                eloc,
                S::Loop {
                    name,
                    has_break,
                    block: loop_body,
                },
            ));
            if has_break {
                make_ignore_and_pop(block, bound_exp);
            }
            context.exit_named_block(name);
        }
        E::Block((_, seq)) => statement_block(context, block, seq),
        E::Return(rhs) => {
            let expected_type = context.signature.as_ref().map(|s| s.return_type.clone());
            let exp = value(context, block, expected_type.as_ref(), *rhs);
            let ret_command = C::Return {
                from_user: true,
                exp,
            };
            block.push_back(make_command(eloc, ret_command));
        }
        E::Abort(rhs) => {
            let exp = value(context, block, None, *rhs);
            block.push_back(make_command(eloc, C::Abort(exp.exp.loc, exp)));
        }
        E::Give(name, rhs) => {
            let out_name = translate_block_label(name);
            let bind_ty = context.lookup_named_block_type(&out_name);
            let rhs = value(context, block, bind_ty.as_ref(), *rhs);
            let binders = context.lookup_named_block_binders(&out_name);
            if binders.is_empty() {
                make_ignore_and_pop(block, rhs);
            } else {
                bind_value_in_block(context, binders, bind_ty, block, rhs);
            }
            block.push_back(make_command(eloc, C::Break(out_name)));
        }
        E::Continue(name) => {
            let out_name = translate_block_label(name);
            block.push_back(make_command(eloc, C::Continue(out_name)));
        }

        // -----------------------------------------------------------------------------------------
        //  statements with effects
        // -----------------------------------------------------------------------------------------
        E::Assign(assigns, lvalue_ty, rhs) => {
            let expected_type = expected_types(context, eloc, lvalue_ty);
            let exp = value(context, block, Some(&expected_type), *rhs);
            make_assignments(context, block, eloc, H::AssignCase::Update, assigns, exp);
        }

        E::Mutate(lhs_in, rhs_in) => {
            // evaluate RHS first
            let rhs = value(context, block, None, *rhs_in);
            let lhs = value(context, block, None, *lhs_in);
            block.push_back(make_command(eloc, C::Mutate(Box::new(lhs), Box::new(rhs))));
        }

        // calls might be for effect
        e_ @ E::ModuleCall(_) | e_ @ E::Builtin(_, _) => {
            value_statement(context, block, make_exp(e_));
        }

        // -----------------------------------------------------------------------------------------
        // valued expressions -- when these occur in statement position need their children
        // unravelled to find any embedded, effectful operations. We unravel those and discard the
        // results. These cases could be synthesized as ignore_and_pop but we avoid them altogether
        // -----------------------------------------------------------------------------------------

        // FIXME(cgswords): we can't optimize because almost all of these throw. We have to do the
        // "honest" work here, even though it's thrown away. Consider emitting a warning about
        // these and/or weaking guarantees in Move 2024.
        e_ @ (E::Vector(_, _, _, _)
        | E::Dereference(_)
        | E::UnaryExp(_, _)
        | E::BinopExp(_, _, _, _)
        | E::Pack(_, _, _, _)
        | E::PackVariant(_, _, _, _, _)
        | E::ExpList(_)
        | E::Borrow(_, _, _)
        | E::TempBorrow(_, _)
        | E::Cast(_, _)
        | E::Annotate(_, _)
        | E::BorrowLocal(_, _)
        | E::Constant(_, _)
        | E::ErrorConstant { .. }
        | E::Move { .. }
        | E::Copy { .. }
        | E::UnresolvedError
        | E::NamedBlock(_, _)) => value_statement(context, block, make_exp(e_)),

        E::Value(_) | E::Unit { .. } => (),

        // -----------------------------------------------------------------------------------------
        // odds and ends -- things we need to deal with but that don't do much
        // -----------------------------------------------------------------------------------------
        E::Use(_) => {
            context.add_diag(ice!((eloc, "ICE unexpanded use")));
        }
    }
}

fn statement_block(context: &mut Context, block: &mut Block, seq: VecDeque<T::SequenceItem>) {
    use T::SequenceItem_ as S;
    for sp!(sloc, seq_item) in seq.into_iter() {
        match seq_item {
            S::Seq(stmt_expr) => {
                statement(context, block, *stmt_expr);
            }
            S::Declare(bindings) => {
                declare_bind_list(context, &bindings);
            }
            S::Bind(bindings, ty, expr) => {
                let expected_tys = expected_types(context, sloc, ty);
                let rhs_exp = value(context, block, Some(&expected_tys), *expr);
                declare_bind_list(context, &bindings);
                make_assignments(context, block, sloc, H::AssignCase::Let, bindings, rhs_exp);
            }
        }
    }
}

// Treat something like a value, and add a final `ignore_and_pop` at the end to consume that value.
fn value_statement(context: &mut Context, block: &mut Block, e: T::Exp) {
    let exp = value(context, block, None, e);
    make_ignore_and_pop(block, exp);
}

// -------------------------------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------------------------------

fn make_command(loc: Loc, command: H::Command_) -> H::Statement {
    sp(loc, H::Statement_::Command(sp(loc, command)))
}

fn process_loop_body(context: &mut Context, name: &BlockLabel, body: T::Exp) -> (H::Block, bool) {
    let mut loop_block = make_block!();
    statement(context, &mut loop_block, body);
    // nonlocal control flow may have removed the break, so we recompute has_break.
    let has_break = still_has_break(name, &loop_block);
    (loop_block, has_break)
}

fn tbool(loc: Loc) -> H::Type {
    H::Type_::bool(loc)
}

fn bool_exp(loc: Loc, value: bool) -> H::Exp {
    H::exp(
        tbool(loc),
        sp(
            loc,
            H::UnannotatedExp_::Value(sp(loc, H::Value_::Bool(value))),
        ),
    )
}

fn tunit(loc: Loc) -> H::Type {
    sp(loc, H::Type_::Unit)
}

fn typing_unit_exp(loc: Loc) -> T::Exp {
    T::exp(
        sp(loc, N::Type_::Unit),
        sp(loc, T::UnannotatedExp_::Unit { trailing: false }),
    )
}

fn unit_exp(loc: Loc) -> H::Exp {
    H::exp(
        tunit(loc),
        sp(
            loc,
            H::UnannotatedExp_::Unit {
                case: H::UnitCase::Implicit,
            },
        ),
    )
}

fn trailing_unit_exp(loc: Loc) -> H::Exp {
    H::exp(
        tunit(loc),
        sp(
            loc,
            H::UnannotatedExp_::Unit {
                case: H::UnitCase::Trailing,
            },
        ),
    )
}

fn maybe_freeze(
    context: &mut Context,
    block: &mut Block,
    expected_type_opt: Option<H::Type>,
    exp: H::Exp,
) -> H::Exp {
    if exp.is_unreachable() {
        exp
    } else if let Some(expected_type) = expected_type_opt {
        let (mut stmts, frozen_exp) = freeze(context, &expected_type, exp);
        block.append(&mut stmts);
        frozen_exp
    } else {
        exp
    }
}

fn is_statement(e: &T::Exp) -> bool {
    use T::UnannotatedExp_ as E;
    matches!(
        e.exp.value,
        E::Return(_)
            | E::Abort(_)
            | E::Give(_, _)
            | E::Continue(_)
            | E::Assign(_, _, _)
            | E::Mutate(_, _)
    )
}

fn is_unit_statement(e: &T::Exp) -> bool {
    use T::UnannotatedExp_ as E;
    matches!(e.exp.value, E::Assign(_, _, _) | E::Mutate(_, _))
}

fn is_binop(e: &T::Exp) -> bool {
    use T::UnannotatedExp_ as E;
    matches!(e.exp.value, E::BinopExp(_, _, _, _))
}

fn is_exp_list(e: &T::Exp) -> bool {
    use T::UnannotatedExp_ as E;
    matches!(e.exp.value, E::ExpList(_))
}

macro_rules! hcmd {
    ($cmd:pat) => {
        S::Command(sp!(_, $cmd))
    };
}

fn still_has_break(name: &BlockLabel, block: &Block) -> bool {
    use H::{Command_ as C, Statement_ as S};

    fn has_break(name: &BlockLabel, sp!(_, stmt_): &H::Statement) -> bool {
        match stmt_ {
            S::IfElse {
                if_block,
                else_block,
                ..
            } => has_break_block(name, if_block) || has_break_block(name, else_block),
            S::While {
                name: _,
                cond: _,
                block,
            } => has_break_block(name, block),
            S::Loop {
                name: _,
                has_break: _,
                block,
            } => has_break_block(name, block),
            S::NamedBlock { name: _, block } => has_break_block(name, block),
            hcmd!(C::Break(break_name)) => break_name == name,
            S::Command(_) => false,
            S::VariantMatch {
                subject: _,
                enum_name: _,
                arms,
            } => arms
                .iter()
                .map(|(_id, arm)| arm)
                .any(|arm| has_break_block(name, arm)),
        }
    }

    fn has_break_block(name: &BlockLabel, block: &Block) -> bool {
        block.iter().any(|stmt| has_break(name, stmt))
    }

    has_break_block(name, block)
}

//**************************************************************************************************
// LValue
//**************************************************************************************************

fn declare_bind_list(context: &mut Context, sp!(_, binds): &T::LValueList) {
    binds.iter().for_each(|b| declare_bind(context, b))
}

fn declare_bind(context: &mut Context, sp!(_, bind_): &T::LValue) {
    use T::LValue_ as L;
    match bind_ {
        L::Ignore => (),
        L::Var {
            var: v, ty, mut_, ..
        } => {
            let st = single_type(context, *ty.clone());
            context.bind_local(mut_.unwrap(), *v, st)
        }
        L::Unpack(_, _, _, fields) | L::BorrowUnpack(_, _, _, _, fields) => fields
            .iter()
            .for_each(|(_, _, (_, (_, b)))| declare_bind(context, b)),
        L::UnpackVariant(_, _, _, _, fields) | L::BorrowUnpackVariant(_, _, _, _, _, fields) => {
            fields
                .iter()
                .for_each(|(_, _, (_, (_, b)))| declare_bind(context, b))
        }
    }
}

fn make_assignments(
    context: &mut Context,
    result: &mut Block,
    loc: Loc,
    case: H::AssignCase,
    sp!(_, assigns): T::LValueList,
    rvalue: H::Exp,
) {
    use H::{Command_ as C, Statement_ as S};
    let mut lvalues = vec![];
    let mut after = Block::new();
    for (idx, a) in assigns.into_iter().enumerate() {
        let a_ty = rvalue.ty.value.type_at_index(idx);
        let (ls, mut af) = assign(context, case, a, a_ty);

        lvalues.push(ls);
        after.append(&mut af);
    }
    result.push_back(sp(
        loc,
        S::Command(sp(loc, C::Assign(case, lvalues, rvalue))),
    ));
    result.append(&mut after);
}

fn assign(
    context: &mut Context,
    case: H::AssignCase,
    sp!(loc, ta_): T::LValue,
    rvalue_ty: &H::SingleType,
) -> (H::LValue, Block) {
    use H::{LValue_ as L, UnannotatedExp_ as E};
    use T::LValue_ as A;
    let mut after = Block::new();
    let l_ = match ta_ {
        A::Ignore => L::Ignore,
        A::Var {
            var: v,
            ty: st,
            unused_binding,
            ..
        } => L::Var {
            var: translate_var(v),
            ty: Box::new(single_type(context, *st)),
            unused_assignment: unused_binding,
        },
        A::Unpack(m, s, tbs, tfields) => {
            // all fields of an unpacked struct type are used
            context
                .used_fields
                .entry(s.value())
                .or_default()
                .extend(tfields.iter().map(|(_, s, _)| *s));

            let bs = base_types(context, tbs);

            let mut fields = vec![];
            for (decl_idx, f, bt, tfa) in assign_struct_fields(context, &m, &s, tfields) {
                assert!(fields.len() == decl_idx);
                let st = &H::SingleType_::base(bt);
                let (fa, mut fafter) = assign(context, case, tfa, st);
                after.append(&mut fafter);
                fields.push((f, fa))
            }
            L::Unpack(s, bs, fields)
        }
        A::BorrowUnpack(mut_, m, s, _tss, tfields) => {
            // all fields of an unpacked struct type are used
            context
                .used_fields
                .entry(s.value())
                .or_default()
                .extend(tfields.iter().map(|(_, s, _)| *s));

            let unused_assignment = tfields.is_empty();

            let tmp = context.new_temp(loc, rvalue_ty.clone());
            let copy_tmp = || {
                let copy_tmp_ = E::Copy {
                    from_user: false,
                    var: tmp,
                };
                H::exp(H::Type_::single(rvalue_ty.clone()), sp(loc, copy_tmp_))
            };
            let from_unpack = Some(loc);
            let fields = assign_struct_fields(context, &m, &s, tfields)
                .into_iter()
                .enumerate();
            for (idx, (decl_idx, f, bt, tfa)) in fields {
                assert!(idx == decl_idx);
                let floc = tfa.loc;
                let borrow_ = E::Borrow(mut_, Box::new(copy_tmp()), f, from_unpack);
                let borrow_ty = H::Type_::single(sp(floc, H::SingleType_::Ref(mut_, bt)));
                let borrow = H::exp(borrow_ty, sp(floc, borrow_));
                make_assignments(context, &mut after, floc, case, sp(floc, vec![tfa]), borrow);
            }
            L::Var {
                var: tmp,
                ty: Box::new(rvalue_ty.clone()),
                unused_assignment,
            }
        }
        A::UnpackVariant(m, e, v, tbs, tfields) => {
            let bs = base_types(context, tbs);

            let mut fields = vec![];
            for (decl_idx, f, bt, tfa) in assign_variant_fields(context, &m, &e, &v, tfields) {
                assert!(fields.len() == decl_idx);
                let st = &H::SingleType_::base(bt);
                let (fa, mut fafter) = assign(context, case, tfa, st);
                after.append(&mut fafter);
                fields.push((f, fa))
            }
            L::UnpackVariant(e, v, UnpackType::ByValue, loc, bs, fields)
        }
        A::BorrowUnpackVariant(mut_, m, e, v, tbs, tfields) => {
            let bs = base_types(context, tbs);

            let unpack = if mut_ {
                UnpackType::ByMutRef
            } else {
                UnpackType::ByImmRef
            };

            let mut fields = vec![];
            for (decl_idx, f, bt, tfa) in assign_variant_fields(context, &m, &e, &v, tfields) {
                assert!(fields.len() == decl_idx);
                let borrow_ty = sp(tfa.loc, H::SingleType_::Ref(mut_, bt));
                let (fa, mut fafter) = assign(context, case, tfa, &borrow_ty);
                after.append(&mut fafter);
                fields.push((f, fa))
            }
            L::UnpackVariant(e, v, unpack, loc, bs, fields)
        }
    };
    (sp(loc, l_), after)
}

fn assign_struct_fields(
    context: &mut Context,
    m: &ModuleIdent,
    s: &DatatypeName,
    tfields: Fields<(N::Type, T::LValue)>,
) -> Vec<(usize, Field, H::BaseType, T::LValue)> {
    let decl_fields = context.info.struct_fields(m, s);
    let mut tfields_vec: Vec<_> = match decl_fields {
        Some(m) => tfields
            .into_iter()
            .map(|(f, (_idx, (tbt, tfa)))| {
                let field = *m.get(&f).unwrap();
                let base_ty = base_type(context, tbt);
                (field, f, base_ty, tfa)
            })
            .collect(),
        None => tfields
            .into_iter()
            .enumerate()
            .map(|(ndx, (f, (_idx, (tbt, tfa))))| {
                let base_ty = base_type(context, tbt);
                (ndx, f, base_ty, tfa)
            })
            .collect(),
    };
    tfields_vec.sort_by(|(idx1, _, _, _), (idx2, _, _, _)| idx1.cmp(idx2));
    tfields_vec
}

fn assign_variant_fields(
    context: &mut Context,
    m: &ModuleIdent,
    e: &DatatypeName,
    v: &VariantName,
    tfields: Fields<(N::Type, T::LValue)>,
) -> Vec<(usize, Field, H::BaseType, T::LValue)> {
    let decl_fields = context.info.enum_variant_fields(m, e, v);
    let mut tfields_vec: Vec<_> = match decl_fields {
        Some(m) => tfields
            .into_iter()
            .map(|(f, (_idx, (tbt, tfa)))| {
                let field = *m.get(&f).unwrap();
                let base_ty = base_type(context, tbt);
                (field, f, base_ty, tfa)
            })
            .collect(),
        None => tfields
            .into_iter()
            .enumerate()
            .map(|(ndx, (f, (_idx, (tbt, tfa))))| {
                let base_ty = base_type(context, tbt);
                (ndx, f, base_ty, tfa)
            })
            .collect(),
    };
    tfields_vec.sort_by(|(idx1, _, _, _), (idx2, _, _, _)| idx1.cmp(idx2));
    tfields_vec
}

//**************************************************************************************************
// Commands
//**************************************************************************************************

fn make_ignore_and_pop(block: &mut Block, exp: H::Exp) {
    use H::UnannotatedExp_ as E;
    let loc = exp.exp.loc;
    if exp.is_unreachable() {
        return;
    }
    match &exp.ty.value {
        H::Type_::Unit => match exp.exp.value {
            E::Unit { .. } => (),
            E::Value(_) => (),
            _ => {
                let c = sp(loc, H::Command_::IgnoreAndPop { pop_num: 0, exp });
                block.push_back(sp(loc, H::Statement_::Command(c)));
            }
        },
        H::Type_::Single(_) => {
            let c = sp(loc, H::Command_::IgnoreAndPop { pop_num: 1, exp });
            block.push_back(sp(loc, H::Statement_::Command(c)));
        }
        H::Type_::Multiple(tys) => {
            let c = sp(
                loc,
                H::Command_::IgnoreAndPop {
                    pop_num: tys.len(),
                    exp,
                },
            );
            block.push_back(sp(loc, H::Statement_::Command(c)));
        }
    };
}

//**************************************************************************************************
// Expressions
//**************************************************************************************************

fn struct_name(sp!(_, t): &H::Type) -> Option<DatatypeName> {
    let H::Type_::Single(st) = t else {
        return None;
    };
    let bt = match &st.value {
        H::SingleType_::Base(bt) => bt,
        H::SingleType_::Ref(_, bt) => bt,
    };
    let H::BaseType_::Apply(_, tname, _) = &bt.value else {
        return None;
    };
    if let H::TypeName_::ModuleType(_, struct_name) = tname.value {
        return Some(struct_name);
    }
    None
}

fn value_evaluation_order(
    context: &mut Context,
    block: &mut Block,
    input_exps: Vec<(T::Exp, Option<H::Type>)>,
) -> Vec<H::Exp> {
    let mut needs_binding = false;
    let mut statements = vec![];
    let mut values = vec![];
    for (exp, expected_type) in input_exps.into_iter().rev() {
        let mut new_stmts = make_block!();
        debug_print!(context.debug.eval_order, ("has new statements" => !new_stmts.is_empty(); fmt));
        let exp = value(context, &mut new_stmts, expected_type.as_ref(), exp);
        let exp = if needs_binding {
            bind_exp(context, &mut new_stmts, exp)
        } else {
            exp
        };
        values.push(exp);
        // If evaluating this expression introduces statements, all previous exps need to be bound
        // to preserve left-to-right evaluation order
        let adds_to_result = !new_stmts.is_empty();
        needs_binding = needs_binding || adds_to_result;
        statements.push(new_stmts);
    }
    block.append(&mut statements.into_iter().rev().flatten().collect());
    values.into_iter().rev().collect()
}

fn bind_exp(context: &mut Context, stmts: &mut Block, e: H::Exp) -> H::Exp {
    let loc = e.exp.loc;
    let ty = e.ty.clone();
    let (binders, var_exp) = make_binders(context, loc, ty.clone());
    bind_value_in_block(context, binders, Some(ty), stmts, e);
    var_exp
}

// Takes binder(s), a block, and a value. If the value is defined, adds an assignment to the end
// of the block to assign the binders to that value.
// Returns the block and a flag indicating if that operation happened.
fn bind_value_in_block(
    context: &mut Context,
    binders: Vec<H::LValue>,
    binders_type: Option<H::Type>,
    stmts: &mut Block,
    value_exp: H::Exp,
) {
    use H::{Command_ as C, Statement_ as S};
    let mut binders_valid = true;
    for sp!(loc, lvalue) in &binders {
        match lvalue {
            H::LValue_::Var { .. } => (),
            lv => {
                context.add_diag(ice!((
                    *loc,
                    format!(
                        "ICE tried bind_value for non-var lvalue {}",
                        debug_display!(lv)
                    )
                )));
                binders_valid = false;
            }
        }
    }
    if !binders_valid {
        return;
    }
    let rhs_exp = maybe_freeze(context, stmts, binders_type, value_exp);
    let loc = rhs_exp.exp.loc;
    stmts.push_back(sp(
        loc,
        S::Command(sp(loc, C::Assign(H::AssignCase::Let, binders, rhs_exp))),
    ));
}

fn make_binders(context: &mut Context, loc: Loc, ty: H::Type) -> (Vec<H::LValue>, H::Exp) {
    use H::Type_ as T;
    use H::UnannotatedExp_ as E;
    match ty.value {
        T::Unit => (
            vec![],
            H::exp(
                tunit(loc),
                sp(
                    loc,
                    E::Unit {
                        case: H::UnitCase::Implicit,
                    },
                ),
            ),
        ),
        T::Single(single_type) => {
            let (binder, var_exp) = make_temp(context, loc, single_type);
            (vec![binder], var_exp)
        }
        T::Multiple(types) => {
            let (binders, vars) = types
                .iter()
                .map(|single_type| make_temp(context, loc, single_type.clone()))
                .unzip();
            (
                binders,
                H::exp(
                    sp(loc, T::Multiple(types)),
                    sp(loc, H::UnannotatedExp_::Multiple(vars)),
                ),
            )
        }
    }
}

fn make_temp(context: &mut Context, loc: Loc, sp!(_, ty): H::SingleType) -> (H::LValue, H::Exp) {
    let binder = context.new_temp(loc, sp(loc, ty.clone()));
    let lvalue_ = H::LValue_::Var {
        var: binder,
        ty: Box::new(sp(loc, ty.clone())),
        unused_assignment: false,
    };
    let lvalue = sp(loc, lvalue_);
    let uexp = sp(
        loc,
        H::UnannotatedExp_::Move {
            annotation: MoveOpAnnotation::InferredLastUsage,
            var: binder,
        },
    );
    (lvalue, H::exp(H::Type_::single(sp(loc, ty)), uexp))
}

fn builtin(
    context: &mut Context,
    block: &mut Block,
    _eloc: Loc,
    sp!(_, tb_): T::BuiltinFunction,
    targ: Box<T::Exp>,
) -> H::UnannotatedExp_ {
    use H::UnannotatedExp_ as E;
    use T::BuiltinFunction_ as TB;

    match tb_ {
        TB::Freeze(_bt) => {
            let args = value(context, block, None, *targ);
            E::Freeze(Box::new(args))
        }
        TB::Assert(_) => unreachable!(),
    }
}

fn process_value(context: &mut Context, sp!(loc, ev_): E::Value) -> H::Value {
    use E::Value_ as EV;
    use H::Value_ as HV;
    let v_ = match ev_ {
        EV::InferredNum(_) => {
            context.add_diag(ice!((loc, "ICE not expanded to value")));
            HV::U64(0)
        }
        EV::Address(a) => HV::Address(a.into_addr_bytes()),
        EV::U8(u) => HV::U8(u),
        EV::U16(u) => HV::U16(u),
        EV::U32(u) => HV::U32(u),
        EV::U64(u) => HV::U64(u),
        EV::U128(u) => HV::U128(u),
        EV::U256(u) => HV::U256(u),
        EV::Bool(u) => HV::Bool(u),
        EV::Bytearray(bytes) => HV::Vector(
            Box::new(H::BaseType_::u8(loc)),
            bytes.into_iter().map(|b| sp(loc, HV::U8(b))).collect(),
        ),
    };
    sp(loc, v_)
}

#[derive(Debug)]
enum BinopEntry {
    Op {
        exp_loc: Loc,
        lhs: Box<BinopEntry>,
        op: BinOp,
        op_type: Box<N::Type>,
        rhs: Box<BinopEntry>,
    },
    ShortCircuitAnd {
        loc: Loc,
        tests: Vec<BinopEntry>,
        last: Box<BinopEntry>,
    },
    ShortCircuitOr {
        loc: Loc,
        tests: Vec<BinopEntry>,
        last: Box<BinopEntry>,
    },
    Exp {
        exp: T::Exp,
    },
}

#[allow(dead_code)]
fn print_entry(entry: &BinopEntry, indent: usize) {
    match entry {
        BinopEntry::Op { lhs, op, rhs, .. } => {
            println!("{:indent$} op {op}", " ");
            print_entry(lhs, indent + 2);
            print_entry(rhs, indent + 2);
        }
        BinopEntry::ShortCircuitAnd { tests, last, .. } => {
            println!("{:indent$} '&&' op group", " ");
            for entry in tests {
                print_entry(entry, indent + 2);
            }
            print_entry(last, indent + 2);
        }
        BinopEntry::ShortCircuitOr { tests, last, .. } => {
            println!("{:indent$} '||' op group", " ");
            for entry in tests {
                print_entry(entry, indent + 2);
            }
            print_entry(last, indent + 2);
        }
        BinopEntry::Exp { .. } => {
            println!("{:indent$} value", " ");
        }
    }
}

#[growing_stack]
fn group_boolean_binops(e: T::Exp) -> BinopEntry {
    use BinopEntry as BE;
    use T::UnannotatedExp_ as TE;
    let exp_loc = e.exp.loc;
    let _exp_type = e.ty.clone();
    match e.exp.value {
        TE::BinopExp(lhs, op, op_type, rhs) => {
            let lhs = group_boolean_binops(*lhs);
            let rhs = group_boolean_binops(*rhs);
            match &op.value {
                BinOp_::And => {
                    let mut new_tests = match lhs {
                        BE::ShortCircuitAnd {
                            loc: _,
                            mut tests,
                            last,
                        } => {
                            tests.push(*last);
                            tests
                        }
                        other => vec![other],
                    };
                    let last = match rhs {
                        BE::ShortCircuitAnd {
                            loc: _,
                            tests,
                            last,
                        } => {
                            new_tests.extend(tests);
                            last
                        }
                        other => Box::new(other),
                    };
                    BE::ShortCircuitAnd {
                        loc: exp_loc,
                        tests: new_tests,
                        last,
                    }
                }
                BinOp_::Or => {
                    let mut new_tests = match lhs {
                        BE::ShortCircuitOr {
                            loc: _,
                            mut tests,
                            last,
                        } => {
                            tests.push(*last);
                            tests
                        }
                        other => vec![other],
                    };
                    let last = match rhs {
                        BE::ShortCircuitOr {
                            loc: _,
                            tests,
                            last,
                        } => {
                            new_tests.extend(tests);
                            last
                        }
                        other => Box::new(other),
                    };
                    BE::ShortCircuitOr {
                        loc: exp_loc,
                        tests: new_tests,
                        last,
                    }
                }
                _ => {
                    let lhs = Box::new(lhs);
                    let rhs = Box::new(rhs);
                    BE::Op {
                        exp_loc,
                        lhs,
                        op,
                        op_type,
                        rhs,
                    }
                }
            }
        }
        _ => BE::Exp { exp: e },
    }
}

fn process_binops(
    context: &mut Context,
    input_block: &mut Block,
    result_type: H::Type,
    e: T::Exp,
) -> H::Exp {
    let entry = group_boolean_binops(e.clone());
    // print_entry(&entry, 0);

    #[growing_stack]
    fn build_binop(
        context: &mut Context,
        input_block: &mut Block,
        result_type: H::Type,
        e: BinopEntry,
    ) -> H::Exp {
        match e {
            BinopEntry::Op {
                exp_loc,
                lhs,
                op,
                op_type,
                rhs,
            } => {
                let op_type = freeze_ty(type_(context, *op_type));
                let mut lhs_block = make_block!();
                let mut lhs_exp = build_binop(context, &mut lhs_block, op_type.clone(), *lhs);
                let mut rhs_block = make_block!();
                let rhs_exp = build_binop(context, &mut rhs_block, op_type, *rhs);
                if !rhs_block.is_empty() {
                    lhs_exp = bind_exp(context, &mut lhs_block, lhs_exp);
                }
                input_block.extend(lhs_block);
                input_block.extend(rhs_block);
                H::exp(result_type, sp(exp_loc, make_binop(lhs_exp, op, rhs_exp)))
            }
            BinopEntry::ShortCircuitAnd { loc, tests, last } => {
                let bool_ty = tbool(loc);
                let (binders, bound_exp) = make_binders(context, loc, bool_ty.clone());

                let mut cur_block = make_block!();
                let out_exp = build_binop(context, &mut cur_block, bool_ty.clone(), *last);
                bind_value_in_block(
                    context,
                    binders.clone(),
                    Some(bool_ty.clone()),
                    &mut cur_block,
                    out_exp,
                );

                for entry in tests.into_iter().rev() {
                    let if_block = std::mem::take(&mut cur_block);
                    let cond =
                        Box::new(build_binop(context, &mut cur_block, bool_ty.clone(), entry));
                    let mut else_block = make_block!();
                    bind_value_in_block(
                        context,
                        binders.clone(),
                        Some(bool_ty.clone()),
                        &mut else_block,
                        bool_exp(loc, false),
                    );
                    let if_stmt_ = H::Statement_::IfElse {
                        cond,
                        if_block,
                        else_block,
                    };
                    let if_stmt = sp(loc, if_stmt_);
                    cur_block.push_back(if_stmt);
                }
                input_block.extend(cur_block);
                bound_exp
            }
            BinopEntry::ShortCircuitOr { loc, tests, last } => {
                let bool_ty = tbool(loc);
                let (binders, bound_exp) = make_binders(context, loc, bool_ty.clone());

                let mut cur_block = make_block!();
                let out_exp = build_binop(context, &mut cur_block, bool_ty.clone(), *last);
                bind_value_in_block(
                    context,
                    binders.clone(),
                    Some(bool_ty.clone()),
                    &mut cur_block,
                    out_exp,
                );

                for entry in tests.into_iter().rev() {
                    let else_block = std::mem::take(&mut cur_block);
                    let cond =
                        Box::new(build_binop(context, &mut cur_block, bool_ty.clone(), entry));
                    let mut if_block = make_block!();
                    bind_value_in_block(
                        context,
                        binders.clone(),
                        Some(bool_ty.clone()),
                        &mut if_block,
                        bool_exp(loc, true),
                    );
                    let if_stmt_ = H::Statement_::IfElse {
                        cond,
                        if_block,
                        else_block,
                    };
                    let if_stmt = sp(loc, if_stmt_);
                    cur_block.push_back(if_stmt);
                }
                input_block.extend(cur_block);
                bound_exp
            }
            BinopEntry::Exp { exp } => value(context, input_block, Some(&result_type), exp),
        }
    }

    build_binop(context, input_block, result_type, entry)
}

fn make_binop(lhs: H::Exp, op: BinOp, rhs: H::Exp) -> H::UnannotatedExp_ {
    H::UnannotatedExp_::BinopExp(Box::new(lhs), op, Box::new(rhs))
}

//**************************************************************************************************
// Freezing
//**************************************************************************************************

#[derive(PartialEq, Eq)]
enum Freeze {
    NotNeeded,
    Point,
    Sub(Vec<bool>),
}

fn needs_freeze(
    context: &mut Context,
    sp!(loc, actual): &H::Type,
    sp!(eloc, expected): &H::Type,
) -> Freeze {
    use H::BaseType_ as BT;
    use H::SingleType_ as ST;
    use H::Type_ as T;
    match (actual, expected) {
        (T::Unit, T::Unit) => Freeze::NotNeeded,
        (T::Single(actual_type), T::Single(expected_type)) => {
            if needs_freeze_single(actual_type, expected_type) {
                Freeze::Point
            } else {
                Freeze::NotNeeded
            }
        }
        (T::Multiple(actual_ss), T::Multiple(actual_es)) => {
            assert!(actual_ss.len() == actual_es.len());
            let points = actual_ss
                .iter()
                .zip(actual_es)
                .map(|(a, e)| needs_freeze_single(a, e))
                .collect::<Vec<_>>();
            if points.iter().any(|needs| *needs) {
                Freeze::Sub(points)
            } else {
                Freeze::NotNeeded
            }
        }
        (T::Single(sp!(_, ST::Base(sp!(_, BT::UnresolvedError)))), _)
        | (_, T::Single(sp!(_, ST::Base(sp!(_, BT::UnresolvedError)))))
        | (T::Single(sp!(_, ST::Base(sp!(_, BT::Unreachable)))), _)
        | (_, T::Single(sp!(_, ST::Base(sp!(_, BT::Unreachable))))) => Freeze::NotNeeded,
        (_actual, _expected) => {
            if !context.env.has_errors() {
                let diag = ice!(
                    (*loc, "ICE HLIR freezing went wrong"),
                    (
                        *loc,
                        format!("Actual type: {}", debug_display_verbose!(_actual))
                    ),
                    (
                        *eloc,
                        format!("Expected type: {}", debug_display_verbose!(_expected))
                    ),
                );
                context.add_diag(diag);
            }
            Freeze::NotNeeded
        }
    }
}

fn needs_freeze_single(sp!(_, actual): &H::SingleType, sp!(_, expected): &H::SingleType) -> bool {
    use H::SingleType_ as T;
    matches!((actual, expected), (T::Ref(true, _), T::Ref(false, _)))
}

fn freeze(context: &mut Context, expected_type: &H::Type, e: H::Exp) -> (Block, H::Exp) {
    use H::{Type_ as T, UnannotatedExp_ as E};

    match needs_freeze(context, &e.ty, expected_type) {
        Freeze::NotNeeded => (make_block!(), e),
        Freeze::Point => (make_block!(), freeze_point(e)),
        Freeze::Sub(points) => {
            let mut bind_stmts = make_block!();
            let bound_rhs = bind_exp(context, &mut bind_stmts, e);
            if let H::Exp {
                ty: _,
                exp: sp!(eloc, E::Multiple(exps)),
            } = bound_rhs
            {
                assert!(exps.len() == points.len());
                let exps: Vec<_> = exps
                    .into_iter()
                    .zip(points)
                    .map(|(exp, needs_freeze)| if needs_freeze { freeze_point(exp) } else { exp })
                    .collect();
                let tys = exps
                    .iter()
                    .map(|e| match &e.ty.value {
                        T::Single(s) => s.clone(),
                        _ => {
                            let msg = format!(
                                "ICE list item has Multple type: {}",
                                debug_display_verbose!(e.ty)
                            );
                            context.add_diag(ice!((e.ty.loc, msg)));
                            H::SingleType_::base(error_base_type(e.ty.loc))
                        }
                    })
                    .collect();
                (
                    bind_stmts,
                    H::exp(sp(eloc, T::Multiple(tys)), sp(eloc, E::Multiple(exps))),
                )
            } else {
                unreachable!("ICE needs_freeze failed")
            }
        }
    }
}

fn freeze_point(e: H::Exp) -> H::Exp {
    let frozen_ty = freeze_ty(e.ty.clone());
    let eloc = e.exp.loc;
    let e_ = H::UnannotatedExp_::Freeze(Box::new(e));
    H::exp(frozen_ty, sp(eloc, e_))
}

fn freeze_ty(sp!(tloc, t): H::Type) -> H::Type {
    use H::Type_ as T;
    match t {
        T::Single(s) => sp(tloc, T::Single(freeze_single(s))),
        t => sp(tloc, t),
    }
}

fn freeze_single(sp!(sloc, s): H::SingleType) -> H::SingleType {
    use H::SingleType_ as S;
    match s {
        S::Ref(true, inner) => sp(sloc, S::Ref(false, inner)),
        s => sp(sloc, s),
    }
}

//**************************************************************************************************
// Generates warnings for unused struct fields.
//**************************************************************************************************

fn gen_unused_warnings(
    context: &mut Context,
    target_kind: TargetKind,
    structs: &UniqueMap<DatatypeName, H::StructDefinition>,
) {
    if !matches!(
        target_kind,
        TargetKind::Source {
            is_root_package: true
        }
    ) {
        // generate warnings only for modules compiled in this pass rather than for all modules
        // including pre-compiled libraries for which we do not have source code available and
        // cannot be analyzed in this pass
        return;
    }
    let is_sui_mode = context.env.package_config(context.current_package).flavor == Flavor::Sui;

    for (_, sname, sdef) in structs {
        context.push_warning_filter_scope(sdef.warning_filter);

        let has_key = sdef.abilities.has_ability_(Ability_::Key);

        if let H::StructFields::Defined(fields) = &sdef.fields {
            for (f, _) in fields {
                // skip for Sui ID fields
                if is_sui_mode && has_key && f.value() == ID_FIELD_NAME {
                    continue;
                }
                if !context
                    .used_fields
                    .get(sname)
                    .is_some_and(|names| names.contains(&f.value()))
                {
                    let msg = format!("The '{}' field of the '{sname}' type is unused", f.value());
                    context.add_diag(diag!(UnusedItem::StructField, (f.loc(), msg)));
                }
            }
        }

        context.pop_warning_filter_scope();
    }
}
