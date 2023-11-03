// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    debug_display, diag,
    diagnostics::{codes::NameResolution, Diagnostic},
    expansion::ast::{AbilitySet, ModuleIdent, ModuleIdent_, Visibility},
    naming::ast::{
        self as N, BlockLabel, BuiltinTypeName_, DatatypeTypeParameter, EnumDefinition,
        ResolvedUseFuns, StructDefinition, TParam, TParamID, TVar, Type, TypeName, TypeName_,
        Type_, UseFunKind, Var,
    },
    parser::ast::{
        Ability_, ConstantName, DatatypeName, Field, FunctionName, Mutability, VariantName,
        ENTRY_MODIFIER,
    },
    shared::{known_attributes::TestingAttribute, program_info::*, unique_map::UniqueMap, *},
    FullyCompiledProgram,
};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::collections::{BTreeMap, BTreeSet, HashMap};

//**************************************************************************************************
// Context
//**************************************************************************************************

struct UseFunsScope {
    count: usize,
    use_funs: ResolvedUseFuns,
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

pub struct Local {
    pub mut_: Mutability,
    pub ty: Type,
    pub used_mut: Option<Loc>,
}

pub struct Context<'env> {
    pub modules: NamingProgramInfo,
    pub env: &'env mut CompilationEnv,

    use_funs: Vec<UseFunsScope>,
    pub current_package: Option<Symbol>,
    pub current_module: Option<ModuleIdent>,
    pub current_function: Option<FunctionName>,
    pub return_type: Option<Type>,
    locals: UniqueMap<Var, Local>,

    pub subst: Subst,
    pub constraints: Constraints,

    named_block_map: BTreeMap<BlockLabel, Type>,

    /// collects all friends that should be added over the course of 'public(package)' calls
    /// structured as (defining module, new friend, location) where `new friend` is usually the
    /// context's current module. Note there may be more than one location in practice, but
    /// tracking a single one is sufficient for error reporting.
    pub new_friends: BTreeSet<(ModuleIdent, Loc)>,
    /// collects all used module members (functions and constants) but it's a superset of these in
    /// that it may contain other identifiers that do not in fact represent a function or a constant
    pub used_module_members: BTreeMap<ModuleIdent_, BTreeSet<Symbol>>,
}

impl UseFunsScope {
    pub fn global(info: &NamingProgramInfo) -> Self {
        let count = 1;
        let mut use_funs = BTreeMap::new();
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
                    !use_funs.contains_key(tn),
                    "ICE public methods should have been filtered to the defining module.
                    tn: {tn}.
                    prev: {}
                    new: {}",
                    debug_display!((tn, (use_funs.get(tn).unwrap()))),
                    debug_display!((tn, &public_methods))
                );
                use_funs.insert(tn.clone(), public_methods);
            }
        }
        UseFunsScope { count, use_funs }
    }
}

impl<'env> Context<'env> {
    pub fn new(
        env: &'env mut CompilationEnv,
        _pre_compiled_lib: Option<&FullyCompiledProgram>,
        info: NamingProgramInfo,
    ) -> Self {
        let global_use_funs = UseFunsScope::global(&info);
        Context {
            use_funs: vec![global_use_funs],
            subst: Subst::empty(),
            current_package: None,
            current_module: None,
            current_function: None,
            return_type: None,
            constraints: vec![],
            locals: UniqueMap::new(),
            modules: info,
            named_block_map: BTreeMap::new(),
            env,
            new_friends: BTreeSet::new(),
            used_module_members: BTreeMap::new(),
        }
    }

    pub fn add_use_funs_scope(&mut self, new_scope: N::UseFuns) {
        let N::UseFuns {
            resolved: new_scope,
            implicit_candidates,
        } = new_scope;
        assert!(
            implicit_candidates.is_empty(),
            "ICE use fun candidates should have been resolved"
        );
        let cur = self.use_funs.last_mut().unwrap();
        if new_scope.is_empty() {
            cur.count += 1;
            return;
        }
        self.use_funs.push(UseFunsScope {
            count: 1,
            use_funs: new_scope,
        })
    }

