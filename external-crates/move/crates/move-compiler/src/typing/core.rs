// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    debug_display, diag,
    diagnostics::{
        Diagnostic, DiagnosticReporter, Diagnostics,
        codes::{NameResolution, TypeSafety},
        warning_filters::WarningFilters,
    },
    editions::FeatureGate,
    expansion::ast::{AbilitySet, ModuleIdent, ModuleIdent_, Mutability, Visibility},
    ice,
    naming::ast::{
        self as N, BlockLabel, BuiltinTypeName_, Color, DatatypeTypeParameter, EnumDefinition,
        IndexSyntaxMethods, ResolvedUseFuns, StructDefinition, TParam, TParamID, TVar, Type, Type_,
        TypeName, TypeName_, UseFun, UseFunKind, Var,
    },
    parser::ast::{
        Ability_, ConstantName, DatatypeName, DocComment, ENTRY_MODIFIER, Field, FunctionName,
        VariantName,
    },
    shared::{
        ide::{AutocompleteMethod, IDEAnnotation, IDEInfo},
        known_attributes::{ModeAttribute, TestingAttribute},
        matching::{MatchContext, new_match_var_name},
        program_info::*,
        string_utils::{debug_print, format_oxford_list},
        unique_map::UniqueMap,
        *,
    },
    typing::deprecation_warnings::Deprecations,
};
use known_attributes::AttributePosition;
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap},
};

//**************************************************************************************************
// Context
//**************************************************************************************************

pub struct UseFunsScope<'env, 'outer> {
    color: Option<Color>,
    count: usize,
    use_funs: UseFunsScope_<'env, 'outer>,
}

pub type UsedMethods = BTreeSet<(TypeName, Name)>;

pub enum UseFunsScope_<'env, 'outer> {
    Global(&'env ResolvedUseFuns),
    Outer(&'outer ResolvedUseFuns, UsedMethods),
    Local(ResolvedUseFuns),
}
enum FoundMethod<'outer, 'local> {
    Global(&'outer N::UseFun),
    Outer(&'outer N::UseFun, &'local mut UsedMethods),
    Local(&'local mut N::UseFun),
}

pub enum Constraint {
    AbilityConstraint {
        loc: Loc,
        msg: Option<String>,
        ty: Type,
        constraints: AbilitySet,
    },
    NumericConstraint(Loc, &'static str, Type),
    BitsConstraint(Loc, &'static str, Type),
    OrderedConstraint(Loc, &'static str, Type),
    BaseTypeConstraint(Loc, String, Type),
    SingleTypeConstraint(Loc, String, Type),
}
pub type Constraints = Vec<Constraint>;
pub type TParamSubst = HashMap<TParamID, Type>;

#[derive(Debug)]
pub struct MacroCall {
    pub module: ModuleIdent,
    pub function: FunctionName,
    pub invocation: Loc,
    pub scope_color: Color,
}

#[derive(Debug)]
pub enum MacroExpansion {
    Call(Box<MacroCall>),
    // An argument to a macro, where the entire expression was substituted in
    Argument { scope_color: Color },
}

pub(super) struct TypingDebugFlags {
    #[allow(dead_code)]
    pub(super) match_counterexample: bool,
    #[allow(dead_code)]
    pub(super) autocomplete_resolution: bool,
    #[allow(dead_code)]
    pub(super) function_translation: bool,
    #[allow(dead_code)]
    pub(super) type_elaboration: bool,
}

pub struct TVarCounter {
    next: u64,
}

pub struct ModuleContext<'env> {
    pub env: &'env CompilationEnv,
    pub reporter: DiagnosticReporter<'env>,
    pub info: &'env NamingProgramInfo,
    macros: &'env UniqueMap<ModuleIdent, UniqueMap<FunctionName, N::Sequence>>,
    #[allow(dead_code)]
    pub(super) debug: TypingDebugFlags,

    use_funs: Vec<UseFunsScope<'env, /* unused */ 'static>>,
    deprecations: Deprecations,
    pub current_package: Option<Symbol>,
    pub current_module: Option<ModuleIdent>,
    /// collects all friends that should be added over the course of 'public(package)' calls
    /// structured as (defining module, new friend, location) where `new friend` is usually the
    /// context's current module. Note there may be more than one location in practice, but
    /// tracking a single one is sufficient for error reporting.
    pub new_friends: BTreeSet<(ModuleIdent, Loc)>,
    /// collects all used module members (functions and constants) but it's a superset of these in
    /// that it may contain other identifiers that do not in fact represent a function or a constant
    pub used_module_members: BTreeMap<ModuleIdent_, BTreeSet<Symbol>>,
}

pub struct Context<'env, 'outer> {
    pub outer: &'outer ModuleContext<'env>,

    pub reporter: DiagnosticReporter<'env>,
    /// collects all friends that should be added over the course of 'public(package)' calls
    /// structured as (defining module, new friend, location) where `new friend` is usually the
    /// context's current module. Note there may be more than one location in practice, but
    /// tracking a single one is sufficient for error reporting.
    pub new_friends: BTreeSet<(ModuleIdent, Loc)>,
    /// collects all used module members (functions and constants) but it's a superset of these in
    /// that it may contain other identifiers that do not in fact represent a function or a constant
    pub used_module_members: BTreeMap<ModuleIdent_, BTreeSet<Symbol>>,

    // for generating new variables during match compilation
    next_match_var_id: usize,
    use_funs: Vec<UseFunsScope<'env, 'outer>>,
    pub current_function: Option<FunctionName>,
    pub in_macro_function: bool,
    max_variable_color: RefCell<u16>,
    pub return_type: Option<Type>,
    locals: UniqueMap<Var, Type>,

    pub tvar_counter: TVarCounter,
    pub subst: Subst,
    pub constraints: Constraints,

    named_block_map: BTreeMap<BlockLabel, Type>,
    /// Current macros being expanded
    pub macro_expansion: Vec<MacroExpansion>,
    /// Stack of items from `macro_expansion` pushed/popped when entering/leaving a lambda expansion
    /// This is to prevent accidentally thinking we are in a recursive call if a macro is used
    /// inside a lambda body
    pub lambda_expansion: Vec<Vec<MacroExpansion>>,
    /// IDE Info for the current module member. We hold onto this during typing so we can elaborate
    /// it at the end.
    pub ide_info: IDEInfo,
}

pub struct ResolvedFunctionType {
    pub declared: Loc,
    pub macro_: Option<Loc>,
    pub ty_args: Vec<Type>,
    pub params: Vec<(Var, Type)>,
    pub return_: Type,
}

pub fn global_use_funs(info: &NamingProgramInfo) -> ResolvedUseFuns {
    let mut global_use_funs = BTreeMap::new();
    for (_, _, minfo) in &info.modules {
        for (tn, methods) in &minfo.use_funs {
            let public_methods = methods.ref_filter_map(|_, uf| {
                if uf.is_public.is_some() {
                    Some(uf.clone())
                } else {
                    None
                }
            });
            if public_methods.is_empty() {
                continue;
            }

            assert!(
                !global_use_funs.contains_key(tn),
                "ICE public methods should have been filtered to the defining module.
                tn: {tn}.
                prev: {}
                new: {}",
                debug_display!((tn, (global_use_funs.get(tn).unwrap()))),
                debug_display!((tn, &public_methods))
            );
            global_use_funs.insert(*tn, public_methods);
        }
    }
    global_use_funs
}

impl<'env> UseFunsScope<'env, '_> {
    pub fn global(global_use_funs: &'env ResolvedUseFuns) -> Self {
        UseFunsScope {
            color: None,
            count: 1,
            use_funs: UseFunsScope_::Global(global_use_funs),
        }
    }
}

impl UseFunsScope_<'_, '_> {
    pub fn resolved(&self) -> &ResolvedUseFuns {
        match self {
            UseFunsScope_::Global(r) => r,
            UseFunsScope_::Outer(r, _) => r,
            UseFunsScope_::Local(r) => r,
        }
    }
}

impl FoundMethod<'_, '_> {
    pub fn use_fun(&self) -> &N::UseFun {
        match self {
            FoundMethod::Global(use_fun) => use_fun,
            FoundMethod::Outer(use_fun, _) => use_fun,
            FoundMethod::Local(use_fun) => use_fun,
        }
    }

    pub fn mark_used(&mut self, tn: &TypeName, method_name: &Name) {
        match self {
            FoundMethod::Global(_) => (),
            FoundMethod::Outer(_, used) => {
                used.insert((*tn, *method_name));
            }
            FoundMethod::Local(use_fun) => {
                use_fun.used = true;
            }
        }
    }
}

macro_rules! add_use_funs_scope {
    ($self:ident, $new_scope:expr) => {{
        let N::UseFuns {
            color,
            resolved: mut new_scope,
            implicit_candidates,
        } = $new_scope;
        assert!(
            implicit_candidates.is_empty(),
            "ICE use fun candidates should have been resolved"
        );
        let cur = $self.use_funs.last_mut().unwrap();
        if new_scope.is_empty() && cur.color == Some(color) {
            cur.count += 1;
            return;
        }
        for (tn, methods) in &mut new_scope {
            for (method, use_fun) in methods.key_cloned_iter_mut() {
                if use_fun.used || !matches!(use_fun.kind, UseFunKind::Explicit) {
                    continue;
                }
                let mut same_target = false;
                let mut case = None;
                let Some(prev) = $self.find_method_impl(tn, &method, |mut prev| {
                    let prev_use_fun = prev.use_fun();
                    if use_fun.target_function == prev_use_fun.target_function {
                        case = Some(match &prev_use_fun.kind {
                            UseFunKind::UseAlias | UseFunKind::Explicit => "Duplicate",
                            UseFunKind::FunctionDeclaration => "Unnecessary",
                        });
                        same_target = true;
                        // suppress unused warning
                        prev.mark_used(tn, &method);
                    }
                }) else {
                    continue;
                };
                if same_target {
                    let case = case.unwrap();
                    let prev_loc = prev.loc;
                    let (target_m, target_f) = &use_fun.target_function;
                    let msg =
                        format!("{case} method alias '{tn}.{method}' for '{target_m}::{target_f}'");
                    $self.add_diag(diag!(
                        Declarations::DuplicateAlias,
                        (use_fun.loc, msg),
                        (prev_loc, "The same alias was previously declared here")
                    ));
                }
            }
        }
        $self.use_funs.push(UseFunsScope {
            count: 1,
            use_funs: UseFunsScope_::Local(new_scope),
            color: Some(color),
        })
    }};
}

fn pop_use_funs_scope(use_funs: &mut Vec<UseFunsScope>) -> N::UseFuns {
    let cur = use_funs.last_mut().unwrap();
    if cur.count > 1 {
        cur.count -= 1;
        return N::UseFuns::new(cur.color.unwrap_or(0));
    }
    let UseFunsScope {
        use_funs, color, ..
    } = use_funs.pop().unwrap();
    let use_funs = match use_funs {
        UseFunsScope_::Global(_) => panic!("ICE global scope should never be popped"),
        UseFunsScope_::Outer(_, _) => panic!("ICE outer scope should never be popped"),
        UseFunsScope_::Local(use_funs) => use_funs,
    };
    N::UseFuns {
        resolved: use_funs,
        color: color.unwrap_or(0),
        implicit_candidates: UniqueMap::new(),
    }
}

fn report_unused_use_funs(reporter: &mut DiagnosticReporter, use_funs: &N::UseFuns) {
    for (tn, methods) in &use_funs.resolved {
        let unused = methods.iter().filter(|(_, _, uf)| !uf.used);
        for (_, method, use_fun) in unused {
            let N::UseFun {
                doc: _,
                loc,
                kind,
                attributes: _,
                is_public: _,
                tname: _,
                target_function: _,
                used: _,
            } = use_fun;
            match kind {
                UseFunKind::Explicit => {
                    let msg = format!("Unused 'use fun' of '{tn}.{method}'. Consider removing it");
                    reporter.add_diag(diag!(UnusedItem::Alias, (*loc, msg)))
                }
                UseFunKind::UseAlias => {
                    let msg = format!("Unused 'use' of alias '{method}'. Consider removing it");
                    reporter.add_diag(diag!(UnusedItem::Alias, (*loc, msg)))
                }
                UseFunKind::FunctionDeclaration => {
                    let diag = ice!((
                        *loc,
                        "ICE fun declaration 'use' funs should never be added to 'use' funs"
                    ));
                    reporter.add_diag(diag);
                }
            }
        }
    }
}

fn use_funs_find_method<'local>(
    use_funs: &'local mut [UseFunsScope],
    tn: &TypeName,
    method_name: &Name,
    mut fmap_use_fun: impl FnMut(FoundMethod<'_, '_>),
) -> Option<&'local UseFun> {
    let cur_color = use_funs.last().unwrap().color;
    use_funs.iter_mut().rev().find_map(|scope| {
        // scope color is None for global scope, which is always in consideration
        // otherwise, the color must match the current color. In practice, we are preventing
        // macro scopes from interfering with each the scopes in which they are expanded
        if scope.color.is_some() && scope.color != cur_color {
            return None;
        }
        match &mut scope.use_funs {
            UseFunsScope_::Global(r) => {
                let method = r.get(tn)?.get(method_name)?;
                fmap_use_fun(FoundMethod::Global(method));
                Some(method)
            }
            UseFunsScope_::Outer(r, used) => {
                let method = r.get(tn)?.get(method_name)?;
                fmap_use_fun(FoundMethod::Outer(method, used));
                Some(method)
            }
            UseFunsScope_::Local(r) => {
                let method = r.get_mut(tn)?.get_mut(method_name)?;
                fmap_use_fun(FoundMethod::Local(method));
                Some(&*method)
            }
        }
    })
}

impl<'env> ModuleContext<'env> {
    pub fn new(
        env: &'env CompilationEnv,
        info: &'env NamingProgramInfo,
        global_use_funs: &'env ResolvedUseFuns,
        macros: &'env UniqueMap<ModuleIdent, UniqueMap<FunctionName, N::Sequence>>,
    ) -> Box<Self> {
        let global_use_funs = UseFunsScope::global(global_use_funs);
        let deprecations = Deprecations::new(env, info);
        let debug = TypingDebugFlags {
            match_counterexample: false,
            autocomplete_resolution: false,
            function_translation: false,
            type_elaboration: false,
        };
        let reporter = env.diagnostic_reporter_at_top_level();
        Box::new(ModuleContext {
            use_funs: vec![global_use_funs],
            current_package: None,
            current_module: None,
            info,
            macros,
            env,
            reporter,
            debug,
            new_friends: BTreeSet::new(),
            used_module_members: BTreeMap::new(),
            deprecations,
        })
    }

    pub fn add_diag(&self, diag: Diagnostic) {
        self.reporter.add_diag(diag);
    }

    pub fn add_diags(&self, diags: Diagnostics) {
        self.reporter.add_diags(diags);
    }

    pub fn extend_ide_info(&self, info: IDEInfo) {
        self.reporter.extend_ide_info(info);
    }

    pub fn add_ide_annotation(&self, loc: Loc, info: IDEAnnotation) {
        self.reporter.add_ide_annotation(loc, info);
    }

    pub fn push_warning_filter_scope(&mut self, filters: WarningFilters) {
        self.reporter.push_warning_filter_scope(filters)
    }

    pub fn pop_warning_filter_scope(&mut self) {
        self.reporter.pop_warning_filter_scope()
    }

    pub fn add_use_funs_scope(&mut self, new_scope: N::UseFuns) {
        add_use_funs_scope!(self, new_scope)
    }

    pub fn finish_use_funs_scope(
        &mut self,
        used_methods: &BTreeSet<(TypeName, Name)>,
    ) -> N::UseFuns {
        let mut use_funs = pop_use_funs_scope(&mut self.use_funs);
        for (tn, method) in used_methods {
            use_funs
                .resolved
                .get_mut(tn)
                .unwrap()
                .get_mut(method)
                .unwrap()
                .used = true;
        }
        report_unused_use_funs(&mut self.reporter, &use_funs);
        use_funs
    }

    fn find_method_impl(
        &mut self,
        tn: &TypeName,
        method: &Name,
        fmap_use_fun: impl FnMut(FoundMethod<'_, '_>),
    ) -> Option<&UseFun> {
        use_funs_find_method(&mut self.use_funs, tn, method, fmap_use_fun)
    }

    pub fn check_feature(&self, package: Option<Symbol>, feature: FeatureGate, loc: Loc) -> bool {
        self.env
            .check_feature(&self.reporter, package, feature, loc)
    }

    pub fn new_module_member<'a>(&'a self) -> Context<'env, 'a> {
        let use_funs = self
            .use_funs
            .iter()
            .map(|scope| {
                let UseFunsScope {
                    color,
                    count,
                    use_funs,
                } = scope;
                match use_funs {
                    UseFunsScope_::Global(r) => {
                        let use_funs = UseFunsScope_::Global(r);
                        UseFunsScope {
                            color: *color,
                            count: *count,
                            use_funs,
                        }
                    }
                    UseFunsScope_::Outer(_, _) => unreachable!(),
                    UseFunsScope_::Local(r) => {
                        let scope = UseFunsScope_::Outer(r, BTreeSet::new());
                        debug_assert_eq!(color, &Some(0));
                        debug_assert_eq!(count, &1);
                        UseFunsScope {
                            color: *color,
                            count: *count,
                            use_funs: scope,
                        }
                    }
                }
            })
            .collect();
        Context {
            outer: self,
            reporter: self.reporter.clone(),
            new_friends: BTreeSet::new(),
            used_module_members: BTreeMap::new(),
            next_match_var_id: 0,
            use_funs,
            current_function: None,
            in_macro_function: false,
            max_variable_color: RefCell::new(0),
            return_type: None,
            locals: UniqueMap::new(),
            tvar_counter: TVarCounter::new(),
            subst: Subst::empty(),
            constraints: Constraints::new(),
            named_block_map: BTreeMap::new(),
            macro_expansion: vec![],
            lambda_expansion: vec![],
            ide_info: IDEInfo::new(),
        }
    }

    pub fn error_type(&self, loc: Loc) -> Type {
        sp(loc, Type_::UnresolvedError)
    }

    pub fn is_current_module(&self, m: &ModuleIdent) -> bool {
        match &self.current_module {
            Some(curm) => curm == m,
            None => false,
        }
    }

    pub fn current_package(&self) -> Option<Symbol> {
        self.current_module
            .as_ref()
            .and_then(|mident| self.module_info(mident).package)
    }

    fn current_module_shares_package_and_address(&self, m: &ModuleIdent) -> bool {
        self.current_module.is_some_and(|current_mident| {
            m.value.address == current_mident.value.address
                && self.module_info(m).package == self.module_info(&current_mident).package
        })
    }

    fn current_module_is_a_friend_of(&self, m: &ModuleIdent) -> bool {
        match &self.current_module {
            None => false,
            Some(current_mident) => {
                let minfo = self.module_info(m);
                minfo.friends.contains_key(current_mident)
            }
        }
    }

    fn module_info(&self, m: &ModuleIdent) -> &ModuleInfo {
        self.info.module(m)
    }

    fn struct_definition(&self, m: &ModuleIdent, n: &DatatypeName) -> &StructDefinition {
        self.info.struct_definition(m, n)
    }

    pub fn struct_declared_abilities(&self, m: &ModuleIdent, n: &DatatypeName) -> &AbilitySet {
        self.info.struct_declared_abilities(m, n)
    }

    pub fn struct_declared_loc(&self, m: &ModuleIdent, n: &DatatypeName) -> Loc {
        self.info.struct_declared_loc(m, n)
    }

    pub fn struct_tparams(&self, m: &ModuleIdent, n: &DatatypeName) -> &Vec<DatatypeTypeParameter> {
        self.info.struct_type_parameters(m, n)
    }

    fn enum_definition(&self, m: &ModuleIdent, n: &DatatypeName) -> &EnumDefinition {
        self.info.enum_definition(m, n)
    }

    pub fn enum_declared_abilities(&self, m: &ModuleIdent, n: &DatatypeName) -> &AbilitySet {
        self.info.enum_declared_abilities(m, n)
    }

    pub fn enum_declared_loc(&self, m: &ModuleIdent, n: &DatatypeName) -> Loc {
        self.info.enum_declared_loc(m, n)
    }

    pub fn enum_tparams(&self, m: &ModuleIdent, n: &DatatypeName) -> &Vec<DatatypeTypeParameter> {
        self.info.enum_type_parameters(m, n)
    }

    pub fn datatype_kind(&self, m: &ModuleIdent, n: &DatatypeName) -> DatatypeKind {
        self.info.datatype_kind(m, n)
    }

    pub fn function_info(&self, m: &ModuleIdent, n: &FunctionName) -> &FunctionInfo {
        self.info.function_info(m, n)
    }

    pub fn macro_body(&self, m: &ModuleIdent, n: &FunctionName) -> Option<&N::Sequence> {
        self.macros.get(m)?.get(n)
    }

    pub fn constant_info(&self, m: &ModuleIdent, n: &ConstantName) -> &ConstantInfo {
        let constants = &self.module_info(m).constants;
        constants.get(n).expect("ICE should have failed in naming")
    }

    /// Find all valid fields in scope for a given `TypeName`. This is used for autocomplete.
    pub fn find_all_fields(&self, tn: &TypeName) -> Vec<(Symbol, N::Type)> {
        debug_print!(self.debug.autocomplete_resolution, (msg "fields"), ("name" => tn));
        let fields_info = match &tn.value {
            TypeName_::Multiple(_) => vec![],
            // TODO(cswords): are there any valid builtin fields?
            TypeName_::Builtin(_) => vec![],
            TypeName_::ModuleType(m, _n) if !self.is_current_module(m) => vec![],
            TypeName_::ModuleType(m, n) => match self.datatype_kind(m, n) {
                DatatypeKind::Enum => vec![],
                DatatypeKind::Struct => match &self.struct_definition(m, n).fields {
                    N::StructFields::Native(_) => vec![],
                    N::StructFields::Defined(is_positional, fields) => {
                        if *is_positional {
                            fields
                                .iter()
                                .enumerate()
                                .map(|(idx, (_, _, (_, (_, t))))| {
                                    (format!("{}", idx).into(), t.clone())
                                })
                                .collect::<Vec<_>>()
                        } else {
                            fields
                                .key_cloned_iter()
                                .map(|(k, (_, (_, t)))| (k.value(), t.clone()))
                                .collect::<Vec<_>>()
                        }
                    }
                },
            },
        };
        debug_print!(self.debug.autocomplete_resolution, (lines "fields" => &fields_info; dbg));
        fields_info
    }
}

impl<'env, 'outer> Context<'env, 'outer> {
    pub fn env(&self) -> &'outer CompilationEnv {
        self.outer.env
    }

    pub fn reporter(&self) -> &'outer DiagnosticReporter<'env> {
        &self.outer.reporter
    }

    pub fn info(&self) -> &'outer NamingProgramInfo {
        self.outer.info
    }

    pub fn macros(&self) -> &'outer UniqueMap<ModuleIdent, UniqueMap<FunctionName, N::Sequence>> {
        self.outer.macros
    }

    #[allow(dead_code)]
    pub(super) fn debug(&self) -> &'outer TypingDebugFlags {
        &self.outer.debug
    }

    pub fn deprecations(&self) -> &'outer Deprecations {
        &self.outer.deprecations
    }

    pub fn current_module(&self) -> Option<&'outer ModuleIdent> {
        self.outer.current_module.as_ref()
    }

    pub fn check_feature(&self, package: Option<Symbol>, feature: FeatureGate, loc: Loc) -> bool {
        self.outer.check_feature(package, feature, loc)
    }

    pub fn error_type(&mut self, loc: Loc) -> Type {
        self.outer.error_type(loc)
    }

    pub fn is_current_module(&self, m: &ModuleIdent) -> bool {
        self.outer.is_current_module(m)
    }

    pub fn current_package(&self) -> Option<Symbol> {
        self.outer.current_package()
    }

    pub fn current_module_shares_package_and_address(&self, m: &ModuleIdent) -> bool {
        self.outer.current_module_shares_package_and_address(m)
    }

    pub fn current_module_is_a_friend_of(&self, m: &ModuleIdent) -> bool {
        self.outer.current_module_is_a_friend_of(m)
    }

    pub fn module_info(&self, m: &ModuleIdent) -> &ModuleInfo {
        self.outer.module_info(m)
    }

    pub fn struct_definition(&self, m: &ModuleIdent, n: &DatatypeName) -> &StructDefinition {
        self.outer.struct_definition(m, n)
    }

    pub fn struct_declared_abilities(&self, m: &ModuleIdent, n: &DatatypeName) -> &AbilitySet {
        self.outer.struct_declared_abilities(m, n)
    }

    pub fn struct_declared_loc(&self, m: &ModuleIdent, n: &DatatypeName) -> Loc {
        self.outer.struct_declared_loc(m, n)
    }

    pub fn struct_tparams(&self, m: &ModuleIdent, n: &DatatypeName) -> &Vec<DatatypeTypeParameter> {
        self.outer.struct_tparams(m, n)
    }

    pub fn enum_definition(&self, m: &ModuleIdent, n: &DatatypeName) -> &EnumDefinition {
        self.outer.enum_definition(m, n)
    }

    pub fn enum_declared_abilities(&self, m: &ModuleIdent, n: &DatatypeName) -> &AbilitySet {
        self.outer.enum_declared_abilities(m, n)
    }

    pub fn enum_declared_loc(&self, m: &ModuleIdent, n: &DatatypeName) -> Loc {
        self.outer.enum_declared_loc(m, n)
    }

    pub fn enum_tparams(&self, m: &ModuleIdent, n: &DatatypeName) -> &Vec<DatatypeTypeParameter> {
        self.outer.enum_tparams(m, n)
    }

    pub fn datatype_kind(&self, m: &ModuleIdent, n: &DatatypeName) -> DatatypeKind {
        self.outer.datatype_kind(m, n)
    }

    pub fn function_info(&self, m: &ModuleIdent, n: &FunctionName) -> &FunctionInfo {
        self.outer.function_info(m, n)
    }

    pub fn macro_body(&self, m: &ModuleIdent, n: &FunctionName) -> Option<&N::Sequence> {
        self.outer.macro_body(m, n)
    }

    pub fn constant_info(&self, m: &ModuleIdent, n: &ConstantName) -> &ConstantInfo {
        self.outer.constant_info(m, n)
    }

    pub fn find_all_fields(&self, tn: &TypeName) -> Vec<(Symbol, N::Type)> {
        self.outer.find_all_fields(tn)
    }

    pub fn add_diag(&self, diag: Diagnostic) {
        self.reporter.add_diag(diag);
    }

    pub fn add_diags(&self, diags: Diagnostics) {
        self.reporter.add_diags(diags);
    }

    pub fn extend_ide_info(&self, info: IDEInfo) {
        self.reporter.extend_ide_info(info);
    }

    pub fn add_ide_annotation(&self, loc: Loc, info: IDEAnnotation) {
        self.reporter.add_ide_annotation(loc, info);
    }

    pub fn push_warning_filter_scope(&mut self, filters: WarningFilters) {
        self.reporter.push_warning_filter_scope(filters)
    }

    pub fn pop_warning_filter_scope(&mut self) {
        self.reporter.pop_warning_filter_scope()
    }

    pub fn emit_warning_if_deprecated(
        &mut self,
        mident: &ModuleIdent,
        name: Name,
        method_opt: Option<Name>,
    ) {
        let in_same_module = self.is_current_module(mident);
        if let Some(deprecation) = self.deprecations().get_deprecation(*mident, name) {
            // Don't register a warning if we are in the module that is deprecated and the actual
            // member is not deprecated.
            if deprecation.location == AttributePosition::Module && in_same_module {
                return;
            }
            let diags = deprecation.deprecation_warnings(name, method_opt);
            self.add_diags(diags);
        }
    }

    pub fn add_use_funs_scope(&mut self, new_scope: N::UseFuns) {
        add_use_funs_scope!(self, new_scope)
    }

    pub fn pop_use_funs_scope(&mut self) -> N::UseFuns {
        let popped_scope = pop_use_funs_scope(&mut self.use_funs);
        report_unused_use_funs(&mut self.reporter, &popped_scope);
        popped_scope
    }

    fn find_method_impl(
        &mut self,
        tn: &TypeName,
        method: &Name,
        fmap_use_fun: impl FnMut(FoundMethod<'_, '_>),
    ) -> Option<&UseFun> {
        use_funs_find_method(&mut self.use_funs, tn, method, fmap_use_fun)
    }

    pub fn find_method_and_mark_used(
        &mut self,
        tn: &TypeName,
        method: Name,
    ) -> Option<(ModuleIdent, FunctionName)> {
        self.find_method_impl(tn, &method, |mut use_fun| use_fun.mark_used(tn, &method))
            .map(|use_fun| use_fun.target_function)
    }

    /// true iff it is safe to expand,
    /// false with an error otherwise (e.g. a recursive expansion)
    pub fn add_macro_expansion(&mut self, m: ModuleIdent, f: FunctionName, loc: Loc) -> bool {
        let current_call_color = self.current_call_color();

        let mut prev_opt = None;
        for (idx, mexp) in self.macro_expansion.iter().enumerate().rev() {
            match mexp {
                MacroExpansion::Argument { scope_color } => {
                    // the argument has a smaller (or equal) color, meaning this lambda/arg was
                    // written in an outer scope
                    if current_call_color > *scope_color {
                        break;
                    }
                }
                MacroExpansion::Call(c) => {
                    let MacroCall {
                        module,
                        function,
                        scope_color,
                        ..
                    } = &**c;
                    // If we find a call (same module/fn) above us at a shallower expansion depth,
                    // without an interceding macro arg/lambda, we are in a macro calling itself.
                    // If it was a deeper depth, that's fine -- it must have come from elsewhere.
                    if current_call_color > *scope_color && module == &m && function == &f {
                        prev_opt = Some(idx);
                        break;
                    }
                }
            }
        }

        if let Some(idx) = prev_opt {
            let msg = format!(
                "Recursive macro expansion. '{}::{}' cannot recursively expand itself",
                m, f
            );
            let mut diag = diag!(TypeSafety::CannotExpandMacro, (loc, msg));
            let cycle = self.macro_expansion[idx..]
                .iter()
                .filter_map(|case| match case {
                    MacroExpansion::Call(c) => Some((&c.module, &c.function, &c.invocation)),
                    MacroExpansion::Argument { .. } => None,
                });
            for (prev_m, prev_f, prev_loc) in cycle {
                let msg = if prev_m == &m && prev_f == &f {
                    format!("'{}::{}' previously expanded here", prev_m, prev_f)
                } else {
                    "From this macro expansion".to_owned()
                };
                diag.add_secondary_label((*prev_loc, msg));
            }
            self.add_diag(diag);
            false
        } else {
            self.macro_expansion
                .push(MacroExpansion::Call(Box::new(MacroCall {
                    module: m,
                    function: f,
                    invocation: loc,
                    scope_color: current_call_color,
                })));
            true
        }
    }

    pub fn pop_macro_expansion(&mut self, loc: Loc, m: &ModuleIdent, f: &FunctionName) -> bool {
        let c = match self.macro_expansion.pop() {
            Some(MacroExpansion::Call(c)) => c,
            _ => {
                let diag = ice!((
                    loc,
                    "ICE macro expansion stack should have a call when leaving a macro expansion"
                ));
                self.add_diag(diag);
                return false;
            }
        };
        let MacroCall {
            module, function, ..
        } = *c;
        assert!(
            m == &module && f == &function,
            "ICE macro expansion stack should be popped in reverse order"
        );
        true
    }

    pub fn maybe_enter_macro_argument(
        &mut self,
        from_macro_argument: Option<N::MacroArgument>,
        color: Color,
    ) {
        if from_macro_argument.is_some() {
            self.macro_expansion
                .push(MacroExpansion::Argument { scope_color: color })
        }
    }

    pub fn maybe_exit_macro_argument(
        &mut self,
        loc: Loc,
        from_macro_argument: Option<N::MacroArgument>,
    ) {
        if from_macro_argument.is_some() {
            match self.macro_expansion.pop() {
                Some(MacroExpansion::Argument { .. }) => (),
                _ => {
                    let diag = ice!((
                        loc,
                        "ICE macro expansion stack should have a lambda when leaving a lambda",
                    ));
                    self.add_diag(diag);
                }
            }
        }
    }

    pub fn expanding_macros_names(&self) -> Option<String> {
        if self.macro_expansion.is_empty() {
            return None;
        }
        let names = self
            .macro_expansion
            .iter()
            .filter_map(|exp| exp.maybe_name())
            .map(|(m, f)| format!("{m}::{f}"))
            .collect::<Vec<_>>();
        Some(format_oxford_list!("and", "'{}'", names))
    }

    pub fn current_call_color(&self) -> Color {
        self.use_funs.last().unwrap().color.unwrap()
    }

    pub fn add_ability_constraint(
        &mut self,
        loc: Loc,
        msg_opt: Option<impl Into<String>>,
        ty: Type,
        ability_: Ability_,
    ) {
        self.add_ability_set_constraint(
            loc,
            msg_opt,
            ty,
            AbilitySet::from_abilities(vec![sp(loc, ability_)]).unwrap(),
        )
    }

    pub fn add_ability_set_constraint(
        &mut self,
        loc: Loc,
        msg_opt: Option<impl Into<String>>,
        ty: Type,
        constraints: AbilitySet,
    ) {
        self.constraints.push(Constraint::AbilityConstraint {
            loc,
            msg: msg_opt.map(|s| s.into()),
            ty,
            constraints,
        })
    }

    pub fn add_base_type_constraint(&mut self, loc: Loc, msg: impl Into<String>, t: Type) {
        self.constraints
            .push(Constraint::BaseTypeConstraint(loc, msg.into(), t))
    }

    pub fn add_single_type_constraint(&mut self, loc: Loc, msg: impl Into<String>, t: Type) {
        self.constraints
            .push(Constraint::SingleTypeConstraint(loc, msg.into(), t))
    }

    pub fn add_numeric_constraint(&mut self, loc: Loc, op: &'static str, t: Type) {
        self.constraints
            .push(Constraint::NumericConstraint(loc, op, t))
    }

    pub fn add_bits_constraint(&mut self, loc: Loc, op: &'static str, t: Type) {
        self.constraints
            .push(Constraint::BitsConstraint(loc, op, t))
    }

    pub fn add_ordered_constraint(&mut self, loc: Loc, op: &'static str, t: Type) {
        self.constraints
            .push(Constraint::OrderedConstraint(loc, op, t))
    }

    pub fn declare_local(&mut self, _: Mutability, var: Var, ty: Type) {
        if let Err((_, prev_loc)) = self.locals.add(var, ty) {
            let msg = format!("ICE duplicate {var:?}. Should have been made unique in naming");
            self.add_diag(ice!((var.loc, msg), (prev_loc, "Previously declared here")));
        }
    }

    pub fn get_local_type(&mut self, var: &Var) -> Type {
        if !self.locals.contains_key(var) {
            let msg = format!("ICE unbound {var:?}. Should have failed in naming");
            self.add_diag(ice!((var.loc, msg)));
            return self.error_type(var.loc);
        }

        self.locals.get(var).unwrap().clone()
    }

    pub fn is_current_function(&self, m: &ModuleIdent, f: &FunctionName) -> bool {
        self.is_current_module(m) && matches!(&self.current_function, Some(curf) if curf == f)
    }

    // `loc` indicates the location that caused the add to occur
    fn record_current_module_as_friend(&mut self, m: &ModuleIdent, loc: Loc) {
        if !self.is_current_module(m) {
            self.new_friends.insert((*m, loc));
        }
    }

    /// current_module.is_test_only || current_function.is_test_only || current_function.is_test
    fn is_testing_context(&self) -> bool {
        self.current_module().as_ref().is_some_and(|m| {
            let minfo = self.module_info(m);
            let is_test_only = minfo.attributes.is_test_or_test_only();
            is_test_only
                || self.current_function.as_ref().is_some_and(|f| {
                    let finfo = minfo.functions.get(f).unwrap();
                    finfo.attributes.is_test_or_test_only()
                })
        })
    }

    // pass in a location for a better error location
    pub fn named_block_type(&mut self, name: BlockLabel, loc: Loc) -> Type {
        if let Some(ty) = self.named_block_map.get(&name) {
            ty.clone()
        } else {
            // A named block may diverge
            let new_type = make_divergent_tvar(self, loc);
            self.named_block_map.insert(name, new_type.clone());
            new_type
        }
    }

    // pass in a location for a better error location
    pub fn update_named_block_type(&mut self, name: BlockLabel, ty: Type) {
        self.named_block_map.insert(name, ty);
    }

    pub fn named_block_type_opt(&self, name: BlockLabel) -> Option<Type> {
        self.named_block_map.get(&name).cloned()
    }

    pub fn next_variable_color(&mut self) -> Color {
        let max_variable_color: &mut u16 = &mut self.max_variable_color.borrow_mut();
        *max_variable_color += 1;
        *max_variable_color
    }

    pub fn set_max_variable_color(&self, color: Color) {
        let max_variable_color: &mut u16 = &mut self.max_variable_color.borrow_mut();
        assert!(
            *max_variable_color <= color,
            "ICE a new, lower color means reusing variables \
            {} <= {}",
            *max_variable_color,
            color,
        );
        *max_variable_color = color;
    }

    fn next_match_var_id(&mut self) -> usize {
        self.next_match_var_id += 1;
        self.next_match_var_id
    }

    pub fn finish(
        self,
    ) -> (
        BTreeSet<(ModuleIdent, Loc)>,
        BTreeMap<ModuleIdent_, BTreeSet<Symbol>>,
        UsedMethods,
    ) {
        let Self {
            used_module_members,
            new_friends,
            mut use_funs,
            ..
        } = self;
        assert!(use_funs.len() <= 2);
        let used_methods = match use_funs.pop().unwrap().use_funs {
            UseFunsScope_::Global(_) => BTreeSet::new(),
            UseFunsScope_::Outer(_, used) => used,
            UseFunsScope_::Local(_) => panic!("ICE last scope should never be local"),
        };
        (new_friends, used_module_members, used_methods)
    }

    //********************************************
    // IDE Information
    //********************************************

    /// Find all valid methods in scope for a given `TypeName`. This is used for autocomplete.
    pub fn find_all_methods(&mut self, tn: &TypeName) -> Vec<AutocompleteMethod> {
        debug_print!(self.debug().autocomplete_resolution, (msg "methods"), ("name" => tn));
        if !self
            .env()
            .supports_feature(self.current_package(), FeatureGate::DotCall)
        {
            debug_print!(self.debug().autocomplete_resolution, (msg "dot call unsupported"));
            return vec![];
        }
        let cur_color = self.use_funs.last().unwrap().color;
        let mut result = BTreeSet::new();
        self.use_funs.iter().rev().for_each(|scope| {
            if scope.color.is_some() && scope.color != cur_color {
                return;
            }
            if let Some(names) = scope.use_funs.resolved().get(tn) {
                let mut new_names = names
                    .iter()
                    .map(|(_, method_name, use_fun)| {
                        AutocompleteMethod::new(*method_name, use_fun.target_function)
                    })
                    .collect();
                result.append(&mut new_names);
            }
        });
        let (same, mut different) = result
            .clone()
            .into_iter()
            .partition::<Vec<_>, _>(|a| a.method_name == a.target_function.1.value());
        // favor aliased completions over those where method name is the same as the target function
        // name as the former are shadowing the latter - keep the latter only if the aliased set has
        // no entry with the same target or with the same method name
        let mut same_filtered = vec![];
        'outer: for sa in same.into_iter() {
            for da in different.iter() {
                if da.method_name == sa.method_name
                    || da.target_function.1.value() == sa.target_function.1.value()
                {
                    continue 'outer;
                }
            }
            same_filtered.push(sa);
        }

        different.append(&mut same_filtered);
        different.sort_by(|a1, a2| a1.method_name.cmp(&a2.method_name));
        different
    }

    pub fn add_ide_info(&mut self, loc: Loc, info: IDEAnnotation) {
        self.ide_info.add_ide_annotation(loc, info);
    }
}