    pub fn pop_use_funs_scope(&mut self) {
        let cur = self.use_funs.last_mut().unwrap();
        if cur.count > 1 {
            cur.count -= 1;
            return;
        }
        let UseFunsScope { use_funs, .. } = self.use_funs.pop().unwrap();
        for (tn, methods) in use_funs {
            let unused = methods.iter().filter(|(_, _, uf)| !uf.used);
            for (_, method, use_fun) in unused {
                let N::UseFun {
                    loc,
                    kind,
                    attributes: _,
                    is_public: _,
                    target_function: _,
                    used: _,
                } = use_fun;
                let msg = match kind {
                    UseFunKind::Explicit => {
                        format!("Unused 'use fun' of '{tn}.{method}'. Consider removing it")
                    }
                    UseFunKind::UseAlias => {
                        format!("Unused 'use' of alias '{method}'. Consider removing it")
                    }
                    UseFunKind::FunctionDeclaration => {
                        panic!("ICE function declaration use funs should never be added to use fun")
                    }
                };
                self.env.add_diag(diag!(UnusedItem::Alias, (*loc, msg)))
            }
        }
    }

    pub fn find_method_and_mark_used(
        &mut self,
        tn: &TypeName,
        method: Name,
    ) -> Option<(ModuleIdent, FunctionName)> {
        self.use_funs.iter_mut().rev().find_map(|scope| {
            let use_fun = scope.use_funs.get_mut(tn)?.get_mut(&method)?;
            use_fun.used = true;
            Some(use_fun.target_function)
        })
    }

    pub fn reset_for_module_item(&mut self) {
        self.named_block_map = BTreeMap::new();
        self.return_type = None;
        self.locals = UniqueMap::new();
        self.subst = Subst::empty();
        self.constraints = Constraints::new();
        self.current_function = None;
    }