impl MatchContext<false> for Context<'_, '_> {
    fn env(&self) -> &CompilationEnv {
        self.env()
    }

    fn reporter(&self) -> &DiagnosticReporter<'_> {
        &self.reporter
    }

    /// Makes a new `naming/ast.rs` variable. Does _not_ record it as a function local, since this
    /// should only be called in match expansion, which will have its body processed in HLIR
    /// translation after type expansion.
    fn new_match_var(&mut self, name: String, loc: Loc) -> N::Var {
        let id = self.next_match_var_id();
        let name = new_match_var_name(&name, id);
        // NOTE: Since these variables are only used for counterexample generation, etc., color
        // does not matter.
        sp(
            loc,
            N::Var_ {
                name,
                id: id as u16,
                color: 0,
            },
        )
    }

    fn program_info(&self) -> &ProgramInfo<false> {
        self.info()
    }
}

impl MacroExpansion {
    fn maybe_name(&self) -> Option<(ModuleIdent, FunctionName)> {
        match self {
            MacroExpansion::Call(call) => Some((call.module, call.function)),
            MacroExpansion::Argument { .. } => None,
        }
    }
}

impl Default for TVarCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl TVarCounter {
    pub fn new() -> Self {
        TVarCounter { next: 0 }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> TVar {
        let id = self.next;
        self.next += 1;
        TVar(id)
    }
}

//**************************************************************************************************
// Subst
//**************************************************************************************************

#[derive(Clone, Debug)]
pub struct Subst {
    tvars: HashMap<TVar, Type>,
    tvar_constraints: HashMap<TVar, VarConstraint>,
}

#[derive(Clone, Debug)]
// This will eventually hold constraints like `Void` and `String` as well
pub enum VarConstraint {
    Num(Loc),
    Divergent(Loc),
}

impl Subst {
    pub fn empty() -> Self {
        Self {
            tvars: HashMap::new(),
            tvar_constraints: HashMap::new(),
        }
    }