    pub fn error_type(&mut self, loc: Loc) -> Type {
        sp(loc, Type_::UnresolvedError)
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

    pub fn declare_local(&mut self, mut_: Mutability, var: Var, ty: Type) {
        let local = Local {
            mut_,
            ty,
            used_mut: None,
        };
        self.locals.add(var, local).unwrap()
    }

    pub fn get_local_type(&mut self, var: &Var) -> Type {
        // should not fail, already checked in naming
        self.locals.get(var).unwrap().ty.clone()
    }

    pub fn mark_mutable_usage(&mut self, loc: Loc, var: &Var) -> (Loc, Mutability) {
        // should not fail, already checked in naming
        let decl_loc = *self.locals.get_loc(var).unwrap();
        let local = self.locals.get_mut(var).unwrap();
        local.used_mut = Some(loc);
        (decl_loc, local.mut_)
    }

    pub fn take_locals(&mut self) -> UniqueMap<Var, Local> {
        std::mem::take(&mut self.locals)
    }

    pub fn is_current_module(&self, m: &ModuleIdent) -> bool {
        match &self.current_module {
            Some(curm) => curm == m,
            None => false,
        }
    }

    pub fn is_current_function(&self, m: &ModuleIdent, f: &FunctionName) -> bool {
        self.is_current_module(m) && matches!(&self.current_function, Some(curf) if curf == f)
    }

    pub fn current_package(&self) -> Option<Symbol> {
        self.current_module
            .as_ref()
            .and_then(|mident| self.module_info(mident).package)
    }

    // `loc` indicates the location that caused the add to occur
    fn record_current_module_as_friend(&mut self, m: &ModuleIdent, loc: Loc) {
        if matches!(self.current_module, Some(current_mident) if m != &current_mident) {
            self.new_friends.insert((*m, loc));
        }
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

    /// current_module.is_test_only || current_function.is_test_only || current_function.is_test
    fn is_testing_context(&self) -> bool {
        self.current_module.as_ref().is_some_and(|m| {
            let minfo = self.module_info(m);
            let is_test_only = minfo.attributes.is_test_or_test_only();
            is_test_only
                || self.current_function.as_ref().is_some_and(|f| {
                    let finfo = minfo.functions.get(f).unwrap();
                    finfo.attributes.is_test_or_test_only()
                })
        })
    }

    fn module_info(&self, m: &ModuleIdent) -> &ModuleInfo {
        self.modules.module(m)
    }

    fn struct_definition(&self, m: &ModuleIdent, n: &DatatypeName) -> &StructDefinition {
        self.modules.struct_definition(m, n)
    }

    pub fn struct_declared_abilities(&self, m: &ModuleIdent, n: &DatatypeName) -> &AbilitySet {
        self.modules.struct_declared_abilities(m, n)
    }

    pub fn struct_declared_loc(&self, m: &ModuleIdent, n: &DatatypeName) -> Loc {
        self.modules.struct_declared_loc(m, n)
    }

    pub fn struct_tparams(&self, m: &ModuleIdent, n: &DatatypeName) -> &Vec<DatatypeTypeParameter> {
        self.modules.struct_type_parameters(m, n)
    }

    fn enum_definition(&self, m: &ModuleIdent, n: &DatatypeName) -> &EnumDefinition {
        self.modules.enum_definition(m, n)
    }

    pub fn enum_declared_abilities(&self, m: &ModuleIdent, n: &DatatypeName) -> &AbilitySet {
        self.modules.enum_declared_abilities(m, n)
    }

    pub fn enum_declared_loc(&self, m: &ModuleIdent, n: &DatatypeName) -> Loc {
        self.modules.enum_declared_loc(m, n)
    }

    pub fn enum_tparams(&self, m: &ModuleIdent, n: &DatatypeName) -> &Vec<DatatypeTypeParameter> {
        self.modules.enum_type_parameters(m, n)
    }

    pub fn datatype_kind(&self, m: &ModuleIdent, n: &DatatypeName) -> DatatypeKind {
        self.modules.datatype_kind(m, n)
    }

    fn function_info(&self, m: &ModuleIdent, n: &FunctionName) -> &FunctionInfo {
        self.modules.function_info(m, n)
    }

    fn constant_info(&mut self, m: &ModuleIdent, n: &ConstantName) -> &ConstantInfo {
        let constants = &self.module_info(m).constants;
        constants.get(n).expect("ICE should have failed in naming")
    }

    // pass in a location for a better error location
    pub fn named_block_type(&mut self, name: BlockLabel, loc: Loc) -> Type {
        if let Some(ty) = self.named_block_map.get(&name) {
            ty.clone()
        } else {
            let new_type = make_tvar(self, loc);
            self.named_block_map.insert(name, new_type.clone());
            new_type
        }
    }

    pub fn named_block_type_opt(&self, name: BlockLabel) -> Option<Type> {
        self.named_block_map.get(&name).cloned()
    }
}

//**************************************************************************************************
// Subst
//**************************************************************************************************

#[derive(Clone, Debug)]
pub struct Subst {
    tvars: HashMap<TVar, Type>,
    num_vars: HashMap<TVar, Loc>,
}

impl Subst {
    pub fn empty() -> Self {
        Self {
            tvars: HashMap::new(),
            num_vars: HashMap::new(),
        }
    }

    pub fn insert(&mut self, tvar: TVar, bt: Type) {
        self.tvars.insert(tvar, bt);
    }

    pub fn get(&self, tvar: TVar) -> Option<&Type> {
        self.tvars.get(&tvar)
    }

    pub fn new_num_var(&mut self, loc: Loc) -> TVar {
        let tvar = TVar::next();
        assert!(self.num_vars.insert(tvar, loc).is_none());
        tvar
    }

    pub fn set_num_var(&mut self, tvar: TVar, loc: Loc) {
        self.num_vars.entry(tvar).or_insert(loc);
        if let Some(sp!(_, Type_::Var(next))) = self.get(tvar) {
            let next = *next;
            self.set_num_var(next, loc)
        }
    }

    pub fn is_num_var(&self, tvar: TVar) -> bool {
        self.num_vars.contains_key(&tvar)
    }
}

impl ast_debug::AstDebug for Subst {
    fn ast_debug(&self, w: &mut ast_debug::AstWriter) {
        let Subst { tvars, num_vars } = self;

        w.write("tvars:");
        w.indent(4, |w| {
            let mut tvars = tvars.iter().collect::<Vec<_>>();
            tvars.sort_by_key(|(v, _)| *v);
            for (tvar, bt) in tvars {
                w.write(&format!("{:?} => ", tvar));
                bt.ast_debug(w);
                w.new_line();
            }
        });
        w.write("num_vars:");
        w.indent(4, |w| {
            let mut num_vars = num_vars.keys().collect::<Vec<_>>();
            num_vars.sort();
            for tvar in num_vars {
                w.writeln(&format!("{:?}", tvar))
            }
        })
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
        UnresolvedError | Anything => "_".to_string(),
        Unit => "()".to_string(),
        Var(id) => {
            let last_id = forward_tvar(subst, *id);
            match subst.get(last_id) {
                Some(sp!(_, Var(_))) => unreachable!(),
                Some(t) => error_format_nested(t, subst),
                None if nested && subst.is_num_var(last_id) => "{integer}".to_string(),
                None if subst.is_num_var(last_id) => return "integer".to_string(),
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
        Param(tp) => tp.user_specified_name.value.to_string(),
        Ref(mut_, ty) => format!(
            "&{}{}",
            if *mut_ { "mut " } else { "" },
            error_format_nested(ty, subst)
        ),
    };
    if nested {
        res
    } else {
        format!("'{}'", res)
    }
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
        T::UnresolvedError | T::Anything => AbilitySet::all(loc),
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
    }
}

// Returns
// - the declared location where abilities are added (if applicable)
// - the set of declared abilities
// - its type arguments
fn debug_abilities_info(context: &Context, ty: &Type) -> (Option<Loc>, AbilitySet, Vec<Type>) {
    use Type_ as T;
    let loc = ty.loc;
    match &ty.value {
        T::Unit | T::Ref(_, _) => (None, AbilitySet::references(loc), vec![]),
        T::Var(_) => panic!("ICE call unfold_type before debug_abilities_info"),
        T::UnresolvedError | T::Anything => (None, AbilitySet::all(loc), vec![]),
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
    }
}

pub fn make_num_tvar(context: &mut Context, loc: Loc) -> Type {
    let tvar = context.subst.new_num_var(loc);
    sp(loc, Type_::Var(tvar))
}

pub fn make_tvar(_context: &mut Context, loc: Loc) -> Type {
    sp(loc, Type_::Var(TVar::next()))
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
                _ => panic!("ICE instantiate_apply returned non Apply"),
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
        N::StructFields::Defined(m) => {
            N::StructFields::Defined(m.ref_map(|_, (idx, field_ty)| {
                (*idx, subst_tparams(tparam_subst, field_ty.clone()))
            }))
        }
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
            context.env.add_diag(diag!(
                NameResolution::UnboundField,
                (loc, msg),
                (nloc, "Struct declared 'native' here")
            ));
            return context.error_type(loc);
        }
        N::StructFields::Defined(m) => m,
    };
    match fields_map.get(field).cloned() {
        None => {
            context.env.add_diag(diag!(
                NameResolution::UnboundField,
                (loc, format!("Unbound field '{}' in '{}::{}'", field, m, n)),
            ));
            context.error_type(loc)
        }
        Some((_, field_ty)) => {
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

// ty_args should come from make_struct_type
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
        N::VariantFields::Defined(m) => {
            N::VariantFields::Defined(m.ref_map(|_, (idx, field_ty)| {
                (*idx, subst_tparams(tparam_subst, field_ty.clone()))
            }))
        }
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
    let in_current_module = Some(m) == context.current_module.as_ref();
    let (defined_loc, signature) = {
        let ConstantInfo {
            attributes: _,
            defined_loc,
            signature,
        } = context.constant_info(m, c);
        (*defined_loc, signature.clone())
    };
    if !in_current_module {
        let msg = format!("Invalid access of '{}::{}'", m, c);
        let internal_msg = "Constants are internal to their module, and cannot can be accessed \
                            outside of their module";
        context.env.add_diag(diag!(
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
) -> Option<(
    Loc,
    ModuleIdent,
    FunctionName,
    Vec<Type>,
    Vec<(Var, Type)>,
    Type,
)> {
    let target_function_opt = context.find_method_and_mark_used(tn, method);
    // try to find a function in the defining module for errors
    let Some((target_m, target_f)) = target_function_opt else {
        let lhs_ty_str = error_format_nested(lhs_ty, &Subst::empty());
        let defining_module = match &tn.value {
            TypeName_::Multiple(_) => panic!("ICE method on tuple"),
            TypeName_::Builtin(sp!(_, bt_)) => context.env.primitive_definer(*bt_),
            TypeName_::ModuleType(m, _) => Some(m),
        };
        let finfo_opt = defining_module.and_then(|m| {
            let finfo = context
                .modules
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
                    let tys_str = error_format(&ty, &Subst::empty());
                    format!("but it has a different type for its first argument, {tys_str}")
                }
                None => "but it takes no arguments".to_owned(),
            };
            let msg = format!(
                "Invalid method call. \
                No known method '{method}' on type '{lhs_ty_str}'"
            );
            let fmsg = format!("The function '{m}::{method}' exists, {arg_msg}");
            context.env.add_diag(diag!(
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
            context.env.add_diag(diag!(
                TypeSafety::InvalidMethodCall,
                (loc, msg),
                (method.loc, fmsg)
            ));
        }
        return None;
    };

    let (defined_loc, ty_args, params, return_ty) =
        make_function_type(context, loc, &target_m, &target_f, ty_args_opt);

    Some((defined_loc, target_m, target_f, ty_args, params, return_ty))
}

pub fn make_function_type(
    context: &mut Context,
    loc: Loc,
    m: &ModuleIdent,
    f: &FunctionName,
    ty_args_opt: Option<Vec<Type>>,
) -> (Loc, Vec<Type>, Vec<(Var, Type)>, Type) {
    let in_current_module = match &context.current_module {
        Some(current) => m == current,
        None => false,
    };
    let constraints: Vec<_> = context
        .function_info(m, f)
        .signature
        .type_parameters
        .iter()
        .map(|tp| tp.abilities.clone())
        .collect();

    let ty_args = match ty_args_opt {
        None => {
            let locs_constraints = constraints.into_iter().map(|k| (loc, k)).collect();
            make_tparams(context, loc, TVarCase::Base, locs_constraints)
        }
        Some(ty_args) => {
            let ty_args = check_type_argument_arity(
                context,
                loc,
                || format!("{}::{}", m, f),
                ty_args,
                &constraints,
            );
            instantiate_type_args(context, loc, None, ty_args, constraints)
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
    let public_for_testing =
        public_testing_visibility(context.env, context.current_package, f, finfo.entry);
    let is_testing_context = context.is_testing_context();
    match finfo.visibility {
        _ if is_testing_context && public_for_testing.is_some() => (),
        Visibility::Internal if in_current_module => (),
        Visibility::Internal => {
            let internal_msg = format!(
                "This function is internal to its module. Only '{}', '{}', and '{}' functions can \
                 be called outside of their module",
                Visibility::PUBLIC,
                Visibility::FRIEND,
                Visibility::PACKAGE
            );
            visibility_error(
                context,
                public_for_testing,
                (loc, format!("Invalid call to '{}::{}'", m, f)),
                (defined_loc, internal_msg),
            );
        }
        Visibility::Package(loc)
            if in_current_module || context.current_module_shares_package_and_address(m) =>
        {
            context.record_current_module_as_friend(m, loc);
        }
        Visibility::Package(vis_loc) => {
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
                    .current_module
                    .map(|cur_module| cur_module.value.address.to_string())
                    .unwrap_or("<unknown addr>".to_string()),
                &context
                    .current_module
                    .and_then(|cur_module| context.module_info(&cur_module).package)
                    .map(|pkg_name| format!("{}", pkg_name))
                    .unwrap_or("<unknown package>".to_string())
            );
            visibility_error(
                context,
                public_for_testing,
                (loc, format!("Invalid call to '{}::{}'", m, f)),
                (vis_loc, internal_msg),
            );
        }
        Visibility::Friend(_) if in_current_module || context.current_module_is_a_friend_of(m) => {}
        Visibility::Friend(vis_loc) => {
            let internal_msg = format!(
                "This function can only be called from a 'friend' of module '{}'",
                m
            );
            visibility_error(
                context,
                public_for_testing,
                (loc, format!("Invalid call to '{}::{}'", m, f)),
                (vis_loc, internal_msg),
            );
        }
        Visibility::Public(_) => (),
    };
    (defined_loc, ty_args, params, return_ty)
}

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
    if !env.flags().is_testing() {
        return None;
    }

    // TODO support sui init functions
    // let flavor = env.package_config(package).flavor;
    // flavor == Flavor::Sui && callee_name.value() == INIT_FUNCTION_NAME
    callee_entry.map(PublicForTesting::Entry)
}

fn visibility_error(
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
    if context.env.flags().is_testing() {
        if let Some(case) = public_for_testing {
            let (test_loc, test_msg) = match case {
                PublicForTesting::Entry(entry_loc) => {
                    let entry_msg = format!(
                        "'{}' functions can be called in tests, \
                    but only from testing contexts, e.g. '#[{}]' or '#[{}]'",
                        ENTRY_MODIFIER,
                        TestingAttribute::TEST,
                        TestingAttribute::TEST_ONLY,
                    );
                    (entry_loc, entry_msg)
                }
            };
            diag.add_secondary_label((test_loc, test_msg))
        }
    }
    context.env.add_diag(diag)
}

//**************************************************************************************************
// Constraints
//**************************************************************************************************

pub fn solve_constraints(context: &mut Context) {
    use BuiltinTypeName_ as BT;
    let num_vars = context.subst.num_vars.clone();
    let mut subst = std::mem::replace(&mut context.subst, Subst::empty());
    for (num_var, loc) in num_vars {
        let tvar = sp(loc, Type_::Var(num_var));
        match unfold_type(&subst, tvar.clone()).value {
            Type_::UnresolvedError | Type_::Anything => {
                let next_subst = join(subst, &Type_::u64(loc), &tvar).unwrap().0;
                subst = next_subst;
            }
            _ => (),
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
    let ty_abilities = infer_abilities(&context.modules, &context.subst, ty.clone());

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
                let abilities = infer_abilities(&context.modules, &context.subst, ty_arg.clone());
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
        context.env.add_diag(diag)
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
    use TypeName_::*;
    use Type_::*;
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
            context.env.add_diag(diag!(
                TypeSafety::BuiltinOperation,
                (loc, format!("Invalid argument to '{}'", op)),
                (tloc, tmsg)
            ))
        }
    }
}

fn solve_base_type_constraint(context: &mut Context, loc: Loc, msg: String, ty: &Type) {
    use TypeName_::*;
    use Type_::*;
    let sp!(tyloc, unfolded_) = unfold_type(&context.subst, ty.clone());
    match unfolded_ {
        Var(_) => unreachable!(),
        Unit | Ref(_, _) | Apply(_, sp!(_, Multiple(_)), _) => {
            let tystr = error_format(ty, &context.subst);
            let tmsg = format!("Expected a single non-reference type, but found: {}", tystr);
            context.env.add_diag(diag!(
                TypeSafety::ExpectedBaseType,
                (loc, msg),
                (tyloc, tmsg)
            ))
        }
        UnresolvedError | Anything | Param(_) | Apply(_, _, _) => (),
    }
}

fn solve_single_type_constraint(context: &mut Context, loc: Loc, msg: String, ty: &Type) {
    use TypeName_::*;
    use Type_::*;
    let sp!(tyloc, unfolded_) = unfold_type(&context.subst, ty.clone());
    match unfolded_ {
        Var(_) => unreachable!(),
        Unit | Apply(_, sp!(_, Multiple(_)), _) => {
            let tmsg = format!(
                "Expected a single type, but found expression list type: {}",
                error_format(ty, &context.subst)
            );
            context.env.add_diag(diag!(
                TypeSafety::ExpectedSingleType,
                (loc, msg),
                (tyloc, tmsg)
            ))
        }
        UnresolvedError | Anything | Ref(_, _) | Param(_) | Apply(_, _, _) => (),
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
        x @ Unit | x @ UnresolvedError | x @ Anything => sp(loc, x),
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
    }
}

pub fn ready_tvars(subst: &Subst, sp!(loc, t_): Type) -> Type {
    use Type_::*;
    match t_ {
        x @ UnresolvedError | x @ Unit | x @ Anything | x @ Param(_) => sp(loc, x),
        Ref(mut_, t) => sp(loc, Ref(mut_, Box::new(ready_tvars(subst, *t)))),
        Apply(k, n, tys) => {
            let tys = tys.into_iter().map(|t| ready_tvars(subst, t)).collect();
            sp(loc, Apply(k, n, tys))
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

pub fn instantiate(context: &mut Context, sp!(loc, t_): Type) -> Type {
    use Type_::*;
    let it_ = match t_ {
        Unit => Unit,
        UnresolvedError => UnresolvedError,
        Anything => make_tvar(context, loc).value,
        Ref(mut_, b) => {
            let inner = *b;
            context.add_base_type_constraint(loc, "Invalid reference type", inner.clone());
            Ref(mut_, Box::new(instantiate(context, inner)))
        }
        Apply(abilities_opt, n, ty_args) => {
            instantiate_apply(context, loc, abilities_opt, n, ty_args)
        }
        x @ Param(_) => x,
        Var(_) => panic!("ICE instantiate type variable"),
    };
    sp(loc, it_)
}

// abilities_opt is expected to be None for non primitive types
fn instantiate_apply(
    context: &mut Context,
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
            debug_assert!(abilities_opt.is_none(), "ICE instantiated expanded type");
            let tps = match context.datatype_kind(m, n) {
                DatatypeKind::Struct => context.struct_tparams(m, n),
                DatatypeKind::Enum => context.enum_tparams(m, n),
            };
            tps.iter().map(|tp| tp.param.abilities.clone()).collect()
        }
    };

    let tys = instantiate_type_args(context, loc, Some(&n.value), ty_args, tparam_constraints);
    Type_::Apply(abilities_opt, n, tys)
}

// The type arguments are bound to type variables after intantiation
// i.e. vec<t1, ..., tn> ~> vec<a1, ..., an> s.t a1 => t1, ... , an => tn
// This might be needed for any variance case, and I THINK that it should be fine without it
// BUT I'm adding it as a safeguard against instantiating twice. Can always remove once this
// stabilizes
fn instantiate_type_args(
    context: &mut Context,
    loc: Loc,
    n: Option<&TypeName_>,
    mut ty_args: Vec<Type>,
    constraints: Vec<AbilitySet>,
) -> Vec<Type> {
    assert!(ty_args.len() == constraints.len());
    let locs_constraints = constraints
        .into_iter()
        .zip(&ty_args)
        .map(|(abilities, t)| (t.loc, abilities))
        .collect();
    let tvar_case = match n {
        Some(TypeName_::Multiple(_)) => {
            TVarCase::Single("Invalid expression list type argument".to_owned())
        }
        None | Some(TypeName_::Builtin(_)) | Some(TypeName_::ModuleType(_, _)) => TVarCase::Base,
    };
    let tvars = make_tparams(context, loc, tvar_case, locs_constraints);
    ty_args = ty_args
        .into_iter()
        .map(|t| instantiate(context, t))
        .collect();

    assert!(ty_args.len() == tvars.len());
    let mut res = vec![];
    let subst = std::mem::replace(&mut context.subst, /* dummy value */ Subst::empty());
    context.subst = tvars
        .into_iter()
        .zip(ty_args)
        .fold(subst, |subst, (tvar, ty_arg)| {
            // tvar is just a type variable, so shouldn't throw ever...
            let (subst, t) = join(subst, &tvar, &ty_arg).ok().unwrap();
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
        context.env.add_diag(diag!(code, (loc, msg)));
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
            };
            tvar
        })
        .collect()
}

//**************************************************************************************************
// Subtype and joining
//**************************************************************************************************

#[derive(Debug)]
pub enum TypingError {
    SubtypeError(Box<Type>, Box<Type>),
    Incompatible(Box<Type>, Box<Type>),
    ArityMismatch(usize, Box<Type>, usize, Box<Type>),
    RecursiveType(Loc),
}

#[derive(Clone, Copy, Debug)]
enum TypingCase {
    Join,
    Subtype,
}

pub fn subtype(subst: Subst, lhs: &Type, rhs: &Type) -> Result<(Subst, Type), TypingError> {
    join_impl(subst, TypingCase::Subtype, lhs, rhs)
}

pub fn join(subst: Subst, lhs: &Type, rhs: &Type) -> Result<(Subst, Type), TypingError> {
    join_impl(subst, TypingCase::Join, lhs, rhs)
}

fn join_impl(
    mut subst: Subst,
    case: TypingCase,
    lhs: &Type,
    rhs: &Type,
) -> Result<(Subst, Type), TypingError> {
    use TypeName_::*;
    use Type_::*;
    use TypingCase::*;
    match (lhs, rhs) {
        (sp!(_, Anything), other) | (other, sp!(_, Anything)) => Ok((subst, other.clone())),

        (sp!(_, Unit), sp!(loc, Unit)) => Ok((subst, sp(*loc, Unit))),

        (sp!(loc1, Ref(mut1, t1)), sp!(loc2, Ref(mut2, t2))) => {
            let (loc, mut_) = match (case, mut1, mut2) {
                (Join, _, _) => {
                    // if 1 is imm and 2 is mut, use loc1. Else, loc2
                    let loc = if !*mut1 && *mut2 { *loc1 } else { *loc2 };
                    (loc, *mut1 && *mut2)
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
                    ))
                }
            };
            let (subst, t) = join_impl(subst, case, t1, t2)?;
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
            let (subst, tys) = join_impl_types(subst, case, tys1, tys2)?;
            Ok((subst, sp(*loc, Apply(k2.clone(), n2.clone(), tys))))
        }
        (sp!(loc1, Var(id1)), sp!(loc2, Var(id2))) => {
            if *id1 == *id2 {
                Ok((subst, sp(*loc2, Var(*id2))))
            } else {
                join_tvar(subst, case, *loc1, *id1, *loc2, *id2)
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
            let new_tvar = TVar::next();
            subst.insert(new_tvar, other.clone());
            join_tvar(subst, case, *loc, *id, other.loc, new_tvar)
        }
        (other, sp!(loc, Var(id))) => {
            let new_tvar = TVar::next();
            subst.insert(new_tvar, other.clone());
            join_tvar(subst, case, other.loc, new_tvar, *loc, *id)
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
    mut subst: Subst,
    case: TypingCase,
    tys1: &[Type],
    tys2: &[Type],
) -> Result<(Subst, Vec<Type>), TypingError> {
    // if tys1.len() != tys2.len(), we will get an error when instantiating the type elsewhere
    // as all types are instantiated as a sanity check
    let mut tys = vec![];
    for (ty1, ty2) in tys1.iter().zip(tys2) {
        let (nsubst, t) = join_impl(subst, case, ty1, ty2)?;
        subst = nsubst;
        tys.push(t)
    }
    Ok((subst, tys))
}

fn join_tvar(
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

    let new_tvar = TVar::next();
    let num_loc_1 = subst.num_vars.get(&last_id1);
    let num_loc_2 = subst.num_vars.get(&last_id2);
    match (num_loc_1, num_loc_2) {
        (_, Some(nloc)) | (Some(nloc), _) => {
            let nloc = *nloc;
            subst.set_num_var(new_tvar, nloc);
        }
        _ => (),
    }
    subst.insert(last_id1, sp(loc1, Var(new_tvar)));
    subst.insert(last_id2, sp(loc2, Var(new_tvar)));

    let (mut subst, new_ty) = join_impl(subst, case, &ty1, &ty2)?;
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

    fn used_tvars(used: &mut BTreeMap<TVar, Loc>, sp!(loc, t_): &Type) {
        use Type_ as T;
        match t_ {
            T::Var(v) => {
                used.insert(*v, *loc);
            }
            T::Ref(_, inner) => used_tvars(used, inner),
            T::Apply(_, _, inners) => inners
                .iter()
                .rev()
                .for_each(|inner| used_tvars(used, inner)),
            T::Unit | T::Param(_) | T::Anything | T::UnresolvedError => (),
        }
    }

    // check not necessary for soundness but improves error message structure
    if !check_num_tvar(subst, loc, tvar, &ty) {
        return Ok(false);
    }

    let used = &mut BTreeMap::new();
    used_tvars(used, &ty);
    if let Some(_rec_loc) = used.get(&tvar) {
        return Err(TypingError::RecursiveType(loc));
    }

    match &ty.value {
        Type_::Anything => (),
        _ => subst.insert(tvar, ty),
    }
    Ok(true)
}

fn check_num_tvar(subst: &Subst, _loc: Loc, tvar: TVar, ty: &Type) -> bool {
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
                None => subst.is_num_var(last_tvar),
                Some(t) => check_num_tvar_(subst, t),
            }
        }
        _ => false,
    }
}