    pub fn insert(&mut self, tvar: TVar, bt: Type) {
        self.tvars.insert(tvar, bt);
    }

    pub fn insert_with_constraint(&mut self, tvar: TVar, bt: Type) {
        self.tvars.insert(tvar, bt);
    }

    pub fn get(&self, tvar: TVar) -> Option<&Type> {
        self.tvars.get(&tvar)
    }

    pub fn new_num_var(&mut self, counter: &mut TVarCounter, loc: Loc) -> TVar {
        let tvar = counter.next();
        assert!(
            self.tvar_constraints
                .insert(tvar, VarConstraint::Num(loc))
                .is_none()
        );
        tvar
    }

    pub fn new_divergent_var(&mut self, counter: &mut TVarCounter, loc: Loc) -> TVar {
        let tvar = counter.next();
        assert!(
            self.tvar_constraints
                .insert(tvar, VarConstraint::Divergent(loc))
                .is_none()
        );
        tvar
    }

    pub fn set_constraint_opt(&mut self, tvar: TVar, constraint_opt: Option<VarConstraint>) {
        if let Some(constraint) = constraint_opt {
            assert!(self.tvar_constraints.insert(tvar, constraint).is_none());
        }
    }

    pub fn is_num_var(&self, tvar: &TVar) -> bool {
        self.tvar_constraints
            .get(tvar)
            .map(|constraint| constraint.is_num_var())
            .unwrap_or(false)
    }
}

impl VarConstraint {
    pub fn is_num_var(&self) -> bool {
        matches!(self, VarConstraint::Num(_))
    }

    pub fn loc(&self) -> Loc {
        match self {
            VarConstraint::Num(loc) => *loc,
            VarConstraint::Divergent(loc) => *loc,
        }
    }
}

impl ast_debug::AstDebug for Subst {
    fn ast_debug(&self, w: &mut ast_debug::AstWriter) {
        let Subst {
            tvars,
            tvar_constraints: var_constraints,
        } = self;

        w.write("tvars:");
        w.indent(4, |w| {
            let mut tvars = tvars.iter().collect::<Vec<_>>();
            tvars.sort_by_key(|(v, _)| *v);
            for (tvar, bt) in tvars {
                w.write(format!("{:?} => ", tvar));
                bt.ast_debug(w);
                w.new_line();
            }
        });
        w.write("tvar_constraints:");
        w.indent(4, |w| {
            let mut num_vars = var_constraints.keys().collect::<Vec<_>>();
            num_vars.sort();
            for tvar in num_vars {
                w.writeln(format!("{:?}", tvar))
            }
        });
    }
}

//**************************************************************************************************
// Type error display
//**************************************************************************************************

pub fn error_format(b: &Type, subst: &Subst) -> String {
    error_format_impl(b, subst, false)
}

pub fn error_format_(b_: &Type_, subst: &Subst) -> String {
    error_format_impl_(b_, subst, false)
}

pub fn error_format_nested(b: &Type, subst: &Subst) -> String {
    error_format_impl(b, subst, true)
}

fn error_format_impl(sp!(_, b_): &Type, subst: &Subst, nested: bool) -> String {
    error_format_impl_(b_, subst, nested)
}

fn error_format_impl_(b_: &Type_, subst: &Subst, nested: bool) -> String {
    use Type_::*;
    let res = match b_ {
        UnresolvedError | Anything | Void => "_".to_string(),
        Unit => "()".to_string(),
        Var(id) => {
            let last_id = forward_tvar(subst, *id);
            match subst.get(last_id) {
                Some(sp!(_, Var(_))) => unreachable!(),
                Some(t) => error_format_nested(t, subst),
                None if nested && subst.is_num_var(&last_id) => "{integer}".to_string(),
                None if subst.is_num_var(&last_id) => return "integer".to_string(),
                None => "_".to_string(),
            }
        }
        Apply(_, sp!(_, TypeName_::Multiple(_)), tys) => {
            let inner = format_comma(tys.iter().map(|s| error_format_nested(s, subst)));
            format!("({})", inner)
        }
        Apply(_, n, tys) => {
            let tys_str = if !tys.is_empty() {
                format!(
                    "<{}>",
                    format_comma(tys.iter().map(|t| error_format_nested(t, subst)))
                )
            } else {
                "".to_string()
            };
            format!("{}{}", n, tys_str)
        }
        Fun(args, result) => {
            format!(
                "|{}| -> {}",
                format_comma(args.iter().map(|t| error_format_nested(t, subst))),
                error_format_nested(result, subst)
            )
        }
        Param(tp) => tp.user_specified_name.value.to_string(),
        Ref(mut_, ty) => format!(
            "&{}{}",
            if *mut_ { "mut " } else { "" },
            error_format_nested(ty, subst)
        ),
    };
    if nested { res } else { format!("'{}'", res) }
}

//**************************************************************************************************
// Type utils
//**************************************************************************************************

pub fn infer_abilities<const INFO_PASS: bool>(
    context: &ProgramInfo<INFO_PASS>,
    subst: &Subst,
    ty: Type,
) -> AbilitySet {
    use Type_ as T;
    let loc = ty.loc;
    match unfold_type(subst, ty).value {
        T::Unit => AbilitySet::collection(loc),
        T::Ref(_, _) => AbilitySet::references(loc),
        T::Var(_) => unreachable!("ICE unfold_type failed, which is impossible"),
        T::UnresolvedError | T::Anything | T::Void => AbilitySet::all(loc),
        T::Param(TParam { abilities, .. }) | T::Apply(Some(abilities), _, _) => abilities,
        T::Apply(None, n, ty_args) => {
            let (declared_abilities, ty_args) = match &n.value {
                TypeName_::Multiple(_) => (AbilitySet::collection(loc), ty_args),
                TypeName_::Builtin(b) => (b.value.declared_abilities(b.loc), ty_args),
                TypeName_::ModuleType(m, n) => match context.datatype_kind(m, n) {
                    DatatypeKind::Struct => {
                        let declared_abilities = context.struct_declared_abilities(m, n).clone();
                        let non_phantom_ty_args = ty_args
                            .into_iter()
                            .zip(context.struct_type_parameters(m, n))
                            .filter(|(_, param)| !param.is_phantom)
                            .map(|(arg, _)| arg)
                            .collect::<Vec<_>>();
                        (declared_abilities, non_phantom_ty_args)
                    }
                    DatatypeKind::Enum => {
                        let declared_abilities = context.enum_declared_abilities(m, n).clone();
                        let non_phantom_ty_args = ty_args
                            .into_iter()
                            .zip(context.enum_type_parameters(m, n))
                            .filter(|(_, param)| !param.is_phantom)
                            .map(|(arg, _)| arg)
                            .collect::<Vec<_>>();
                        (declared_abilities, non_phantom_ty_args)
                    }
                },
            };
            let ty_args_abilities = ty_args
                .into_iter()
                .map(|ty| infer_abilities(context, subst, ty))
                .collect::<Vec<_>>();
            AbilitySet::from_abilities(declared_abilities.into_iter().filter(|ab| {
                let requirement = ab.value.requires();
                ty_args_abilities
                    .iter()
                    .all(|ty_arg_abilities| ty_arg_abilities.has_ability_(requirement))
            }))
            .unwrap()
        }
        T::Fun(_, _) => AbilitySet::functions(loc),
    }
}

// Returns
// - the declared location where abilities are added (if applicable)
// - the set of declared abilities
// - its type arguments
fn debug_abilities_info(context: &mut Context, ty: &Type) -> (Option<Loc>, AbilitySet, Vec<Type>) {
    use Type_ as T;
    let loc = ty.loc;
    match &ty.value {
        T::Unit | T::Ref(_, _) => (None, AbilitySet::references(loc), vec![]),
        T::Var(_) => {
            let diag = ice!((
                loc,
                "ICE did not call unfold_type before debug_abiliites_info"
            ));
            context.add_diag(diag);
            (None, AbilitySet::all(loc), vec![])
        }
        T::UnresolvedError | T::Anything | T::Void => (None, AbilitySet::all(loc), vec![]),
        T::Param(TParam {
            abilities,
            user_specified_name,
            ..
        }) => (Some(user_specified_name.loc), abilities.clone(), vec![]),
        T::Apply(_, sp!(_, TypeName_::Multiple(_)), ty_args) => {
            (None, AbilitySet::collection(loc), ty_args.clone())
        }
        T::Apply(_, sp!(_, TypeName_::Builtin(b)), ty_args) => {
            (None, b.value.declared_abilities(b.loc), ty_args.clone())
        }
        T::Apply(_, sp!(_, TypeName_::ModuleType(m, n)), ty_args) => {
            match context.datatype_kind(m, n) {
                DatatypeKind::Struct => (
                    Some(context.struct_declared_loc(m, n)),
                    context.struct_declared_abilities(m, n).clone(),
                    ty_args.clone(),
                ),
                DatatypeKind::Enum => (
                    Some(context.enum_declared_loc(m, n)),
                    context.enum_declared_abilities(m, n).clone(),
                    ty_args.clone(),
                ),
            }
        }
        T::Fun(_, _) => (None, AbilitySet::functions(loc), vec![]),
    }
}

pub fn make_divergent_tvar(context: &mut Context, loc: Loc) -> Type {
    let tvar = context
        .subst
        .new_divergent_var(&mut context.tvar_counter, loc);
    sp(loc, Type_::Var(tvar))
}

pub fn make_num_tvar(context: &mut Context, loc: Loc) -> Type {
    let tvar = context.subst.new_num_var(&mut context.tvar_counter, loc);
    sp(loc, Type_::Var(tvar))
}

pub fn make_tvar(context: &mut Context, loc: Loc) -> Type {
    sp(loc, Type_::Var(context.tvar_counter.next()))
}

//**************************************************************************************************
// Structs
//**************************************************************************************************

pub fn make_struct_type(
    context: &mut Context,
    loc: Loc,
    m: &ModuleIdent,
    n: &DatatypeName,
    ty_args_opt: Option<Vec<Type>>,
) -> (Type, Vec<Type>) {
    context.emit_warning_if_deprecated(m, n.0, None);
    let tn = sp(loc, TypeName_::ModuleType(*m, *n));
    let sdef = context.struct_definition(m, n);
    match ty_args_opt {
        None => {
            let constraints = sdef
                .type_parameters
                .iter()
                .map(|tp| (loc, tp.param.abilities.clone()))
                .collect();
            let ty_args = make_tparams(context, loc, TVarCase::Base, constraints);
            (sp(loc, Type_::Apply(None, tn, ty_args.clone())), ty_args)
        }
        Some(ty_args) => {
            let tapply_ = instantiate_apply(context, loc, None, tn, ty_args);
            let targs = match &tapply_ {
                Type_::Apply(_, _, targs) => targs.clone(),
                _ => unreachable!(),
            };
            (sp(loc, tapply_), targs)
        }
    }
}

pub fn make_expr_list_tvars(
    context: &mut Context,
    loc: Loc,
    constraint_msg: impl Into<String>,
    locs: Vec<Loc>,
) -> Vec<Type> {
    let constraints = locs.iter().map(|l| (*l, AbilitySet::empty())).collect();
    let tys = make_tparams(
        context,
        loc,
        TVarCase::Single(constraint_msg.into()),
        constraints,
    );
    tys.into_iter()
        .zip(locs)
        .map(|(tvar, l)| sp(l, tvar.value))
        .collect()
}

// ty_args should come from make_struct_type
pub fn make_struct_field_types(
    context: &mut Context,
    _loc: Loc,
    m: &ModuleIdent,
    n: &DatatypeName,
    ty_args: Vec<Type>,
) -> N::StructFields {
    let sdef = context.struct_definition(m, n);
    let tparam_subst = &make_tparam_subst(
        context
            .struct_definition(m, n)
            .type_parameters
            .iter()
            .map(|tp| &tp.param),
        ty_args,
    );
    match &sdef.fields {
        N::StructFields::Native(loc) => N::StructFields::Native(*loc),
        N::StructFields::Defined(positional, m) => N::StructFields::Defined(
            *positional,
            m.ref_map(|_, (idx, (_, field_ty))| {
                let doc = DocComment::empty();
                let ty = subst_tparams(tparam_subst, field_ty.clone());
                (*idx, (doc, ty))
            }),
        ),
    }
}

// ty_args should come from make_struct_type
pub fn make_struct_field_type(
    context: &mut Context,
    loc: Loc,
    m: &ModuleIdent,
    n: &DatatypeName,
    ty_args: Vec<Type>,
    field: &Field,
) -> Type {
    let sdef = context.struct_definition(m, n);
    let fields_map = match &sdef.fields {
        N::StructFields::Native(nloc) => {
            let nloc = *nloc;
            let msg = format!("Unbound field '{}' for native struct '{}::{}'", field, m, n);
            context.add_diag(diag!(
                NameResolution::UnboundField,
                (loc, msg),
                (nloc, "Struct declared 'native' here")
            ));
            return context.error_type(loc);
        }
        N::StructFields::Defined(_, m) => m,
    };
    match fields_map.get(field).cloned() {
        None => {
            context.add_diag(diag!(
                NameResolution::UnboundField,
                (loc, format!("Unbound field '{}' in '{}::{}'", field, m, n)),
            ));
            context.error_type(loc)
        }
        Some((_, (_, field_ty))) => {
            let tparam_subst = &make_tparam_subst(
                context
                    .struct_definition(m, n)
                    .type_parameters
                    .iter()
                    .map(|tp| &tp.param),
                ty_args,
            );
            subst_tparams(tparam_subst, field_ty)
        }
    }
}

pub fn find_index_funs(context: &mut Context, type_name: &TypeName) -> Option<IndexSyntaxMethods> {
    let module_ident = match &type_name.value {
        TypeName_::Multiple(_) => return None,
        TypeName_::Builtin(builtin_name) => context.env().primitive_definer(builtin_name.value)?,
        TypeName_::ModuleType(m, _) => m,
    };
    let module_defn = context.module_info(module_ident);
    let entry = module_defn.syntax_methods.get(type_name)?;
    let index = entry.index.clone()?;
    Some(*index)
}

//**************************************************************************************************
// Enums
//**************************************************************************************************

pub fn make_enum_type(
    context: &mut Context,
    loc: Loc,
    mident: &ModuleIdent,
    enum_: &DatatypeName,
    ty_args_opt: Option<Vec<Type>>,
) -> (Type, Vec<Type>) {
    context.emit_warning_if_deprecated(mident, enum_.0, None);
    let tn = sp(loc, TypeName_::ModuleType(*mident, *enum_));
    let edef = context.enum_definition(mident, enum_);
    match ty_args_opt {
        None => {
            let constraints = edef
                .type_parameters
                .iter()
                .map(|tp| (loc, tp.param.abilities.clone()))
                .collect();
            let ty_args = make_tparams(context, loc, TVarCase::Base, constraints);
            (sp(loc, Type_::Apply(None, tn, ty_args.clone())), ty_args)
        }
        Some(ty_args) => {
            let tapply_ = instantiate_apply(context, loc, None, tn, ty_args);
            let targs = match &tapply_ {
                Type_::Apply(_, _, targs) => targs.clone(),
                _ => panic!("ICE instantiate_apply returned non Apply"),
            };
            (sp(loc, tapply_), targs)
        }
    }
}

// ty_args should come from make_enum_type
pub fn make_variant_field_types(
    context: &mut Context,
    _loc: Loc,
    mident: &ModuleIdent,
    enum_: &DatatypeName,
    variant: &VariantName,
    ty_args: Vec<Type>,
) -> N::VariantFields {
    let edef = context.enum_definition(mident, enum_);
    let tparam_subst = &make_tparam_subst(edef.type_parameters.iter().map(|tp| &tp.param), ty_args);
    let vdef = edef
        .variants
        .get(variant)
        .expect("ICE should have failed during naming");
    match &vdef.fields {
        N::VariantFields::Empty => N::VariantFields::Empty,
        N::VariantFields::Defined(is_positional, m) => N::VariantFields::Defined(
            *is_positional,
            m.ref_map(|_, (idx, (_, field_ty))| {
                let doc = DocComment::empty();
                let ty = subst_tparams(tparam_subst, field_ty.clone());
                (*idx, (doc, ty))
            }),
        ),
    }
}

//**************************************************************************************************
// Constants
//**************************************************************************************************

pub fn make_constant_type(
    context: &mut Context,
    loc: Loc,
    m: &ModuleIdent,
    c: &ConstantName,
) -> Type {
    let in_current_module = context.is_current_module(m);
    context.emit_warning_if_deprecated(m, c.0, None);
    let (defined_loc, signature) = {
        let ConstantInfo {
            doc: _,
            index: _,
            attributes: _,
            defined_loc,
            signature,
            value: _,
        } = context.constant_info(m, c);
        (*defined_loc, signature.clone())
    };
    if !in_current_module {
        let msg = format!("Invalid access of '{}::{}'", m, c);
        let internal_msg = "Constants are internal to their module, and cannot can be accessed \
                            outside of their module";
        context.add_diag(diag!(
            TypeSafety::Visibility,
            (loc, msg),
            (defined_loc, internal_msg)
        ));
    }

    signature
}

//**************************************************************************************************
// Functions
//**************************************************************************************************

pub fn make_method_call_type(
    context: &mut Context,
    loc: Loc,
    lhs_ty: &Type,
    tn: &TypeName,
    method: Name,
    ty_args_opt: Option<Vec<Type>>,
) -> Option<(ModuleIdent, FunctionName, ResolvedFunctionType)> {
    let target_function_opt = context.find_method_and_mark_used(tn, method);
    // try to find a function in the defining module for errors
    let Some((target_m, target_f)) = target_function_opt else {
        let lhs_ty_str = error_format_nested(lhs_ty, &context.subst);
        let defining_module = match &tn.value {
            TypeName_::Multiple(_) => {
                let diag = ice!((
                    loc,
                    format!("ICE method on tuple type {}", debug_display!(tn))
                ));
                context.add_diag(diag);
                return None;
            }
            TypeName_::Builtin(sp!(_, bt_)) => context.env().primitive_definer(*bt_),
            TypeName_::ModuleType(m, _) => Some(m),
        };
        let finfo_opt = defining_module.and_then(|m| {
            let finfo = context
                .info()
                .module(m)
                .functions
                .get(&FunctionName(method))?;
            Some((m, finfo))
        });
        // if we found a function with the method name, it must have the wrong type
        if let Some((m, finfo)) = finfo_opt {
            let (first_ty_loc, first_ty) = match finfo
                .signature
                .parameters
                .first()
                .map(|(_, _, t)| t.clone())
            {
                None => (finfo.defined_loc, None),
                Some(t) => (t.loc, Some(t)),
            };
            let arg_msg = match first_ty {
                Some(ty) => {
                    let tys_str = error_format(&ty, &context.subst);
                    format!("but it has a different type for its first argument, {tys_str}")
                }
                None => "but it takes no arguments".to_owned(),
            };
            let msg = format!(
                "Invalid method call. \
                No known method '{method}' on type '{lhs_ty_str}'"
            );
            let fmsg = format!("The function '{m}::{method}' exists, {arg_msg}");
            context.add_diag(diag!(
                TypeSafety::InvalidMethodCall,
                (loc, msg),
                (first_ty_loc, fmsg)
            ));
        } else {
            let msg = format!(
                "Invalid method call. \
                No known method '{method}' on type '{lhs_ty_str}'"
            );
            let decl_msg = match defining_module {
                Some(m) => {
                    format!(", and no function '{method}' was found in the defining module '{m}'")
                }
                None => "".to_owned(),
            };
            let fmsg =
                format!("No local 'use fun' alias was found for '{lhs_ty_str}.{method}'{decl_msg}");
            context.add_diag(diag!(
                TypeSafety::InvalidMethodCall,
                (loc, msg),
                (method.loc, fmsg)
            ));
        }
        return None;
    };

    let target_f = target_f.with_loc(method.loc);
    let function_ty = make_function_type(
        context,
        loc,
        &target_m,
        &target_f,
        ty_args_opt,
        Some(method),
    );

    Some((target_m, target_f, function_ty))
}

/// Make a new resolved function type for the provided function and type arguments.
/// This also checks call visibility, including recording new friends for `public(package)`.
pub fn make_function_type(
    context: &mut Context,
    loc: Loc,
    m: &ModuleIdent,
    f: &FunctionName,
    ty_args_opt: Option<Vec<Type>>,
    method_opt: Option<Name>,
) -> ResolvedFunctionType {
    context.emit_warning_if_deprecated(m, f.0, method_opt);
    let return_ty = make_function_type_no_visibility_check(context, loc, m, f, ty_args_opt);
    let finfo = context.function_info(m, f);
    let defined_loc = finfo.defined_loc;
    check_function_visibility(
        context,
        defined_loc,
        loc,
        m,
        f,
        finfo.entry,
        finfo.visibility,
    );
    return_ty
}

/// Make a new resolved function type for the provided function and type arguments.
/// THIS DOES NOT CHECK CALL VISIBILITY, AND SHOULD BE USED CAREFULLY.
pub fn make_function_type_no_visibility_check(
    context: &mut Context,
    loc: Loc,
    m: &ModuleIdent,
    f: &FunctionName,
    ty_args_opt: Option<Vec<Type>>,
) -> ResolvedFunctionType {
    let finfo = context.function_info(m, f);
    let macro_ = finfo.macro_;
    let constraints: Vec<_> = finfo
        .signature
        .type_parameters
        .iter()
        .map(|tp| tp.abilities.clone())
        .collect();

    let ty_args = match ty_args_opt {
        None => {
            let case = if macro_.is_some() {
                TVarCase::Macro
            } else {
                TVarCase::Base
            };
            let locs_constraints = constraints.into_iter().map(|k| (loc, k)).collect();
            make_tparams(context, loc, case, locs_constraints)
        }
        Some(ty_args) => {
            let case = if macro_.is_some() {
                TArgCase::Macro
            } else {
                TArgCase::Fun
            };
            let ty_args = check_type_argument_arity(
                context,
                loc,
                || format!("{}::{}", m, f),
                ty_args,
                &constraints,
            );
            instantiate_type_args(context, loc, case, ty_args, constraints)
        }
    };

    let finfo = context.function_info(m, f);
    let tparam_subst = &make_tparam_subst(&finfo.signature.type_parameters, ty_args.clone());
    let params = finfo
        .signature
        .parameters
        .iter()
        .map(|(_, n, t)| (*n, subst_tparams(tparam_subst, t.clone())))
        .collect();
    let return_ty = subst_tparams(tparam_subst, finfo.signature.return_type.clone());

    let defined_loc = finfo.defined_loc;
    ResolvedFunctionType {
        declared: defined_loc,
        macro_,
        ty_args,
        params,
        return_: return_ty,
    }
}

fn check_function_visibility(
    context: &mut Context,
    defined_loc: Loc,
    usage_loc: Loc,
    m: &ModuleIdent,
    f: &FunctionName,
    entry_opt: Option<Loc>,
    visibility: Visibility,
) {
    let in_current_module = context.is_current_module(m);
    let public_for_testing =
        public_testing_visibility(context.env(), context.current_package(), f, entry_opt);
    let is_testing_context = context.is_testing_context();
    let supports_public_package = context
        .env()
        .supports_feature(context.current_package(), FeatureGate::PublicPackage);
    match visibility {
        _ if is_testing_context && public_for_testing.is_some() => (),
        Visibility::Internal if in_current_module => (),
        Visibility::Internal => {
            let friend_or_package = if supports_public_package {
                Visibility::PACKAGE
            } else {
                Visibility::FRIEND
            };
            let internal_msg = format!(
                "This function is internal to its module. Only '{}' and '{}' functions can \
                 be called outside of their module",
                Visibility::PUBLIC,
                friend_or_package,
            );
            report_visibility_error_(
                context,
                public_for_testing,
                (
                    usage_loc,
                    format!("Invalid call to internal function '{m}::{f}'"),
                ),
                (defined_loc, internal_msg),
            );
        }
        Visibility::Package(loc)
            if in_current_module || context.current_module_shares_package_and_address(m) =>
        {
            context.record_current_module_as_friend(m, loc);
        }
        Visibility::Package(vis_loc) => {
            let msg = format!(
                "Invalid call to '{}' visible function '{}::{}'",
                Visibility::PACKAGE,
                m,
                f
            );
            let internal_msg = format!(
                "A '{}' function can only be called from the same address and package as \
                module '{}' in package '{}'. This call is from address '{}' in package '{}'",
                Visibility::PACKAGE,
                m,
                context
                    .module_info(m)
                    .package
                    .map(|pkg_name| format!("{}", pkg_name))
                    .unwrap_or("<unknown package>".to_string()),
                &context
                    .current_module()
                    .map(|cur_module| cur_module.value.address.to_string())
                    .unwrap_or("<unknown addr>".to_string()),
                &context
                    .current_module()
                    .and_then(|cur_module| context.module_info(cur_module).package)
                    .map(|pkg_name| format!("{}", pkg_name))
                    .unwrap_or("<unknown package>".to_string())
            );
            report_visibility_error_(
                context,
                public_for_testing,
                (usage_loc, msg),
                (vis_loc, internal_msg),
            );
        }
        Visibility::Friend(_) if in_current_module || context.current_module_is_a_friend_of(m) => {}
        Visibility::Friend(vis_loc) => {
            let msg = format!(
                "Invalid call to '{}' visible function '{m}::{f}'",
                Visibility::FRIEND,
            );
            let internal_msg =
                format!("This function can only be called from a 'friend' of module '{m}'",);
            report_visibility_error_(
                context,
                public_for_testing,
                (usage_loc, msg),
                (vis_loc, internal_msg),
            );
        }
        Visibility::Public(_) => (),
    }
}

#[derive(Clone, Copy)]
pub enum PublicForTesting {
    /// The function is entry, so it can be called in unit tests
    Entry(Loc),
    // TODO we should allow calling init in unit tests, but this would need Sui bytecode verifier
    // support. Or we would need to name dodge init in unit tests
    // SuiInit(Loc),
}

pub fn public_testing_visibility(
    env: &CompilationEnv,
    _package: Option<Symbol>,
    _callee_name: &FunctionName,
    callee_entry: Option<Loc>,
) -> Option<PublicForTesting> {
    // is_testing && (is_entry || is_sui_init)
    if !env.test_mode() {
        return None;
    }

    // TODO support sui init functions
    // let flavor = env.package_config(package).flavor;
    // flavor == Flavor::Sui && callee_name.value() == INIT_FUNCTION_NAME
    callee_entry.map(PublicForTesting::Entry)
}

pub fn report_visibility_error(
    context: &mut Context,
    call_msg: (Loc, impl ToString),
    defn_msg: (Loc, impl ToString),
) {
    report_visibility_error_(context, None, call_msg, defn_msg)
}

fn report_visibility_error_(
    context: &mut Context,
    public_for_testing: Option<PublicForTesting>,
    (call_loc, call_msg): (Loc, impl ToString),
    (vis_loc, vis_msg): (Loc, impl ToString),
) {
    let mut diag = diag!(
        TypeSafety::Visibility,
        (call_loc, call_msg),
        (vis_loc, vis_msg),
    );
    if context.env().test_mode() {
        if let Some(case) = public_for_testing {
            let (test_loc, test_msg) = match case {
                PublicForTesting::Entry(entry_loc) => {
                    let entry_msg = format!(
                        "'{}' functions can be called in tests, \
                    but only from testing contexts, e.g. '#[{}]' or '#[{}]'",
                        ENTRY_MODIFIER,
                        TestingAttribute::TEST,
                        ModeAttribute::TEST_ONLY,
                    );
                    (entry_loc, entry_msg)
                }
            };
            diag.add_secondary_label((test_loc, test_msg))
        }
    }
    if let Some(names) = context.expanding_macros_names() {
        let macro_s = if context.macro_expansion.len() > 1 {
            "macros"
        } else {
            "macro"
        };
        match context.macro_expansion.first() {
            Some(MacroExpansion::Call(call)) => {
                diag.add_secondary_label((call.invocation, "While expanding this macro"));
            }
            _ => {
                context.add_diag(ice!((
                    call_loc,
                    "Error when dealing with macro visibilities"
                )));
            }
        };
        diag.add_note(format!(
            "This visibility error occurs in a macro body while expanding the {macro_s} {names}"
        ));
        diag.add_note(
            "Visibility inside of expanded macros is resolved in the scope of the caller.",
        );
    }
    context.add_diag(diag);
}

pub fn check_call_arity<S: std::fmt::Display, F: Fn() -> S>(
    context: &mut Context,
    loc: Loc,
    msg: F,
    arity_loc: Option<Loc>,
    arity: usize,
    argloc: Loc,
    given_len: usize,
) {
    if given_len == arity {
        return;
    }
    let code = if given_len < arity {
        TypeSafety::TooFewArguments
    } else {
        TypeSafety::TooManyArguments
    };
    let cmsg = format!(
        "{}. The call expected {} argument(s) but got {}",
        msg(),
        arity,
        given_len
    );
    let args_msg = format!("Found {} argument(s) here", given_len);
    let diag = match arity_loc {
        None => diag!(code, (loc, cmsg), (argloc, args_msg)),
        Some(arity_loc) => diag!(
            code,
            (loc, cmsg),
            (arity_loc, format!("Expected {} argument(s)", arity)),
            (argloc, args_msg)
        ),
    };
    context.add_diag(diag);
}

//**************************************************************************************************
// Constraints
//**************************************************************************************************

pub fn solve_constraints(context: &mut Context) {
    use BuiltinTypeName_ as BT;

    let var_constraints = context.subst.tvar_constraints.clone();
    let mut subst = std::mem::replace(&mut context.subst, Subst::empty());

    for (var, constraint) in var_constraints.into_iter() {
        match constraint {
            VarConstraint::Num(loc) => {
                let tvar = sp(loc, Type_::Var(var));
                match unfold_type(&subst, tvar.clone()).value {
                    Type_::UnresolvedError | Type_::Anything => {
                        let next_subst =
                            join(&mut context.tvar_counter, subst, &Type_::u64(loc), &tvar)
                                .unwrap()
                                .0;
                        subst = next_subst;
                    }
                    _ => (),
                }
            }
            VarConstraint::Divergent(loc) => {
                let last_tvar = forward_tvar(&subst, var);
                if subst.get(last_tvar).is_none() {
                    join_bind_tvar(&mut subst, loc, last_tvar, sp(loc, Type_::Void))
                        .expect("ICE failed handling unbound divergent type");
                }
            }
        }
    }

    context.subst = subst;

    let constraints = std::mem::take(&mut context.constraints);
    for constraint in constraints {
        match constraint {
            Constraint::AbilityConstraint {
                loc,
                msg,
                ty,
                constraints,
            } => solve_ability_constraint(context, loc, msg, ty, constraints),
            Constraint::NumericConstraint(loc, op, t) => {
                solve_builtin_type_constraint(context, BT::numeric(), loc, op, t)
            }
            Constraint::BitsConstraint(loc, op, t) => {
                solve_builtin_type_constraint(context, BT::bits(), loc, op, t)
            }
            Constraint::OrderedConstraint(loc, op, t) => {
                solve_builtin_type_constraint(context, BT::ordered(), loc, op, t)
            }
            Constraint::BaseTypeConstraint(loc, msg, t) => {
                solve_base_type_constraint(context, loc, msg, &t)
            }
            Constraint::SingleTypeConstraint(loc, msg, t) => {
                solve_single_type_constraint(context, loc, msg, &t)
            }
        }
    }
}

fn solve_ability_constraint(
    context: &mut Context,
    loc: Loc,
    given_msg_opt: Option<String>,
    ty: Type,
    constraints: AbilitySet,
) {
    let ty = unfold_type(&context.subst, ty);
    let ty_abilities = infer_abilities(context.info(), &context.subst, ty.clone());

    let (declared_loc_opt, declared_abilities, ty_args) = debug_abilities_info(context, &ty);
    for constraint in constraints {
        if ty_abilities.has_ability(&constraint) {
            continue;
        }

        let constraint_msg = match &given_msg_opt {
            Some(s) => s.clone(),
            None => format!("'{}' constraint not satisifed", constraint),
        };
        let mut diag = diag!(AbilitySafety::Constraint, (loc, constraint_msg));
        ability_not_satisfied_tips(
            &context.subst,
            &mut diag,
            constraint.value,
            &ty,
            declared_loc_opt,
            &declared_abilities,
            ty_args.iter().map(|ty_arg| {
                let abilities = infer_abilities(context.info(), &context.subst, ty_arg.clone());
                (ty_arg, abilities)
            }),
        );

        // is none if it is from a user constraint and not a part of the type system
        if given_msg_opt.is_none() {
            diag.add_secondary_label((
                constraint.loc,
                format!("'{}' constraint declared here", constraint),
            ));
        }
        context.add_diag(diag)
    }
}

pub fn ability_not_satisfied_tips<'a>(
    subst: &Subst,
    diag: &mut Diagnostic,
    constraint: Ability_,
    ty: &Type,
    declared_loc_opt: Option<Loc>,
    declared_abilities: &AbilitySet,
    ty_args: impl IntoIterator<Item = (&'a Type, AbilitySet)>,
) {
    let ty_str = error_format(ty, subst);
    let ty_msg = format!(
        "The type {} does not have the ability '{}'",
        ty_str, constraint
    );
    diag.add_secondary_label((ty.loc, ty_msg));
    match (
        declared_loc_opt,
        declared_abilities.has_ability_(constraint),
    ) {
        // Type was not given the ability
        (Some(dloc), false) => diag.add_secondary_label((
            dloc,
            format!(
                "To satisfy the constraint, the '{}' ability would need to be added here",
                constraint
            ),
        )),
        // Type does not have the ability
        (_, false) => (),
        // Type has the ability but a type argument causes it to fail
        (_, true) => {
            let requirement = constraint.requires();
            let mut label_added = false;
            for (ty_arg, ty_arg_abilities) in ty_args {
                if !ty_arg_abilities.has_ability_(requirement) {
                    let ty_arg_str = error_format(ty_arg, subst);
                    let msg = format!(
                        "The type {ty} can have the ability '{constraint}' but the type argument \
                         {ty_arg} does not have the required ability '{requirement}'",
                        ty = ty_str,
                        ty_arg = ty_arg_str,
                        constraint = constraint,
                        requirement = requirement,
                    );
                    diag.add_secondary_label((ty_arg.loc, msg));
                    label_added = true;
                    break;
                }
            }
            assert!(label_added)
        }
    }
}

fn solve_builtin_type_constraint(
    context: &mut Context,
    builtin_set: &BTreeSet<BuiltinTypeName_>,
    loc: Loc,
    op: &'static str,
    ty: Type,
) {
    use Type_::*;
    use TypeName_::*;
    let t = unfold_type(&context.subst, ty);
    let tloc = t.loc;
    let mk_tmsg = || {
        let set_msg = if builtin_set.is_empty() {
            "the operation is not yet supported on any type".to_string()
        } else {
            format!(
                "expected: {}",
                format_comma(builtin_set.iter().map(|b| format!("'{}'", b)))
            )
        };
        format!(
            "Found: {}. But {}",
            error_format(&t, &context.subst),
            set_msg
        )
    };
    match &t.value {
        // already failed, ignore
        UnresolvedError => (),
        // Will fail later in compiling, either through dead code, or unknown type variable
        Anything => (),
        // Will never arrive here, so it does not matter
        Void => (),
        Apply(abilities_opt, sp!(_, Builtin(sp!(_, b))), args) if builtin_set.contains(b) => {
            if let Some(abilities) = abilities_opt {
                assert!(
                    abilities.has_ability_(Ability_::Drop),
                    "ICE assumes this type is being consumed so should have drop"
                );
            }
            assert!(args.is_empty());
        }
        _ => {
            let tmsg = mk_tmsg();
            context.add_diag(diag!(
                TypeSafety::BuiltinOperation,
                (loc, format!("Invalid argument to '{}'", op)),
                (tloc, tmsg)
            ))
        }
    }
}

fn solve_base_type_constraint(context: &mut Context, loc: Loc, msg: String, ty: &Type) {
    use Type_::*;
    use TypeName_::*;
    let sp!(tyloc, unfolded_) = unfold_type(&context.subst, ty.clone());
    match unfolded_ {
        Var(_) => unreachable!(),
        Unit | Ref(_, _) | Apply(_, sp!(_, Multiple(_)), _) => {
            let tystr = error_format(ty, &context.subst);
            let tmsg = format!("Expected a single non-reference type, but found: {}", tystr);
            context.add_diag(diag!(
                TypeSafety::ExpectedBaseType,
                (loc, msg),
                (tyloc, tmsg)
            ))
        }
        UnresolvedError | Anything | Void | Param(_) | Apply(_, _, _) | Fun(_, _) => (),
    }
}

fn solve_single_type_constraint(context: &mut Context, loc: Loc, msg: String, ty: &Type) {
    use Type_::*;
    use TypeName_::*;
    let sp!(tyloc, unfolded_) = unfold_type(&context.subst, ty.clone());
    match unfolded_ {
        Var(_) => unreachable!(),
        Unit | Apply(_, sp!(_, Multiple(_)), _) => {
            let tmsg = format!(
                "Expected a single type, but found expression list type: {}",
                error_format(ty, &context.subst)
            );
            context.add_diag(diag!(
                TypeSafety::ExpectedSingleType,
                (loc, msg),
                (tyloc, tmsg)
            ))
        }
        UnresolvedError | Anything | Void | Ref(_, _) | Param(_) | Apply(_, _, _) | Fun(_, _) => (),
    }
}

//**************************************************************************************************
// Subst
//**************************************************************************************************

pub fn unfold_type(subst: &Subst, sp!(loc, t_): Type) -> Type {
    match t_ {
        Type_::Var(i) => {
            let last_tvar = forward_tvar(subst, i);
            match subst.get(last_tvar) {
                Some(sp!(_, Type_::Var(_))) => unreachable!(),
                None => sp(loc, Type_::Anything),
                Some(inner) => inner.clone(),
            }
        }
        x => sp(loc, x),
    }
}

pub fn unfold_type_recur(subst: &Subst, sp!(_loc, t_): &mut Type) {
    use Type_ as T;
    match t_ {
        T::Var(i) => {
            let last_tvar = forward_tvar(subst, *i);
            match subst.get(last_tvar) {
                Some(sp!(_, T::Var(_))) => unreachable!(),
                None => {
                    *t_ = T::Anything;
                }
                Some(inner) => {
                    *t_ = inner.value.clone();
                }
            }
        }
        T::Unit | T::Param(_) | T::Anything | T::Void | T::UnresolvedError => (),
        T::Ref(_, inner) => unfold_type_recur(subst, inner),
        T::Apply(_, _, args) => args.iter_mut().for_each(|ty| unfold_type_recur(subst, ty)),
        T::Fun(args, ret) => {
            args.iter_mut().for_each(|ty| unfold_type_recur(subst, ty));
            unfold_type_recur(subst, ret);
        }
    }
}

// Equivelent to unfold_type, but only returns the loc.
// The hope is to point to the last loc in a chain of type var's, giving the loc closest to the
// actual type in the source code
pub fn best_loc(subst: &Subst, sp!(loc, t_): &Type) -> Loc {
    match t_ {
        Type_::Var(i) => {
            let last_tvar = forward_tvar(subst, *i);
            match subst.get(last_tvar) {
                Some(sp!(_, Type_::Var(_))) => unreachable!(),
                None => *loc,
                Some(sp!(inner_loc, _)) => *inner_loc,
            }
        }
        _ => *loc,
    }
}

pub fn make_tparam_subst<'a, I1, I2>(tps: I1, args: I2) -> TParamSubst
where
    I1: IntoIterator<Item = &'a TParam>,
    I1::IntoIter: ExactSizeIterator,
    I2: IntoIterator<Item = Type>,
    I2::IntoIter: ExactSizeIterator,
{
    let tps = tps.into_iter();
    let args = args.into_iter();
    assert!(tps.len() == args.len());
    let mut subst = TParamSubst::new();
    for (tp, arg) in tps.zip(args) {
        let old_val = subst.insert(tp.id, arg);
        assert!(old_val.is_none())
    }
    subst
}

pub fn subst_tparams(subst: &TParamSubst, sp!(loc, t_): Type) -> Type {
    use Type_::*;
    match t_ {
        x @ (Unit | UnresolvedError | Anything | Void) => sp(loc, x),
        Var(_) => panic!("ICE tvar in subst_tparams"),
        Ref(mut_, t) => sp(loc, Ref(mut_, Box::new(subst_tparams(subst, *t)))),
        Param(tp) => subst
            .get(&tp.id)
            .expect("ICE unmapped tparam in subst_tparams_base")
            .clone(),
        Apply(k, n, ty_args) => {
            let ftys = ty_args
                .into_iter()
                .map(|t| subst_tparams(subst, t))
                .collect();
            sp(loc, Apply(k, n, ftys))
        }
        Fun(args, result) => {
            let ftys = args.into_iter().map(|t| subst_tparams(subst, t)).collect();
            let fres = Box::new(subst_tparams(subst, *result));
            sp(loc, Fun(ftys, fres))
        }
    }
}

pub fn all_tparams(sp!(_, t_): Type) -> BTreeSet<TParam> {
    use Type_::*;
    match t_ {
        Unit | UnresolvedError | Anything | Void => BTreeSet::new(),
        Var(_) => panic!("ICE tvar in all_tparams"),
        Ref(_, t) => all_tparams(*t),
        Param(tp) => BTreeSet::from([tp]),
        Apply(_, _, ty_args) => {
            let mut tparams = BTreeSet::new();
            for arg in ty_args {
                tparams.append(&mut all_tparams(arg));
            }
            tparams
        }
        Fun(args, result) => {
            let mut tparams = all_tparams(*result);
            for arg in args {
                tparams.append(&mut all_tparams(arg));
            }
            tparams
        }
    }
}

pub fn ready_tvars(subst: &Subst, sp!(loc, t_): Type) -> Type {
    use Type_::*;
    match t_ {
        x @ (UnresolvedError | Unit | Anything | Void | Param(_)) => sp(loc, x),
        Ref(mut_, t) => sp(loc, Ref(mut_, Box::new(ready_tvars(subst, *t)))),
        Apply(k, n, tys) => {
            let tys = tys.into_iter().map(|t| ready_tvars(subst, t)).collect();
            sp(loc, Apply(k, n, tys))
        }
        Fun(args, result) => {
            let args = args.into_iter().map(|t| ready_tvars(subst, t)).collect();
            let result = Box::new(ready_tvars(subst, *result));
            sp(loc, Fun(args, result))
        }
        Var(i) => {
            let last_var = forward_tvar(subst, i);
            match subst.get(last_var) {
                Some(sp!(_, Var(_))) => unreachable!(),
                None => sp(loc, Var(last_var)),
                Some(t) => ready_tvars(subst, t.clone()),
            }
        }
    }
}

//**************************************************************************************************
// Instantiate
//**************************************************************************************************

pub fn instantiate(context: &mut Context, ty: Type) -> Type {
    let keep_tanything = false;
    instantiate_impl(context, keep_tanything, ty)
}

pub fn instantiate_keep_tanything(context: &mut Context, ty: Type) -> Type {
    let keep_tanything = true;
    instantiate_impl(context, keep_tanything, ty)
}

fn instantiate_apply(
    context: &mut Context,
    loc: Loc,
    abilities_opt: Option<AbilitySet>,
    n: TypeName,
    ty_args: Vec<Type>,
) -> Type_ {
    let keep_tanything = false;
    instantiate_apply_impl(context, keep_tanything, loc, abilities_opt, n, ty_args)
}

fn instantiate_type_args(
    context: &mut Context,
    loc: Loc,
    case: TArgCase,
    ty_args: Vec<Type>,
    constraints: Vec<AbilitySet>,
) -> Vec<Type> {
    let keep_tanything = false;
    instantiate_type_args_impl(context, keep_tanything, loc, case, ty_args, constraints)
}

/// Instantiates a type, applying constraints to type arguments, and binding type arguments to
/// type variables
/// keep_tanything is an annoying case to handle macro signature checking, were we want to delay
/// instantiating Anything to a type varabile until _after_ the macro is expanded
fn instantiate_impl(context: &mut Context, keep_tanything: bool, sp!(loc, t_): Type) -> Type {
    use Type_::*;
    let it_ = match t_ {
        x @ (Unit | UnresolvedError | Void) => x,
        Anything => {
            if keep_tanything {
                Anything
            } else {
                make_tvar(context, loc).value
            }
        }

        Ref(mut_, b) => {
            let inner = *b;
            context.add_base_type_constraint(loc, "Invalid reference type", inner.clone());
            Ref(
                mut_,
                Box::new(instantiate_impl(context, keep_tanything, inner)),
            )
        }
        Apply(abilities_opt, n, ty_args) => {
            instantiate_apply_impl(context, keep_tanything, loc, abilities_opt, n, ty_args)
        }
        Fun(args, result) => Fun(
            args.into_iter()
                .map(|t| instantiate_impl(context, keep_tanything, t))
                .collect(),
            Box::new(instantiate_impl(context, keep_tanything, *result)),
        ),
        x @ Param(_) => x,
        // instantiating a var really shouldn't happen... but it does because of macro expansion
        // We expand macros before type checking, but after the arguments to the macro are type
        // checked (otherwise we couldn't properly do method syntax macros). As a result, we are
        // substituting type variables into the macro body, and might hit one while expanding a
        // type in the macro where a type parameter's argument had a type variable.
        x @ Var(_) => x,
    };
    sp(loc, it_)
}

// abilities_opt is expected to be None for non primitive types
fn instantiate_apply_impl(
    context: &mut Context,
    keep_tanything: bool,
    loc: Loc,
    abilities_opt: Option<AbilitySet>,
    n: TypeName,
    ty_args: Vec<Type>,
) -> Type_ {
    let tparam_constraints: Vec<AbilitySet> = match &n {
        sp!(nloc, N::TypeName_::Builtin(b)) => b.value.tparam_constraints(*nloc),
        sp!(_, N::TypeName_::Multiple(len)) => {
            debug_assert!(abilities_opt.is_none(), "ICE instantiated expanded type");
            (0..*len).map(|_| AbilitySet::empty()).collect()
        }
        sp!(_, N::TypeName_::ModuleType(m, n)) => {
            context.emit_warning_if_deprecated(m, n.0, None);
            debug_assert!(abilities_opt.is_none(), "ICE instantiated expanded type");
            let tps = match context.datatype_kind(m, n) {
                DatatypeKind::Struct => context.struct_tparams(m, n),
                DatatypeKind::Enum => context.enum_tparams(m, n),
            };
            tps.iter().map(|tp| tp.param.abilities.clone()).collect()
        }
    };

    let tys = instantiate_type_args_impl(
        context,
        keep_tanything,
        loc,
        TArgCase::Apply(&n.value),
        ty_args,
        tparam_constraints,
    );
    Type_::Apply(abilities_opt, n, tys)
}

// The type arguments are bound to type variables after intantiation
// i.e. vec<t1, ..., tn> ~> vec<a1, ..., an> s.t a1 => t1, ... , an => tn
// This might be needed for any variance case, and I THINK that it should be fine without it
// BUT I'm adding it as a safeguard against instantiating twice. Can always remove once this
// stabilizes
fn instantiate_type_args_impl(
    context: &mut Context,
    keep_tanything: bool,
    loc: Loc,
    case: TArgCase,
    mut ty_args: Vec<Type>,
    constraints: Vec<AbilitySet>,
) -> Vec<Type> {
    assert!(ty_args.len() == constraints.len());
    let locs_constraints = constraints
        .into_iter()
        .zip(&ty_args)
        .map(|(abilities, t)| (t.loc, abilities))
        .collect();
    let tvar_case = match case {
        TArgCase::Apply(TypeName_::Multiple(_)) => {
            TVarCase::Single("Invalid expression list type argument".to_owned())
        }
        TArgCase::Fun
        | TArgCase::Apply(TypeName_::Builtin(_))
        | TArgCase::Apply(TypeName_::ModuleType(_, _)) => TVarCase::Base,
        TArgCase::Macro => TVarCase::Macro,
    };
    // TODO in many cases we likely immediatley fill these type variables in with the type
    // arguments. We could maybe just not create them in the first place in some instances
    let tvars = make_tparams(context, loc, tvar_case, locs_constraints);
    ty_args = ty_args
        .into_iter()
        .map(|t| instantiate_impl(context, keep_tanything, t))
        .collect();

    assert!(ty_args.len() == tvars.len());
    let mut res = vec![];
    let subst = std::mem::replace(&mut context.subst, /* dummy value */ Subst::empty());
    context.subst = tvars
        .into_iter()
        .zip(ty_args)
        .fold(subst, |subst, (tvar, ty_arg)| {
            // tvar is just a type variable, so shouldn't throw ever...
            let (subst, t) = join(&mut context.tvar_counter, subst, &tvar, &ty_arg)
                .ok()
                .unwrap();
            res.push(t);
            subst
        });
    res
}

fn check_type_argument_arity<F: FnOnce() -> String>(
    context: &mut Context,
    loc: Loc,
    name_f: F,
    mut ty_args: Vec<Type>,
    tparam_constraints: &[AbilitySet],
) -> Vec<Type> {
    let args_len = ty_args.len();
    let arity = tparam_constraints.len();
    if args_len != arity {
        let code = if args_len < arity {
            NameResolution::TooFewTypeArguments
        } else {
            NameResolution::TooManyTypeArguments
        };
        let msg = format!(
            "Invalid instantiation of '{}'. Expected {} type argument(s) but got {}",
            name_f(),
            arity,
            args_len
        );
        context.add_diag(diag!(code, (loc, msg)));
    }

    while ty_args.len() > arity {
        ty_args.pop();
    }

    while ty_args.len() < arity {
        ty_args.push(context.error_type(loc));
    }

    ty_args
}

enum TVarCase {
    Single(String),
    Base,
    Macro,
}

enum TArgCase<'a> {
    Apply(&'a TypeName_),
    Fun,
    Macro,
}

fn make_tparams(
    context: &mut Context,
    loc: Loc,
    case: TVarCase,
    tparam_constraints: Vec<(Loc, AbilitySet)>,
) -> Vec<Type> {
    tparam_constraints
        .into_iter()
        .map(|(vloc, constraint)| {
            let tvar = make_tvar(context, vloc);
            context.add_ability_set_constraint(loc, None::<String>, tvar.clone(), constraint);
            match &case {
                TVarCase::Single(msg) => context.add_single_type_constraint(loc, msg, tvar.clone()),
                TVarCase::Base => {
                    context.add_base_type_constraint(loc, "Invalid type argument", tvar.clone())
                }
                TVarCase::Macro => (),
            };
            tvar
        })
        .collect()
}

// used in macros to make the signatures consistent with the bodies, in that we don't check
// constraints until application
pub fn give_tparams_all_abilities(sp!(_, ty_): &mut Type) {
    use Type_ as T;
    match ty_ {
        T::Unit | T::Var(_) | T::UnresolvedError | T::Anything | T::Void => (),
        T::Ref(_, inner) => give_tparams_all_abilities(inner),
        T::Apply(_, _, ty_args) => {
            for ty_arg in ty_args {
                give_tparams_all_abilities(ty_arg)
            }
        }
        T::Fun(args, ret) => {
            for arg in args {
                give_tparams_all_abilities(arg)
            }
            give_tparams_all_abilities(ret)
        }
        T::Param(_) => *ty_ = T::Anything,
    }
}

//**************************************************************************************************
// Subtype and joining
//**************************************************************************************************

#[derive(Debug)]
pub enum TypingError {
    SubtypeError(Box<Type>, Box<Type>),
    Incompatible(Box<Type>, Box<Type>),
    InvariantError(Box<Type>, Box<Type>),
    ArityMismatch(usize, Box<Type>, usize, Box<Type>),
    FunArityMismatch(usize, Box<Type>, usize, Box<Type>),
    RecursiveType(Loc),
}

#[derive(Clone, Copy, Debug)]
enum TypingCase {
    Join,
    Invariant,
    Subtype,
}

pub fn subtype(
    counter: &mut TVarCounter,
    subst: Subst,
    lhs: &Type,
    rhs: &Type,
) -> Result<(Subst, Type), TypingError> {
    join_impl(counter, subst, TypingCase::Subtype, lhs, rhs)
}

pub fn join(
    counter: &mut TVarCounter,
    subst: Subst,
    lhs: &Type,
    rhs: &Type,
) -> Result<(Subst, Type), TypingError> {
    join_impl(counter, subst, TypingCase::Join, lhs, rhs)
}

pub fn invariant(
    counter: &mut TVarCounter,
    subst: Subst,
    lhs: &Type,
    rhs: &Type,
) -> Result<(Subst, Type), TypingError> {
    join_impl(counter, subst, TypingCase::Invariant, lhs, rhs)
}

fn join_impl(
    counter: &mut TVarCounter,
    mut subst: Subst,
    case: TypingCase,
    lhs: &Type,
    rhs: &Type,
) -> Result<(Subst, Type), TypingError> {
    use Type_::*;
    use TypeName_::*;
    use TypingCase::*;
    match (lhs, rhs) {
        (sp!(_, Anything), other) | (other, sp!(_, Anything)) => Ok((subst, other.clone())),

        (sp!(_, Unit), sp!(loc, Unit)) => Ok((subst, sp(*loc, Unit))),

        (sp!(_, Void), sp!(loc, Void)) if matches!(case, Join | Invariant) => {
            Ok((subst, sp(*loc, Void)))
        }
        (sp!(_, Void), other) if matches!(case, Subtype) => Ok((subst, other.clone())),

        (sp!(loc1, Ref(mut1, t1)), sp!(loc2, Ref(mut2, t2))) => {
            let (loc, mut_) = match (case, mut1, mut2) {
                (Join, _, _) => {
                    // if 1 is imm and 2 is mut, use loc1. Else, loc2
                    let loc = if !*mut1 && *mut2 { *loc1 } else { *loc2 };
                    (loc, *mut1 && *mut2)
                }
                (Invariant, mut1, mut2) if mut1 == mut2 => (*loc1, *mut1),
                (Invariant, _mut1, _mut2) => {
                    return Err(TypingError::InvariantError(
                        Box::new(lhs.clone()),
                        Box::new(rhs.clone()),
                    ));
                }
                // imm <: imm
                // mut <: imm
                (Subtype, false, false) | (Subtype, true, false) => (*loc2, false),
                // mut <: mut
                (Subtype, true, true) => (*loc2, true),
                // imm <\: mut
                (Subtype, false, true) => {
                    return Err(TypingError::SubtypeError(
                        Box::new(lhs.clone()),
                        Box::new(rhs.clone()),
                    ));
                }
            };
            let (subst, t) = join_impl(counter, subst, case, t1, t2)?;
            Ok((subst, sp(loc, Ref(mut_, Box::new(t)))))
        }
        (sp!(_, Param(TParam { id: id1, .. })), sp!(_, Param(TParam { id: id2, .. })))
            if id1 == id2 =>
        {
            Ok((subst, rhs.clone()))
        }
        (sp!(_, Apply(_, sp!(_, Multiple(n1)), _)), sp!(_, Apply(_, sp!(_, Multiple(n2)), _)))
            if n1 != n2 =>
        {
            Err(TypingError::ArityMismatch(
                *n1,
                Box::new(lhs.clone()),
                *n2,
                Box::new(rhs.clone()),
            ))
        }
        (sp!(_, Apply(k1, n1, tys1)), sp!(loc, Apply(k2, n2, tys2))) if n1 == n2 => {
            assert!(
                k1 == k2,
                "ICE failed naming: {:#?}kind != {:#?}kind. {:#?} !=  {:#?}",
                n1,
                n2,
                k1,
                k2
            );
            let (subst, tys) = join_impl_types(counter, subst, case, tys1, tys2)?;
            Ok((subst, sp(*loc, Apply(k2.clone(), *n2, tys))))
        }
        (sp!(_, Fun(a1, _)), sp!(_, Fun(a2, _))) if a1.len() != a2.len() => {
            Err(TypingError::FunArityMismatch(
                a1.len(),
                Box::new(lhs.clone()),
                a2.len(),
                Box::new(rhs.clone()),
            ))
        }
        (sp!(_, Fun(a1, r1)), sp!(loc, Fun(a2, r2))) => {
            // TODO this is going to likely lead to some strange error locations/messages
            // since the RHS in subtyping is currently assumed to be an annotation
            let (subst, args) = match case {
                Join | Invariant => join_impl_types(counter, subst, case, a1, a2)?,
                Subtype => join_impl_types(counter, subst, case, a2, a1)?,
            };
            let (subst, result) = join_impl(counter, subst, case, r1, r2)?;
            Ok((subst, sp(*loc, Fun(args, Box::new(result)))))
        }
        (sp!(loc1, Var(id1)), sp!(loc2, Var(id2))) => {
            if *id1 == *id2 {
                Ok((subst, sp(*loc2, Var(*id2))))
            } else {
                join_tvar(counter, subst, case, *loc1, *id1, *loc2, *id2)
            }
        }
        (sp!(loc, Var(id)), other) if subst.get(*id).is_none() => {
            if join_bind_tvar(&mut subst, *loc, *id, other.clone())? {
                Ok((subst, sp(*loc, Var(*id))))
            } else {
                Err(TypingError::Incompatible(
                    Box::new(sp(*loc, Var(*id))),
                    Box::new(other.clone()),
                ))
            }
        }
        (other, sp!(loc, Var(id))) if subst.get(*id).is_none() => {
            if join_bind_tvar(&mut subst, *loc, *id, other.clone())? {
                Ok((subst, sp(*loc, Var(*id))))
            } else {
                Err(TypingError::Incompatible(
                    Box::new(other.clone()),
                    Box::new(sp(*loc, Var(*id))),
                ))
            }
        }
        (sp!(loc, Var(id)), other) => {
            let new_tvar = counter.next();
            subst.insert(new_tvar, other.clone());
            join_tvar(counter, subst, case, *loc, *id, other.loc, new_tvar)
        }
        (other, sp!(loc, Var(id))) => {
            let new_tvar = counter.next();
            subst.insert(new_tvar, other.clone());
            join_tvar(counter, subst, case, other.loc, new_tvar, *loc, *id)
        }

        (sp!(_, UnresolvedError), other) | (other, sp!(_, UnresolvedError)) => {
            Ok((subst, other.clone()))
        }
        _ => Err(TypingError::Incompatible(
            Box::new(lhs.clone()),
            Box::new(rhs.clone()),
        )),
    }
}

fn join_impl_types(
    counter: &mut TVarCounter,
    mut subst: Subst,
    case: TypingCase,
    tys1: &[Type],
    tys2: &[Type],
) -> Result<(Subst, Vec<Type>), TypingError> {
    // if tys1.len() != tys2.len(), we will get an error when instantiating the type elsewhere
    // as all types are instantiated as a sanity check
    let mut tys = vec![];
    for (ty1, ty2) in tys1.iter().zip(tys2) {
        let (nsubst, t) = join_impl(counter, subst, case, ty1, ty2)?;
        subst = nsubst;
        tys.push(t)
    }
    Ok((subst, tys))
}

pub fn join_var_constraints(
    lhs: Option<VarConstraint>,
    rhs: Option<VarConstraint>,
) -> Result<Option<VarConstraint>, TypingError> {
    use VarConstraint as C;
    match (&lhs, &rhs) {
        // divergnce propagates only if both arms are divergent; otherwise, use the other constraint
        (Some(C::Divergent(_)), Some(C::Divergent(_))) => Ok(rhs),
        (Some(C::Divergent(_)), other) | (other, Some(C::Divergent(_))) => Ok(other.clone()),

        // number constraints propagates if either arms is numeric
        (Some(C::Num(_)), Some(C::Num(_))) => Ok(rhs),
        (Some(C::Num(_)), None) => Ok(lhs),
        (None, Some(C::Num(_))) => Ok(rhs),

        // none case
        (None, None) => Ok(None),
    }
}

fn join_tvar(
    counter: &mut TVarCounter,
    mut subst: Subst,
    case: TypingCase,
    loc1: Loc,
    id1: TVar,
    loc2: Loc,
    id2: TVar,
) -> Result<(Subst, Type), TypingError> {
    use Type_::*;
    let last_id1 = forward_tvar(&subst, id1);
    let last_id2 = forward_tvar(&subst, id2);
    let ty1 = match subst.get(last_id1) {
        None => sp(loc1, Anything),
        Some(t) => t.clone(),
    };
    let ty2 = match subst.get(last_id2) {
        None => sp(loc2, Anything),
        Some(t) => t.clone(),
    };

    let new_tvar = counter.next();

    // join constraints
    let constraints_1 = subst.tvar_constraints.get(&last_id1).cloned();
    let constraints_2 = subst.tvar_constraints.get(&last_id2).cloned();
    let new_constraint_opt = join_var_constraints(constraints_1, constraints_2)?;
    subst.set_constraint_opt(new_tvar, new_constraint_opt);

    subst.insert(last_id1, sp(loc1, Var(new_tvar)));
    subst.insert(last_id2, sp(loc2, Var(new_tvar)));

    let (mut subst, new_ty) = join_impl(counter, subst, case, &ty1, &ty2)?;
    match subst.get(new_tvar) {
        Some(sp!(tloc, _)) => Err(TypingError::RecursiveType(*tloc)),
        None => {
            if join_bind_tvar(&mut subst, loc2, new_tvar, new_ty)? {
                Ok((subst, sp(loc2, Var(new_tvar))))
            } else {
                let ty1 = match ty1 {
                    sp!(loc, Anything) => sp(loc, Var(id1)),
                    t => t,
                };
                let ty2 = match ty2 {
                    sp!(loc, Anything) => sp(loc, Var(id2)),
                    t => t,
                };
                Err(TypingError::Incompatible(Box::new(ty1), Box::new(ty2)))
            }
        }
    }
}

fn forward_tvar(subst: &Subst, id: TVar) -> TVar {
    let mut cur = id;
    loop {
        match subst.get(cur) {
            Some(sp!(_, Type_::Var(next))) => cur = *next,
            Some(_) | None => break cur,
        }
    }
}

fn join_bind_tvar(subst: &mut Subst, loc: Loc, tvar: TVar, ty: Type) -> Result<bool, TypingError> {
    assert!(
        subst.get(tvar).is_none(),
        "ICE join_bind_tvar called on bound tvar"
    );

    fn occurs_check(target_tvar: &TVar, sp!(_, t_): &Type) -> bool {
        use Type_ as T;
        match t_ {
            T::Var(v) => v == target_tvar,
            T::Ref(_, inner) => occurs_check(target_tvar, inner),
            T::Apply(_, _, inners) => inners.iter().any(|inner| occurs_check(target_tvar, inner)),
            T::Fun(inner_args, inner_ret) => {
                inner_args.iter().any(|arg| occurs_check(target_tvar, arg))
                    || occurs_check(target_tvar, inner_ret)
            }
            T::Unit | T::Param(_) | T::Anything | T::Void | T::UnresolvedError => false,
        }
    }

    // check not necessary for soundness but improves error message structure
    if !check_num_tvar(subst, loc, &tvar, &ty) {
        return Ok(false);
    }

    if occurs_check(&tvar, &ty) {
        return Err(TypingError::RecursiveType(loc));
    }

    match &ty.value {
        Type_::Anything => (),
        _ => subst.insert(tvar, ty),
    }
    Ok(true)
}

fn check_num_tvar(subst: &Subst, _loc: Loc, tvar: &TVar, ty: &Type) -> bool {
    !subst.is_num_var(tvar) || check_num_tvar_(subst, ty)
}

fn check_num_tvar_(subst: &Subst, ty: &Type) -> bool {
    use Type_::*;
    match &ty.value {
        UnresolvedError | Anything => true,
        Apply(_, sp!(_, TypeName_::Builtin(sp!(_, bt))), _) => bt.is_numeric(),

        Var(v) => {
            let last_tvar = forward_tvar(subst, *v);
            match subst.get(last_tvar) {
                Some(sp!(_, Var(_))) => unreachable!(),
                None => subst.is_num_var(&last_tvar),
                Some(t) => check_num_tvar_(subst, t),
            }
        }
        _ => false,
    }
}
