// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    diagnostics::codes::*,
    editions::FeatureGate,
    expansion::{
        ast::{self as E, AbilitySet, ModuleIdent, Visibility},
        translate::is_valid_struct_constant_or_schema_name as is_constant_name,
    },
    naming::{
        ast::{self as N, BlockLabel, TParamID},
        fake_natives,
    },
    parser::ast::{self as P, ConstantName, DatatypeName, Field, FunctionName, VariantName},
    shared::{program_info::NamingProgramInfo, unique_map::UniqueMap, *},
    FullyCompiledProgram,
};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::collections::{BTreeMap, BTreeSet};

//**************************************************************************************************
// Context
//**************************************************************************************************

#[derive(Debug, Clone)]
enum ResolvedType {
    ModuleType(Box<ResolvedModuleType>),
    TParam(Loc, N::TParam),
    BuiltinType(N::BuiltinTypeName_),
    Unbound,
}

impl ResolvedType {
    fn is_struct(&self) -> bool {
        match self {
            ResolvedType::ModuleType(rt) => match rt.module_type {
                ModuleType::Struct(..) => true,
                ModuleType::Enum(..) => false,
            },
            _ => false,
        }
    }

    #[allow(dead_code)]
    fn is_enum(&self) -> bool {
        match self {
            ResolvedType::ModuleType(rt) => match rt.module_type {
                ModuleType::Struct(..) => false,
                ModuleType::Enum(..) => true,
            },
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedModuleType {
    // original names/locs are provided to preserve loc information if needed
    original_loc: Loc,
    original_type_name: Name,
    module_type: ModuleType,
}

#[derive(Debug, Clone)]
enum ModuleType {
    Struct(Box<StructType>),
    Enum(Box<EnumType>),
}

impl ModuleType {
    fn decl_loc(&self) -> Loc {
        match self {
            ModuleType::Struct(stype) => stype.decl_loc,
            ModuleType::Enum(etype) => etype.decl_loc,
        }
    }

    fn with_original_mident(self, mident: ModuleIdent) -> ModuleType {
        match self {
            ModuleType::Struct(stype) => {
                let st = StructType {
                    original_mident: mident,
                    ..*stype
                };
                ModuleType::Struct(Box::new(st))
            }
            ModuleType::Enum(etype) => {
                let et = EnumType {
                    original_mident: mident,
                    ..*etype
                };
                ModuleType::Enum(Box::new(et))
            }
        }
    }
}

#[derive(Debug, Clone)]
struct StructType {
    original_mident: ModuleIdent,
    decl_loc: Loc,
    arity: usize,
    is_positional: bool,
}

#[derive(Debug, Clone)]
struct EnumType {
    original_mident: ModuleIdent,
    decl_loc: Loc,
    arity: usize,
    variants: UniqueMap<VariantName, VariantConstructor>,
}

#[derive(Debug, Clone)]
struct VariantConstructor {
    original_variant_name: Name,
    decl_loc: Loc,
    is_positional: bool,
    is_empty: bool,
}

enum ResolvedFunction {
    Builtin(N::BuiltinFunction),
    Module(Box<ResolvedModuleFunction>),
    Unbound,
}

struct ResolvedModuleFunction {
    // original names/locs are provided to preserve loc information if needed
    module: ModuleIdent,
    function: FunctionName,
    ty_args: Option<Vec<N::Type>>,
}

#[derive(PartialEq)]
enum NominalBlockType {
    Loop,
    Block,
}

struct Context<'env> {
    env: &'env mut CompilationEnv,
    current_module: Option<ModuleIdent>,
    scoped_types: BTreeMap<ModuleIdent, BTreeMap<Symbol, ModuleType>>,
    unscoped_types: BTreeMap<Symbol, ResolvedType>,
    scoped_functions: BTreeMap<ModuleIdent, BTreeMap<Symbol, Loc>>,
    scoped_constants: BTreeMap<ModuleIdent, BTreeMap<Symbol, Loc>>,
    local_scopes: Vec<BTreeMap<Symbol, u16>>,
    local_count: BTreeMap<Symbol, u16>,
    used_locals: BTreeSet<N::Var_>,
    nominal_blocks: Vec<(Symbol, u16, NominalBlockType)>,
    nominal_block_id: u16,
    /// Type parameters used in a function (they have to be cleared after processing each function).
    used_fun_tparams: BTreeSet<TParamID>,
    /// Indicates if the compiler is currently translating a function (set to true before starting
    /// to translate a function and to false after translation is over).
    translating_fun: bool,
    current_package: Option<Symbol>,
}

impl<'env> Context<'env> {
    fn new(
        compilation_env: &'env mut CompilationEnv,
        pre_compiled_lib: Option<&FullyCompiledProgram>,
        prog: &E::Program,
    ) -> Self {
        use ResolvedType as RT;
        let all_modules = || {
            prog.modules
                .key_cloned_iter()
                .chain(pre_compiled_lib.iter().flat_map(|pre_compiled| {
                    pre_compiled
                        .expansion
                        .modules
                        .key_cloned_iter()
                        .filter(|(mident, _m)| !prog.modules.contains_key(mident))
                }))
        };
        let scoped_types = all_modules()
            .map(|(mident, mdef)| {
                let mems = {
                    let mut smems = mdef
                        .structs
                        .key_cloned_iter()
                        .map(|(s, sdef)| {
                            let arity = sdef.type_parameters.len();
                            let sname = s.value();
                            let is_positional =
                                matches!(sdef.fields, E::StructFields::Positional(_));
                            let st = StructType {
                                original_mident: mident,
                                decl_loc: s.loc(),
                                arity,
                                is_positional,
                            };
                            let type_info = ModuleType::Struct(Box::new(st));
                            (sname, type_info)
                        })
                        .collect::<BTreeMap<_, _>>();
                    let mut emems = mdef
                        .enums
                        .key_cloned_iter()
                        .map(|(e, edef)| {
                            let arity = edef.type_parameters.len();
                            let ename = e.value();
                            let variants =
                                edef.variants.clone().map(|name, v| VariantConstructor {
                                    original_variant_name: name.0,
                                    decl_loc: v.loc,
                                    is_positional: matches!(
                                        v.fields,
                                        E::VariantFields::Positional(_)
                                    ),
                                    is_empty: matches!(v.fields, E::VariantFields::Empty),
                                });
                            let et = EnumType {
                                original_mident: mident,
                                arity,
                                decl_loc: e.loc(),
                                variants,
                            };
                            let type_info = ModuleType::Enum(Box::new(et));
                            (ename, type_info)
                        })
                        .collect::<BTreeMap<_, _>>();
                    /* duplicates were already reported by expasion */
                    smems.append(&mut emems);
                    smems
                };
                (mident, mems)
            })
            .collect();
        let scoped_functions = all_modules()
            .map(|(mident, mdef)| {
                let mems = mdef
                    .functions
                    .iter()
                    .map(|(nloc, n, _)| (*n, nloc))
                    .collect();
                (mident, mems)
            })
            .collect();
        let scoped_constants = all_modules()
            .map(|(mident, mdef)| {
                let mems = mdef
                    .constants
                    .iter()
                    .map(|(nloc, n, _)| (*n, nloc))
                    .collect();
                (mident, mems)
            })
            .collect();
        let unscoped_types = N::BuiltinTypeName_::all_names()
            .iter()
            .map(|s| {
                let b_ = RT::BuiltinType(N::BuiltinTypeName_::resolve(s.as_str()).unwrap());
                (*s, b_)
            })
            .collect();
        Self {
            env: compilation_env,
            current_module: None,
            scoped_types,
            scoped_functions,
            scoped_constants,
            unscoped_types,
            local_scopes: vec![],
            local_count: BTreeMap::new(),
            nominal_blocks: vec![],
            nominal_block_id: 0,
            used_locals: BTreeSet::new(),
            used_fun_tparams: BTreeSet::new(),
            translating_fun: false,
            current_package: None,
        }
    }

    fn resolve_module(&mut self, m: &ModuleIdent) -> bool {
        // NOTE: piggybacking on `scoped_functions` to provide a set of modules in the context。
        // TODO: a better solution would be to have a single `BTreeMap<ModuleIdent, ModuleInfo>`
        // in the context that can be used to resolve modules, types, and functions.
        let resolved = self.scoped_functions.contains_key(m);
        if !resolved {
            self.env.add_diag(diag!(
                NameResolution::UnboundModule,
                (m.loc, format!("Unbound module '{}'", m))
            ))
        }
        resolved
    }

    fn resolve_module_type(&mut self, loc: Loc, m: &ModuleIdent, n: &Name) -> Option<ModuleType> {
        let types = match self.scoped_types.get(m) {
            None => {
                self.env.add_diag(diag!(
                    NameResolution::UnboundModule,
                    (m.loc, format!("Unbound module '{}'", m)),
                ));
                return None;
            }
            Some(members) => members,
        };
        match types.get(&n.value) {
            None => {
                let msg = format!(
                    "Invalid module access. Unbound struct '{}' in module '{}'",
                    n, m
                );
                self.env
                    .add_diag(diag!(NameResolution::UnboundModuleMember, (loc, msg)));
                None
            }
            Some(module_type) => Some(module_type.clone()),
        }
    }

    fn resolve_module_function(
        &mut self,
        loc: Loc,
        m: &ModuleIdent,
        n: &Name,
    ) -> Option<FunctionName> {
        let functions = match self.scoped_functions.get(m) {
            None => {
                self.env.add_diag(diag!(
                    NameResolution::UnboundModule,
                    (m.loc, format!("Unbound module '{}'", m)),
                ));
                return None;
            }
            Some(members) => members,
        };
        match functions.get(&n.value).cloned() {
            None => {
                let msg = format!(
                    "Invalid module access. Unbound function '{}' in module '{}'",
                    n, m
                );
                self.env
                    .add_diag(diag!(NameResolution::UnboundModuleMember, (loc, msg)));
                None
            }
            Some(_) => Some(FunctionName(*n)),
        }
    }

    fn resolve_module_constant(
        &mut self,
        loc: Loc,
        m: &ModuleIdent,
        n: Name,
    ) -> Option<ConstantName> {
        let constants = match self.scoped_constants.get(m) {
            None => {
                self.env.add_diag(diag!(
                    NameResolution::UnboundModule,
                    (m.loc, format!("Unbound module '{}'", m)),
                ));
                return None;
            }
            Some(members) => members,
        };
        match constants.get(&n.value).cloned() {
            None => {
                let msg = format!(
                    "Invalid module access. Unbound constant '{}' in module '{}'",
                    n, m
                );
                self.env
                    .add_diag(diag!(NameResolution::UnboundModuleMember, (loc, msg)));
                None
            }
            Some(_) => Some(ConstantName(n)),
        }
    }

    fn resolve_type(&mut self, sp!(nloc, ma_): E::ModuleAccess) -> ResolvedType {
        use E::ModuleAccess_ as EN;
        match ma_ {
            EN::Name(n) => self.resolve_unscoped_type(nloc, n),
            EN::ModuleAccess(m, n) | EN::Variant(sp!(_, (m, n)), _) => {
                let Some(module_type) = self.resolve_module_type(nloc, &m, &n) else {
                    assert!(self.env.has_errors());
                    return ResolvedType::Unbound;
                };
                let mt = ResolvedModuleType {
                    original_loc: nloc,
                    original_type_name: n,
                    module_type: module_type.with_original_mident(m),
                };
                ResolvedType::ModuleType(Box::new(mt))
            }
        }
    }

    fn resolve_unscoped_type(&mut self, loc: Loc, n: Name) -> ResolvedType {
        match self.unscoped_types.get(&n.value) {
            None => {
                let msg = format!("Unbound type '{}' in current scope", n);
                self.env
                    .add_diag(diag!(NameResolution::UnboundType, (loc, msg)));
                ResolvedType::Unbound
            }
            Some(rn) => rn.clone(),
        }
    }

    fn resolves_to_struct(&self, sp!(_, ma_): &E::ModuleAccess) -> bool {
        use E::ModuleAccess_ as EA;
        match ma_ {
            EA::Name(n) => self
                .unscoped_types
                .get(&n.value)
                .is_some_and(|rt| rt.is_struct() || matches!(rt, ResolvedType::BuiltinType(_))),
            EA::ModuleAccess(m, n) => self
                .scoped_types
                .get(m)
                .and_then(|types| types.get(&n.value))
                .is_some(),
            EA::Variant(_, _) => false,
        }
    }

    fn resolve_struct_name(
        &mut self,
        loc: Loc,
        verb: &str,
        ma: E::ModuleAccess,
        etys_opt: Option<Vec<E::Type>>,
    ) -> Option<(ModuleIdent, DatatypeName, Option<Vec<N::Type>>, bool)> {
        match self.resolve_type(ma) {
            ResolvedType::Unbound => {
                assert!(self.env.has_errors());
                None
            }
            rt @ (ResolvedType::BuiltinType(_) | ResolvedType::TParam(_, _)) => {
                let (rtloc, msg) = match rt {
                    ResolvedType::TParam(loc, tp) => (
                        loc,
                        format!(
                            "But '{}' was declared as a type parameter here",
                            tp.user_specified_name
                        ),
                    ),
                    ResolvedType::BuiltinType(n) => {
                        (ma.loc, format!("But '{n}' is a builtin type"))
                    }
                    _ => unreachable!(),
                };
                self.env.add_diag(diag!(
                    NameResolution::NamePositionMismatch,
                    (ma.loc, format!("Invalid {}. Expected a struct name", verb)),
                    (rtloc, msg)
                ));
                None
            }
            ResolvedType::ModuleType(mt) => {
                let ResolvedModuleType {
                    module_type,
                    original_type_name: n,
                    ..
                } = *mt;
                match module_type {
                    ModuleType::Struct(struct_type) => {
                        let m = struct_type.original_mident;
                        let tys_opt = etys_opt.map(|etys| {
                            let tys = types(self, etys);
                            let name_f = || format!("{}::{}", &m, &n);
                            check_type_argument_arity(self, loc, name_f, tys, struct_type.arity)
                        });
                        Some((m, DatatypeName(n), tys_opt, struct_type.is_positional))
                    }
                    ModuleType::Enum(..) => {
                        self.env.add_diag(diag!(
                            NameResolution::NamePositionMismatch,
                            (ma.loc, format!("Invalid {}. Expected a struct", verb)),
                            (n.loc, format!("But '{}' is an enum", n))
                        ));
                        None
                    }
                }
            }
        }
    }

    #[allow(dead_code)]
    fn resolve_enum_name(
        &mut self,
        loc: Loc,
        verb: &str,
        ma: E::ModuleAccess,
        etys_opt: Option<Vec<E::Type>>,
    ) -> Option<(ModuleIdent, DatatypeName, Option<Vec<N::Type>>)> {
        match self.resolve_type(ma) {
            ResolvedType::Unbound => {
                assert!(self.env.has_errors());
                None
            }
            rt @ (ResolvedType::BuiltinType(_) | ResolvedType::TParam(_, _)) => {
                let (rtloc, msg) = match rt {
                    ResolvedType::TParam(loc, tp) => (
                        loc,
                        format!(
                            "But '{}' was declared as a type parameter here",
                            tp.user_specified_name
                        ),
                    ),
                    ResolvedType::BuiltinType(n) => {
                        (ma.loc, format!("But '{n}' is a builtin type"))
                    }
                    _ => unreachable!(),
                };
                self.env.add_diag(diag!(
                    NameResolution::NamePositionMismatch,
                    (ma.loc, format!("Invalid {}. Expected a struct name", verb)),
                    (rtloc, msg)
                ));
                None
            }
            ResolvedType::ModuleType(mt) => {
                let ResolvedModuleType {
                    module_type,
                    original_type_name: n,
                    ..
                } = *mt;
                match module_type {
                    ModuleType::Enum(enum_type) => {
                        let m = enum_type.original_mident;
                        let tys_opt = etys_opt.map(|etys| {
                            let tys = types(self, etys);
                            let name_f = || format!("{}::{}", &m, &n);
                            check_type_argument_arity(self, loc, name_f, tys, enum_type.arity)
                        });
                        Some((m, DatatypeName(n), tys_opt))
                    }
                    ModuleType::Struct(..) => {
                        self.env.add_diag(diag!(
                            NameResolution::NamePositionMismatch,
                            (ma.loc, format!("Invalid {}. Expected an enum", verb)),
                            (n.loc, format!("But '{}' is an struct", n))
                        ));
                        None
                    }
                }
            }
        }
    }

    fn resolve_enum_name_with_variants(
        &mut self,
        loc: Loc,
        verb: &str,
        ma: E::ModuleAccess,
        etys_opt: Option<Vec<E::Type>>,
    ) -> Option<(
        ModuleIdent,
        DatatypeName,
        Option<Vec<N::Type>>,
        UniqueMap<VariantName, VariantConstructor>,
    )> {
        match self.resolve_type(ma) {
            ResolvedType::Unbound => {
                assert!(self.env.has_errors());
                None
            }
            rt @ (ResolvedType::BuiltinType(_) | ResolvedType::TParam(_, _)) => {
                let (rtloc, msg) = match rt {
                    ResolvedType::TParam(loc, tp) => (
                        loc,
                        format!(
                            "But '{}' was declared as a type parameter here",
                            tp.user_specified_name
                        ),
                    ),
                    ResolvedType::BuiltinType(n) => {
                        (ma.loc, format!("But '{n}' is a builtin type"))
                    }
                    _ => unreachable!(),
                };
                self.env.add_diag(diag!(
                    NameResolution::NamePositionMismatch,
                    (ma.loc, format!("Invalid {}. Expected a struct name", verb)),
                    (rtloc, msg)
                ));
                None
            }
            ResolvedType::ModuleType(mt) => {
                let ResolvedModuleType {
                    module_type,
                    original_type_name: n,
                    ..
                } = *mt;
                match module_type {
                    ModuleType::Enum(enum_type) => {
                        let m = enum_type.original_mident;
                        let tys_opt = etys_opt.map(|etys| {
                            let tys = types(self, etys);
                            let name_f = || format!("{}::{}", &m, &n);
                            check_type_argument_arity(self, loc, name_f, tys, enum_type.arity)
                        });
                        Some((m, DatatypeName(n), tys_opt, enum_type.variants))
                    }
                    ModuleType::Struct(..) => {
                        self.env.add_diag(diag!(
                            NameResolution::NamePositionMismatch,
                            (ma.loc, format!("Invalid {}. Expected an enum", verb)),
                            (n.loc, format!("But '{}' is an struct", n))
                        ));
                        None
                    }
                }
            }
        }
    }

    fn resolve_variant_name(
        &mut self,
        loc: Loc,
        verb: &str,
        ma: E::ModuleAccess,
        etys_opt: Option<Vec<E::Type>>,
    ) -> Option<(
        ModuleIdent,
        DatatypeName,
        VariantName,
        Option<Vec<N::Type>>,
        Loc,
        /* is_positional */ bool,
        /* is_empty */ bool,
    )> {
        match &ma {
            sp!(_, E::ModuleAccess_::Variant(sp!(_, _), variant_name)) => {
                if let Some((mident, enum_name, ty_opts, variants)) =
                    self.resolve_enum_name_with_variants(loc, verb, ma, etys_opt)
                {
                    if let Some(vdef) = variants.get(&VariantName(*variant_name)) {
                        Some((
                            mident,
                            enum_name,
                            VariantName(vdef.original_variant_name),
                            ty_opts,
                            vdef.decl_loc,
                            vdef.is_positional,
                            vdef.is_empty,
                        ))
                    } else {
                        let primary_msg = format!(
                            "Invalid {}. Variant '{}' is not part of this enum",
                            verb, variant_name
                        );
                        let decl_msg = format!("Enum '{}' is defined here", enum_name);
                        self.env.add_diag(diag!(
                            NameResolution::UnboundVariant,
                            (loc, primary_msg),
                            (enum_name.loc(), decl_msg),
                        ));
                        None
                    }
                } else {
                    assert!(self.env.has_errors());
                    None
                }
            }
            _ => {
                self.env.add_diag(diag!(
                    NameResolution::NamePositionMismatch,
                    (loc, format!("Invalid {}. Expected a variant name", verb)),
                ));
                None
            }
        }
    }

    fn resolve_constant(
        &mut self,
        sp!(loc, ma_): E::ModuleAccess,
    ) -> Option<(ModuleIdent, ConstantName)> {
        use E::ModuleAccess_ as EA;
        match ma_ {
            EA::Name(n) => {
                self.env.add_diag(diag!(
                    NameResolution::UnboundUnscopedName,
                    (loc, format!("Unbound constant '{}'", n)),
                ));
                None
            }
            EA::ModuleAccess(m, n) => match self.resolve_module_constant(loc, &m, n) {
                None => {
                    assert!(self.env.has_errors());
                    None
                }
                Some(cname) => Some((m, cname)),
            },
            EA::Variant(_, _) => {
                self.env.add_diag(diag!(
                    NameResolution::NamePositionMismatch,
                    (loc, "Invalid variant. Expected a constant name".to_string()),
                ));
                None
            }
        }
    }

    fn bind_type(&mut self, s: Symbol, rt: ResolvedType) {
        self.unscoped_types.insert(s, rt);
    }

    fn save_unscoped(&self) -> BTreeMap<Symbol, ResolvedType> {
        self.unscoped_types.clone()
    }

    fn restore_unscoped(&mut self, types: BTreeMap<Symbol, ResolvedType>) {
        self.unscoped_types = types;
    }

    fn new_local_scope(&mut self) {
        let cur = self.local_scopes.last().unwrap().clone();
        self.local_scopes.push(cur)
    }

    fn close_local_scope(&mut self) {
        self.local_scopes.pop();
    }

    fn declare_local(&mut self, is_parameter: bool, sp!(vloc, name): Name) -> N::Var {
        let default = if is_parameter { 0 } else { 1 };
        let id = *self
            .local_count
            .entry(name)
            .and_modify(|c| *c += 1)
            .or_insert(default);
        self.local_scopes.last_mut().unwrap().insert(name, id);
        // all locals start at color zero
        // they will be incremented when substituted for macros
        let nvar_ = N::Var_ { name, id, color: 0 };
        sp(vloc, nvar_)
    }

    fn resolve_local<S: ToString>(
        &mut self,
        loc: Loc,
        variable_msg: impl FnOnce(Symbol) -> S,
        sp!(vloc, name): Name,
    ) -> Option<N::Var> {
        let id_opt = self.local_scopes.last().unwrap().get(&name).copied();
        match id_opt {
            None => {
                let msg = variable_msg(name);
                self.env
                    .add_diag(diag!(NameResolution::UnboundVariable, (loc, msg)));
                None
            }
            Some(id) => {
                // all locals start at color zero
                // they will be incremented when substituted for macros
                let nvar_ = N::Var_ { name, id, color: 0 };
                self.used_locals.insert(nvar_);
                Some(sp(vloc, nvar_))
            }
        }
    }

    /// Similar to resolve_local, but does not mark usage.
    fn as_var(&mut self, loc: Loc, verb: &str, sp!(vloc, name): Name) -> Option<N::Var> {
        let id_opt = self.local_scopes.last().unwrap().get(&name).copied();
        match id_opt {
            None => {
                let msg = format!("Invalid {}. Unbound variable '{}'", verb, name);
                self.env
                    .add_diag(diag!(NameResolution::UnboundVariable, (loc, msg)));
                None
            }
            Some(id) => {
                // all locals start at color zero
                // they will be incremented when substituted for macros
                let nvar_ = N::Var_ { name, id, color: 0 };
                Some(sp(vloc, nvar_))
            }
        }
    }

    fn enter_nominal_block(&mut self, name: Option<P::BlockLabel>, name_type: NominalBlockType) {
        debug_assert!(
            self.nominal_blocks.len() < 100,
            "Nominal block list exceeded 100."
        );
        let sym = if let Some(name) = name {
            name.value()
        } else {
            // all named blocks have names, so a non-named block must be a loop
            N::Exp_::LOOP_NAME_SYMBOL
        };
        let id = self.nominal_block_id;
        self.nominal_block_id += 1;
        self.nominal_blocks.push((sym, id, name_type));
    }

    fn current_loop(&self, loc: Loc) -> Option<BlockLabel> {
        self.nominal_blocks
            .iter()
            .rev()
            .find(|(_, _, name_type)| *name_type == NominalBlockType::Loop)
            .map(|(name, id, _)| {
                BlockLabel(sp(
                    loc,
                    N::Var_ {
                        name: *name,
                        id: *id,
                        color: 0,
                    },
                ))
            })
    }

    fn resolve_nominal_label(
        &mut self,
        verb: &str,
        expected_block_type: NominalBlockType,
        label: P::BlockLabel,
    ) -> Option<BlockLabel> {
        let loc = label.loc();
        let name = label.value();
        let id_opt = self
            .nominal_blocks
            .iter()
            .rev()
            .find(|(block_name, _, _)| name == *block_name)
            .map(|(_, id, block_type)| (id, block_type));
        if let Some((id, block_type)) = id_opt {
            if *block_type == expected_block_type {
                let nvar_ = N::Var_ {
                    name,
                    id: *id,
                    color: 0,
                };
                Some(BlockLabel(sp(loc, nvar_)))
            } else {
                let msg = format!(
                    "Invalid usage of '{}' with a {} block label",
                    verb, block_type
                );
                let mut diag = diag!(NameResolution::InvalidLabel, (loc, msg));
                match expected_block_type {
                    NominalBlockType::Loop => {
                        diag.add_note("Loop labels may only be used with 'break' and 'continue', not 'return'");
                    }
                    NominalBlockType::Block => {
                        diag.add_note("Named block labels may only be used with 'return', not 'break' or 'continue'.");
                    }
                }
                self.env.add_diag(diag);
                None
            }
        } else {
            let msg = format!("Invalid {}. Unbound label '{}", verb, name);
            self.env
                .add_diag(diag!(NameResolution::UnboundVariable, (loc, msg)));
            None
        }
    }

    fn exit_nominal_block(&mut self, loc: Loc) -> BlockLabel {
        let (name, id, _) = self.nominal_blocks.pop().unwrap();
        let nvar_ = N::Var_ { name, id, color: 0 };
        BlockLabel(sp(loc, nvar_))
    }
}

impl std::fmt::Display for NominalBlockType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                NominalBlockType::Loop => "loop",
                NominalBlockType::Block => "named",
            }
        )
    }
}

//**************************************************************************************************
// Entry
//**************************************************************************************************

pub fn program(
    compilation_env: &mut CompilationEnv,
    pre_compiled_lib: Option<&FullyCompiledProgram>,
    prog: E::Program,
) -> N::Program {
    let mut context = Context::new(compilation_env, pre_compiled_lib, &prog);
    let E::Program { modules: emodules } = prog;
    let modules = modules(&mut context, emodules);
    let mut inner = N::Program_ { modules };
    let mut info = NamingProgramInfo::new(pre_compiled_lib, &inner);
    super::resolve_use_funs::program(compilation_env, &mut info, &mut inner);
    N::Program { info, inner }
}

fn modules(
    context: &mut Context,
    modules: UniqueMap<ModuleIdent, E::ModuleDefinition>,
) -> UniqueMap<ModuleIdent, N::ModuleDefinition> {
    modules.map(|ident, mdef| module(context, ident, mdef))
}

fn module(
    context: &mut Context,
    ident: ModuleIdent,
    mdef: E::ModuleDefinition,
) -> N::ModuleDefinition {
    context.current_module = Some(ident);
    let E::ModuleDefinition {
        loc,
        warning_filter,
        package_name,
        attributes,
        is_source_module,
        use_funs: euse_funs,
        friends: efriends,
        structs: estructs,
        enums: eenums,
        functions: efunctions,
        constants: econstants,
    } = mdef;
    context.current_package = package_name;
    context.env.add_warning_filter_scope(warning_filter.clone());
    let unscoped = context.save_unscoped();
    let use_funs = use_funs(context, euse_funs);
    let friends = efriends.filter_map(|mident, f| friend(context, mident, f));
    let struct_names = estructs
        .key_cloned_iter()
        .map(|(k, _)| k)
        .collect::<BTreeSet<_>>();
    let enum_names = eenums
        .key_cloned_iter()
        .map(|(k, _)| k)
        .collect::<BTreeSet<_>>();
    let enum_struct_intersection = enum_names
        .intersection(&struct_names)
        .collect::<BTreeSet<_>>();
    let structs = estructs.map(|name, s| {
        context.restore_unscoped(unscoped.clone());
        struct_def(context, name, s)
    });
    // simply for compilation to continue in the presence of errors, we remove the duplicates
    let enums = eenums.filter_map(|name, e| {
        context.restore_unscoped(unscoped.clone());
        if enum_struct_intersection.contains(&name) {
            None
        } else {
            Some(enum_def(context, name, e))
        }
    });
    let functions = efunctions.map(|name, f| {
        context.restore_unscoped(unscoped.clone());
        function(context, ident, name, f)
    });
    let constants = econstants.map(|name, c| {
        context.restore_unscoped(unscoped.clone());
        constant(context, name, c)
    });
    context.restore_unscoped(unscoped);
    context.env.pop_warning_filter_scope();
    context.current_package = None;
    N::ModuleDefinition {
        loc,
        warning_filter,
        package_name,
        attributes,
        is_source_module,
        use_funs,
        friends,
        structs,
        enums,
        constants,
        functions,
    }
}

//**************************************************************************************************
// Use Funs
//**************************************************************************************************

fn use_funs(context: &mut Context, eufs: E::UseFuns) -> N::UseFuns {
    let E::UseFuns {
        explicit: eexplicit,
        implicit: eimplicit,
    } = eufs;
    let mut resolved = N::ResolvedUseFuns::new();
    let resolved_vec: Vec<_> = eexplicit
        .into_iter()
        .flat_map(|e| explicit_use_fun(context, e))
        .collect();
    for (tn, method, nuf) in resolved_vec {
        let methods = resolved.entry(tn.clone()).or_default();
        let nuf_loc = nuf.loc;
        if let Err((_, prev)) = methods.add(method, nuf) {
            let msg = format!("Duplicate 'use fun' for '{}.{}'", tn, method);
            context.env.add_diag(diag!(
                Declarations::DuplicateItem,
                (nuf_loc, msg),
                (prev, "Previously declared here"),
            ))
        }
    }
    N::UseFuns {
        resolved,
        implicit_candidates: eimplicit,
    }
}

fn explicit_use_fun(
    context: &mut Context,
    e: E::ExplicitUseFun,
) -> Option<(N::TypeName, Name, N::UseFun)> {
    let E::ExplicitUseFun {
        loc,
        attributes,
        is_public,
        function,
        ty,
        method,
    } = e;
    let m_f_opt = match resolve_function(context, loc, function, None) {
        ResolvedFunction::Module(mf) => {
            let ResolvedModuleFunction {
                module,
                function,
                ty_args,
            } = *mf;
            assert!(ty_args.is_none());
            Some((module, function))
        }
        ResolvedFunction::Builtin(_) => {
            let msg = "Invalid 'use fun'. Cannot use a builtin function as a method";
            context
                .env
                .add_diag(diag!(Declarations::InvalidUseFun, (loc, msg)));
            None
        }
        ResolvedFunction::Unbound => {
            assert!(context.env.has_errors());
            None
        }
    };
    let tn_opt = match context.resolve_type(ty) {
        ResolvedType::Unbound => {
            assert!(context.env.has_errors());
            None
        }
        ResolvedType::TParam(tloc, tp) => {
            let tmsg = format!(
                "But '{}' was declared as a type parameter here",
                tp.user_specified_name
            );
            context.env.add_diag(diag!(
                Declarations::InvalidUseFun,
                (
                    loc,
                    "Invalid 'use fun'. Cannot associate a method a type parameter"
                ),
                (tloc, tmsg)
            ));
            None
        }
        ResolvedType::BuiltinType(bt_) => Some(N::TypeName_::Builtin(sp(ty.loc, bt_))),
        ResolvedType::ModuleType(mt) => match mt.module_type {
            ModuleType::Struct(stype) => Some(N::TypeName_::ModuleType(
                stype.original_mident,
                DatatypeName(mt.original_type_name),
            )),
            ModuleType::Enum(etype) => Some(N::TypeName_::ModuleType(
                etype.original_mident,
                DatatypeName(mt.original_type_name),
            )),
        },
    };
    let tn_ = tn_opt?;
    let tn = sp(ty.loc, tn_);
    if let Some(pub_loc) = is_public {
        let current_module = context.current_module;
        if let Err(def_loc_opt) = use_fun_module_defines(context, current_module, &tn) {
            let msg = "Invalid 'use fun'. Cannot publicly associate a function with a \
                type defined in another module";
            let pub_msg = format!(
                "Declared '{}' here. Consider removing to make a local 'use fun' instead",
                Visibility::PUBLIC
            );
            let mut diag = diag!(Declarations::InvalidUseFun, (loc, msg), (pub_loc, pub_msg));
            if let Some(def_loc) = def_loc_opt {
                diag.add_secondary_label((def_loc, "Type defined in another module here"));
            }
            context.env.add_diag(diag);
            return None;
        }
    }
    let target_function = m_f_opt?;
    let use_fun = N::UseFun {
        loc,
        attributes,
        is_public,
        target_function,
        kind: N::UseFunKind::Explicit,
        used: is_public.is_some(), // suppress unused warning for public use funs
    };
    Some((tn, method, use_fun))
}

fn use_fun_module_defines(
    context: &mut Context,
    specified: Option<ModuleIdent>,
    tn: &N::TypeName,
) -> Result<(), Option<Loc>> {
    match &tn.value {
        N::TypeName_::Builtin(sp!(_, b_)) => {
            let definer_opt = context.env.primitive_definer(*b_);
            match (definer_opt, &specified) {
                (None, _) => Err(None),
                (Some(d), None) => Err(Some(d.loc)),
                (Some(d), Some(s)) => {
                    if d == s {
                        Ok(())
                    } else {
                        Err(Some(d.loc))
                    }
                }
            }
        }
        N::TypeName_::ModuleType(m, n) => {
            if specified.as_ref().is_some_and(|n| n == m) {
                Ok(())
            } else {
                let mod_type = context
                    .scoped_types
                    .get(m)
                    .unwrap()
                    .get(&n.value())
                    .unwrap();
                Err(Some(mod_type.decl_loc()))
            }
        }
        N::TypeName_::Multiple(_) => panic!("ICE tuple should not be reachable from use fun"),
    }
}

//**************************************************************************************************
// Friends
//**************************************************************************************************

fn friend(context: &mut Context, mident: ModuleIdent, friend: E::Friend) -> Option<E::Friend> {
    let current_mident = context.current_module.as_ref().unwrap();
    if mident.value.address != current_mident.value.address {
        // NOTE: in alignment with the bytecode verifier, this constraint is a policy decision
        // rather than a technical requirement. The compiler, VM, and bytecode verifier DO NOT
        // rely on the assumption that friend modules must reside within the same account address.
        let msg = "Cannot declare modules out of the current address as a friend";
        context.env.add_diag(diag!(
            Declarations::InvalidFriendDeclaration,
            (friend.loc, "Invalid friend declaration"),
            (mident.loc, msg),
        ));
        None
    } else if &mident == current_mident {
        context.env.add_diag(diag!(
            Declarations::InvalidFriendDeclaration,
            (friend.loc, "Invalid friend declaration"),
            (mident.loc, "Cannot declare the module itself as a friend"),
        ));
        None
    } else if context.resolve_module(&mident) {
        Some(friend)
    } else {
        assert!(context.env.has_errors());
        None
    }
}

//**************************************************************************************************
// Functions
//**************************************************************************************************

fn function(
    context: &mut Context,
    module: ModuleIdent,
    name: FunctionName,
    ef: E::Function,
) -> N::Function {
    let E::Function {
        warning_filter,
        index,
        attributes,
        loc: _,
        visibility,
        entry,
        signature,
        body,
    } = ef;
    assert!(!context.translating_fun);
    assert!(context.local_count.is_empty());
    assert!(context.local_scopes.is_empty());
    assert!(context.nominal_block_id == 0);
    assert!(context.used_fun_tparams.is_empty());
    assert!(context.used_locals.is_empty());
    context.env.add_warning_filter_scope(warning_filter.clone());
    context.local_scopes = vec![BTreeMap::new()];
    context.local_count = BTreeMap::new();
    context.translating_fun = true;
    let signature = function_signature(context, signature);
    let body = function_body(context, body);

    if !matches!(body.value, N::FunctionBody_::Native) {
        for tparam in &signature.type_parameters {
            if !context.used_fun_tparams.contains(&tparam.id) {
                let sp!(loc, n) = tparam.user_specified_name;
                let msg = format!("Unused type parameter '{}'.", n);
                context
                    .env
                    .add_diag(diag!(UnusedItem::FunTypeParam, (loc, msg)))
            }
        }
    }

    let mut f = N::Function {
        warning_filter,
        index,
        attributes,
        visibility,
        entry,
        signature,
        body,
    };
    fake_natives::function(context.env, module, name, &f);
    let used_locals = std::mem::take(&mut context.used_locals);
    remove_unused_bindings_function(context, &used_locals, &mut f);
    context.local_count = BTreeMap::new();
    context.local_scopes = vec![];
    context.nominal_block_id = 0;
    context.used_fun_tparams = BTreeSet::new();
    context.used_locals = BTreeSet::new();
    context.env.pop_warning_filter_scope();
    context.translating_fun = false;
    f
}

fn function_signature(context: &mut Context, sig: E::FunctionSignature) -> N::FunctionSignature {
    let type_parameters = fun_type_parameters(context, sig.type_parameters);

    let mut declared = UniqueMap::new();
    let parameters = sig
        .parameters
        .into_iter()
        .map(|(mut_, param, param_ty)| {
            if let Err((param, prev_loc)) = declared.add(param, ()) {
                if !param.is_underscore() {
                    let msg = format!("Duplicate parameter with name '{}'", param);
                    context.env.add_diag(diag!(
                        Declarations::DuplicateItem,
                        (param.loc(), msg),
                        (prev_loc, "Previously declared here"),
                    ))
                }
            }
            let is_parameter = true;
            let nparam = context.declare_local(is_parameter, param.0);
            let nparam_ty = type_(context, param_ty);
            (mut_, nparam, nparam_ty)
        })
        .collect();
    let return_type = type_(context, sig.return_type);
    N::FunctionSignature {
        type_parameters,
        parameters,
        return_type,
    }
}

fn function_body(context: &mut Context, sp!(loc, b_): E::FunctionBody) -> N::FunctionBody {
    match b_ {
        E::FunctionBody_::Native => sp(loc, N::FunctionBody_::Native),
        E::FunctionBody_::Defined(es) => sp(loc, N::FunctionBody_::Defined(sequence(context, es))),
    }
}

//**************************************************************************************************
// Structs
//**************************************************************************************************

fn struct_def(
    context: &mut Context,
    _name: DatatypeName,
    sdef: E::StructDefinition,
) -> N::StructDefinition {
    let E::StructDefinition {
        warning_filter,
        index,
        attributes,
        loc: _loc,
        abilities,
        type_parameters,
        fields,
    } = sdef;
    context.env.add_warning_filter_scope(warning_filter.clone());
    let type_parameters = datatype_type_parameters(context, type_parameters);
    let fields = struct_fields(context, fields);
    context.env.pop_warning_filter_scope();
    N::StructDefinition {
        warning_filter,
        index,
        attributes,
        abilities,
        type_parameters,
        fields,
    }
}

fn positional_field_name(loc: Loc, idx: usize) -> Field {
    Field::add_loc(loc, format!("{idx}").into())
}

fn struct_fields(context: &mut Context, efields: E::StructFields) -> N::StructFields {
    match efields {
        E::StructFields::Native(loc) => N::StructFields::Native(loc),
        E::StructFields::Named(em) => {
            N::StructFields::Defined(em.map(|_f, (idx, t)| (idx, type_(context, t))))
        }
        E::StructFields::Positional(tys) => {
            let fields = tys
                .into_iter()
                .map(|ty| type_(context, ty))
                .enumerate()
                .map(|(idx, ty)| {
                    let field_name = positional_field_name(ty.loc, idx);
                    (field_name, (idx, ty))
                });
            N::StructFields::Defined(UniqueMap::maybe_from_iter(fields).unwrap())
        }
    }
}

//**************************************************************************************************
// Enums
//**************************************************************************************************

fn enum_def(
    context: &mut Context,
    _name: DatatypeName,
    edef: E::EnumDefinition,
) -> N::EnumDefinition {
    let E::EnumDefinition {
        warning_filter,
        index,
        attributes,
        loc: _loc,
        abilities,
        type_parameters,
        variants,
    } = edef;
    context.env.add_warning_filter_scope(warning_filter.clone());
    let type_parameters = datatype_type_parameters(context, type_parameters);
    let variants = enum_variants(context, variants);
    context.env.pop_warning_filter_scope();
    N::EnumDefinition {
        warning_filter,
        index,
        attributes,
        abilities,
        type_parameters,
        variants,
    }
}

fn enum_variants(
    context: &mut Context,
    evariants: UniqueMap<VariantName, E::VariantDefinition>,
) -> UniqueMap<VariantName, N::VariantDefinition> {
    let variants = evariants
        .into_iter()
        .map(|(key, defn)| (key, variant_def(context, defn)));
    UniqueMap::maybe_from_iter(variants).unwrap()
}

fn variant_def(context: &mut Context, variant: E::VariantDefinition) -> N::VariantDefinition {
    let E::VariantDefinition { loc, index, fields } = variant;

    N::VariantDefinition {
        index,
        loc,
        fields: variant_fields(context, fields),
    }
}

fn variant_fields(context: &mut Context, efields: E::VariantFields) -> N::VariantFields {
    match efields {
        E::VariantFields::Empty => N::VariantFields::Empty,
        E::VariantFields::Named(em) => {
            N::VariantFields::Defined(em.map(|_f, (idx, t)| (idx, type_(context, t))))
        }
        E::VariantFields::Positional(tys) => {
            let fields = tys
                .into_iter()
                .map(|ty| type_(context, ty))
                .enumerate()
                .map(|(idx, ty)| {
                    let field_name = positional_field_name(ty.loc, idx);
                    (field_name, (idx, ty))
                });
            N::VariantFields::Defined(UniqueMap::maybe_from_iter(fields).unwrap())
        }
    }
}

//**************************************************************************************************
// Constants
//**************************************************************************************************

fn constant(context: &mut Context, _name: ConstantName, econstant: E::Constant) -> N::Constant {
    let E::Constant {
        warning_filter,
        index,
        attributes,
        loc,
        signature: esignature,
        value: evalue,
    } = econstant;
    assert!(context.local_scopes.is_empty());
    assert!(context.local_count.is_empty());
    assert!(context.used_locals.is_empty());
    context.env.add_warning_filter_scope(warning_filter.clone());
    context.local_scopes = vec![BTreeMap::new()];
    let signature = type_(context, esignature);
    let value = exp_(context, evalue);
    context.local_scopes = vec![];
    context.local_count = BTreeMap::new();
    context.used_locals = BTreeSet::new();
    context.nominal_block_id = 0;
    context.env.pop_warning_filter_scope();
    N::Constant {
        warning_filter,
        index,
        attributes,
        loc,
        signature,
        value,
    }
}

//**************************************************************************************************
// Types
//**************************************************************************************************

fn fun_type_parameters(
    context: &mut Context,
    type_parameters: Vec<(Name, AbilitySet)>,
) -> Vec<N::TParam> {
    let mut unique_tparams = UniqueMap::new();
    type_parameters
        .into_iter()
        .map(|(name, abilities)| type_parameter(context, &mut unique_tparams, name, abilities))
        .collect()
}

fn datatype_type_parameters(
    context: &mut Context,
    type_parameters: Vec<E::DatatypeTypeParameter>,
) -> Vec<N::DatatypeTypeParameter> {
    let mut unique_tparams = UniqueMap::new();
    type_parameters
        .into_iter()
        .map(|param| {
            let is_phantom = param.is_phantom;
            let param = type_parameter(context, &mut unique_tparams, param.name, param.constraints);
            N::DatatypeTypeParameter { param, is_phantom }
        })
        .collect()
}

fn type_parameter(
    context: &mut Context,
    unique_tparams: &mut UniqueMap<Name, ()>,
    name: Name,
    abilities: AbilitySet,
) -> N::TParam {
    let id = N::TParamID::next();
    let user_specified_name = name;
    let tp = N::TParam {
        id,
        user_specified_name,
        abilities,
    };
    let loc = name.loc;
    context.bind_type(name.value, ResolvedType::TParam(loc, tp.clone()));
    if let Err((name, old_loc)) = unique_tparams.add(name, ()) {
        let msg = format!("Duplicate type parameter declared with name '{}'", name);
        context.env.add_diag(diag!(
            Declarations::DuplicateItem,
            (loc, msg),
            (old_loc, "Type parameter previously defined here"),
        ))
    }
    tp
}

fn types(context: &mut Context, tys: Vec<E::Type>) -> Vec<N::Type> {
    tys.into_iter().map(|t| type_(context, t)).collect()
}

fn type_(context: &mut Context, sp!(loc, ety_): E::Type) -> N::Type {
    use ResolvedType as RT;
    use E::Type_ as ET;
    use N::{TypeName_ as NN, Type_ as NT};
    let ty_ = match ety_ {
        ET::Unit => NT::Unit,
        ET::Multiple(tys) => {
            NT::multiple_(loc, tys.into_iter().map(|t| type_(context, t)).collect())
        }
        ET::Ref(mut_, inner) => NT::Ref(mut_, Box::new(type_(context, *inner))),
        ET::UnresolvedError => {
            assert!(context.env.has_errors());
            NT::UnresolvedError
        }
        ET::Apply(ma, tys) => match context.resolve_type(ma) {
            RT::Unbound => {
                assert!(context.env.has_errors());
                NT::UnresolvedError
            }
            RT::BuiltinType(bn_) => {
                let name_f = || format!("{}", &bn_);
                let arity = bn_.tparam_constraints(loc).len();
                let tys = types(context, tys);
                let tys = check_type_argument_arity(context, loc, name_f, tys, arity);
                NT::builtin_(sp(ma.loc, bn_), tys)
            }
            RT::TParam(_, tp) => {
                if !tys.is_empty() {
                    context.env.add_diag(diag!(
                        NameResolution::NamePositionMismatch,
                        (loc, "Generic type parameters cannot take type arguments"),
                    ));
                    NT::UnresolvedError
                } else {
                    if context.translating_fun {
                        context.used_fun_tparams.insert(tp.id);
                    }
                    NT::Param(tp)
                }
            }
            RT::ModuleType(mt) => {
                let ResolvedModuleType {
                    original_loc,
                    original_type_name,
                    module_type,
                } = *mt;
                let (tn, arity) = match module_type {
                    ModuleType::Struct(stype) => {
                        let tn = sp(
                            original_loc,
                            NN::ModuleType(stype.original_mident, DatatypeName(original_type_name)),
                        );
                        let arity = stype.arity;
                        (tn, arity)
                    }
                    ModuleType::Enum(etype) => {
                        let tn = sp(
                            original_loc,
                            NN::ModuleType(etype.original_mident, DatatypeName(original_type_name)),
                        );
                        let arity = etype.arity;
                        (tn, arity)
                    }
                };
                let tys = types(context, tys);
                let name_f = || format!("{}", tn);
                let tys = check_type_argument_arity(context, loc, name_f, tys, arity);
                NT::Apply(None, tn, tys)
            }
        },
        ET::Fun(_, _) => panic!("ICE only allowed in spec context"),
    };
    sp(loc, ty_)
}

fn check_type_argument_arity<F: FnOnce() -> String>(
    context: &mut Context,
    loc: Loc,
    name_f: F,
    mut ty_args: Vec<N::Type>,
    arity: usize,
) -> Vec<N::Type> {
    let args_len = ty_args.len();
    if args_len != arity {
        let diag_code = if args_len > arity {
            NameResolution::TooManyTypeArguments
        } else {
            NameResolution::TooFewTypeArguments
        };
        let msg = format!(
            "Invalid instantiation of '{}'. Expected {} type argument(s) but got {}",
            name_f(),
            arity,
            args_len
        );
        context.env.add_diag(diag!(diag_code, (loc, msg)));
    }

    while ty_args.len() > arity {
        ty_args.pop();
    }

    while ty_args.len() < arity {
        ty_args.push(sp(loc, N::Type_::UnresolvedError))
    }

    ty_args
}

//**************************************************************************************************
// Exp
//**************************************************************************************************

fn sequence(context: &mut Context, (euse_funs, seq): E::Sequence) -> N::Sequence {
    context.new_local_scope();
    let nuse_funs = use_funs(context, euse_funs);
    let nseq = seq.into_iter().map(|s| sequence_item(context, s)).collect();
    context.close_local_scope();
    (nuse_funs, nseq)
}

fn sequence_item(context: &mut Context, sp!(loc, ns_): E::SequenceItem) -> N::SequenceItem {
    use E::SequenceItem_ as ES;
    use N::SequenceItem_ as NS;

    let s_ = match ns_ {
        ES::Seq(e) => NS::Seq(exp_(context, e)),
        ES::Declare(b, ty_opt) => {
            let bind_opt = bind_list(context, b);
            let tys = ty_opt.map(|t| type_(context, t));
            match bind_opt {
                None => {
                    assert!(context.env.has_errors());
                    NS::Seq(sp(loc, N::Exp_::UnresolvedError))
                }
                Some(bind) => NS::Declare(bind, tys),
            }
        }
        ES::Bind(b, e) => {
            let e = exp_(context, e);
            let bind_opt = bind_list(context, b);
            match bind_opt {
                None => {
                    assert!(context.env.has_errors());
                    NS::Seq(sp(loc, N::Exp_::UnresolvedError))
                }
                Some(bind) => NS::Bind(bind, e),
            }
        }
    };
    sp(loc, s_)
}

fn call_args(context: &mut Context, sp!(loc, es): Spanned<Vec<E::Exp>>) -> Spanned<Vec<N::Exp>> {
    sp(loc, exps(context, es))
}

fn exps(context: &mut Context, es: Vec<E::Exp>) -> Vec<N::Exp> {
    es.into_iter().map(|e| exp_(context, e)).collect()
}

fn exp(context: &mut Context, e: E::Exp) -> Box<N::Exp> {
    Box::new(exp_(context, e))
}

fn exp_(context: &mut Context, e: E::Exp) -> N::Exp {
    use E::Exp_ as EE;
    use N::Exp_ as NE;
    let sp!(eloc, e_) = e;
    let ne_ = match e_ {
        EE::Unit { trailing } => NE::Unit { trailing },
        EE::Value(val) => NE::Value(val),
        EE::Name(sp!(aloc, E::ModuleAccess_::Name(v)), None) => {
            if is_constant_name(&v.value) {
                access_constant(context, sp(aloc, E::ModuleAccess_::Name(v)))
            } else {
                match context.resolve_local(eloc, |name| format!("Unbound variable '{name}'"), v) {
                    None => {
                        debug_assert!(context.env.has_errors());
                        NE::UnresolvedError
                    }
                    Some(nv) => NE::Var(nv),
                }
            }
        }
        EE::Name(ma @ sp!(_, E::ModuleAccess_::Variant(_, _)), etys_opt) => {
            context
                .env
                .check_feature(FeatureGate::Enums, context.current_package, eloc);
            match context.resolve_variant_name(eloc, "construction", ma, etys_opt) {
                None => {
                    assert!(context.env.has_errors());
                    NE::UnresolvedError
                }
                Some((m, en, vn, tys_opt, dloc, is_positional, is_empty)) => {
                    if !is_empty {
                        let msg =
                            "Invalid variant instantiation. Non-empty variant instantiations \
                                   require arguments";
                        let defn_msg = "Variant is defined here.";
                        let mut diag = diag!(
                            NameResolution::PositionalCallMismatch,
                            (eloc, msg),
                            (dloc, defn_msg)
                        );
                        if is_positional {
                            diag.add_note("Pass arguments to positional variants using '()'");
                        } else {
                            diag.add_note("Pass arguments to named variant fields using '{ .. }'");
                        }
                        context.env.add_diag(diag);
                    }

                    NE::PackVariant(m, en, vn, tys_opt, UniqueMap::new())
                }
            }
        }
        EE::Name(ma, None) => access_constant(context, ma),

        EE::IfElse(eb, et, ef) => {
            NE::IfElse(exp(context, *eb), exp(context, *et), exp(context, *ef))
        }
        EE::Match(esubject, sp!(aloc, arms)) => NE::Match(
            exp(context, *esubject),
            sp(
                aloc,
                arms.into_iter()
                    .map(|arm| match_arm(context, arm))
                    .collect(),
            ),
        ),
        EE::While(eb, name_opt, el) => {
            let cond = exp(context, *eb);
            context.enter_nominal_block(name_opt, NominalBlockType::Loop);
            let body = exp(context, *el);
            NE::While(cond, context.exit_nominal_block(eloc), body)
        }
        EE::Loop(name_opt, el) => {
            context.enter_nominal_block(name_opt, NominalBlockType::Loop);
            let body = exp(context, *el);
            NE::Loop(context.exit_nominal_block(eloc), body)
        }
        EE::NamedBlock(name, seq) => {
            context.enter_nominal_block(Some(name), NominalBlockType::Block);
            let body = sequence(context, seq);
            NE::NamedBlock(context.exit_nominal_block(eloc), body)
        }
        EE::Block(seq) => NE::Block(sequence(context, seq)),

        EE::Assign(a, e) => {
            let na_opt = assign_list(context, a);
            let ne = exp(context, *e);
            match na_opt {
                None => {
                    assert!(context.env.has_errors());
                    NE::UnresolvedError
                }
                Some(na) => NE::Assign(na, ne),
            }
        }
        EE::FieldMutate(edotted, er) => {
            let ndot_opt = dotted(context, *edotted);
            let ner = exp(context, *er);
            match ndot_opt {
                None => {
                    assert!(context.env.has_errors());
                    NE::UnresolvedError
                }
                Some(ndot) => NE::FieldMutate(ndot, ner),
            }
        }
        EE::Mutate(el, er) => {
            let nel = exp(context, *el);
            let ner = exp(context, *er);
            NE::Mutate(nel, ner)
        }

        EE::Abort(es) => NE::Abort(exp(context, *es)),
        EE::Return(name_opt, es) => {
            let out_rhs = exp(context, *es);
            if let Some(block_name) = name_opt {
                if let Some(return_name) =
                    context.resolve_nominal_label("return", NominalBlockType::Block, block_name)
                {
                    NE::Give(return_name, out_rhs)
                } else {
                    NE::UnresolvedError
                }
            } else {
                NE::Return(out_rhs)
            }
        }
        EE::Break(name_opt, rhs) => {
            let out_rhs = exp(context, *rhs);
            if let Some(loop_name) = name_opt {
                context
                    .resolve_nominal_label("break", NominalBlockType::Loop, loop_name)
                    .map(|name| NE::Give(name, out_rhs))
                    .unwrap_or_else(|| NE::UnresolvedError)
            } else {
                context
                    .current_loop(eloc)
                    .map(|name| NE::Give(name, out_rhs))
                    .unwrap_or_else(|| {
                        let msg = "Invalid usage of 'break'. \
                            'break' can only be used inside a loop body";
                        context
                            .env
                            .add_diag(diag!(TypeSafety::InvalidLoopControl, (eloc, msg)));
                        NE::UnresolvedError
                    })
            }
        }
        EE::Continue(name_opt) => {
            if let Some(loop_name) = name_opt {
                context
                    .resolve_nominal_label("continue", NominalBlockType::Loop, loop_name)
                    .map(NE::Continue)
                    .unwrap_or_else(|| NE::UnresolvedError)
            } else {
                context
                    .current_loop(eloc)
                    .map(NE::Continue)
                    .unwrap_or_else(|| {
                        let msg = "Invalid usage of 'continue'. \
                            'continue' can only be used inside a loop body";
                        context
                            .env
                            .add_diag(diag!(TypeSafety::InvalidLoopControl, (eloc, msg)));
                        NE::UnresolvedError
                    })
            }
        }

        EE::Dereference(e) => NE::Dereference(exp(context, *e)),
        EE::UnaryExp(uop, e) => NE::UnaryExp(uop, exp(context, *e)),

        e_ @ EE::BinopExp(..) => {
            process_binops!(
                (P::BinOp, Loc),
                Box<N::Exp>,
                Box::new(sp(eloc, e_)),
                e,
                *e,
                sp!(loc, EE::BinopExp(lhs, op, rhs)) => { (lhs, (op, loc), rhs) },
                { exp(context, *e) },
                value_stack,
                (bop, loc) => {
                    let el = value_stack.pop().expect("ICE binop naming issue");
                    let er = value_stack.pop().expect("ICE binop naming issue");
                    Box::new(sp(loc, NE::BinopExp(el, bop, er)))
                }
            )
            .value
        }

        EE::Pack(ma @ sp!(_, E::ModuleAccess_::Variant(_, _)), etys_opt, efields) => {
            context
                .env
                .check_feature(FeatureGate::Enums, context.current_package, eloc);
            let fields = efields.map(|_, (idx, e)| (idx, exp_(context, e)));
            match context.resolve_variant_name(eloc, "construction", ma, etys_opt) {
                None => {
                    assert!(context.env.has_errors());
                    NE::UnresolvedError
                }
                Some((m, en, vn, tys_opt, dloc, is_positional, is_empty)) => {
                    if is_empty {
                        let msg = "Invalid variant instantiation. Empty variant instantiations \
                                   do not use field syntax";
                        let defn_msg = "Variant is defined here.";
                        let mut diag = diag!(
                            NameResolution::PositionalCallMismatch,
                            (eloc, msg),
                            (dloc, defn_msg)
                        );
                        diag.add_note("Remove '{}' after the variant name");
                        context.env.add_diag(diag);
                    } else if is_positional {
                        let msg = "Invalid variant instantiation. Positional variant fields \
                                   require positional instantiations.";
                        let defn_msg = "Variant is defined here.";
                        context.env.add_diag(diag!(
                            NameResolution::PositionalCallMismatch,
                            (eloc, msg),
                            (dloc, defn_msg)
                        ));
                    }

                    NE::PackVariant(m, en, vn, tys_opt, fields)
                }
            }
        }

        EE::Pack(tn, etys_opt, efields) => {
            let fields = efields.map(|_, (idx, e)| (idx, exp_(context, e)));
            match context.resolve_struct_name(eloc, "construction", tn, etys_opt) {
                None => {
                    assert!(context.env.has_errors());
                    NE::UnresolvedError
                }
                Some((m, sn, tys_opt, is_positional)) => {
                    if is_positional {
                        let msg = "Invalid struct instantiation. Positional struct declarations \
                             require positional instantiations.";
                        context
                            .env
                            .add_diag(diag!(NameResolution::PositionalCallMismatch, (eloc, msg)));
                    }
                    NE::Pack(m, sn, tys_opt, fields)
                }
            }
        }
        EE::ExpList(es) => {
            assert!(es.len() > 1);
            NE::ExpList(exps(context, es))
        }

        EE::ExpDotted(case, edot) => match dotted(context, *edot) {
            None => {
                assert!(context.env.has_errors());
                NE::UnresolvedError
            }
            Some(d) => NE::ExpDotted(case, d),
        },

        EE::Cast(e, t) => NE::Cast(exp(context, *e), type_(context, t)),
        EE::Annotate(e, t) => NE::Annotate(exp(context, *e), type_(context, t)),

        EE::Call(sp!(mloc, ma_), true, tys_opt, rhs) => {
            use E::ModuleAccess_ as EA;
            use N::BuiltinFunction_ as BF;
            assert!(tys_opt.is_none(), "ICE macros do not have type arguments");
            let nes = call_args(context, rhs);
            match ma_ {
                EA::Name(n) if n.value.as_str() == BF::ASSERT_MACRO => {
                    NE::Builtin(sp(mloc, BF::Assert(true)), nes)
                }
                ma_ => {
                    context.env.add_diag(diag!(
                        NameResolution::UnboundMacro,
                        (mloc, format!("Unbound macro '{}'", ma_)),
                    ));
                    NE::UnresolvedError
                }
            }
        }
        EE::Call(ma, false, tys_opt, rhs) if context.resolves_to_struct(&ma) => {
            context
                .env
                .check_feature(FeatureGate::PositionalFields, context.current_package, eloc);
            let nes = call_args(context, rhs);
            match context.resolve_struct_name(eloc, "construction", ma, tys_opt) {
                None => {
                    assert!(context.env.has_errors());
                    NE::UnresolvedError
                }
                Some((m, sn, tys_opt, is_positional)) => {
                    if !is_positional {
                        let msg = "Invalid struct instantiation. Named struct declarations \
                                   require named instantiations.";
                        context
                            .env
                            .add_diag(diag!(NameResolution::PositionalCallMismatch, (eloc, msg)));
                    }
                    NE::Pack(
                        m,
                        sn,
                        tys_opt,
                        UniqueMap::maybe_from_iter(nes.value.into_iter().enumerate().map(
                            |(idx, e)| {
                                let field = Field::add_loc(e.loc, format!("{idx}").into());
                                (field, (idx, e))
                            },
                        ))
                        .unwrap(),
                    )
                }
            }
        }
        EE::Call(ma @ sp!(_, E::ModuleAccess_::Variant(_, _)), false, tys_opt, rhs) => {
            context
                .env
                .check_feature(FeatureGate::Enums, context.current_package, eloc);
            let nes = call_args(context, rhs);
            match context.resolve_variant_name(eloc, "construction", ma, tys_opt) {
                None => {
                    assert!(context.env.has_errors());
                    NE::UnresolvedError
                }
                Some((m, en, vn, tys_opt, dloc, is_positional, is_empty)) => {
                    if is_empty {
                        let msg = "Invalid variant instantiation. Empty variant instantiations \
                                   do not use call syntax";
                        let defn_msg = "Variant is defined here.";
                        let mut diag = diag!(
                            NameResolution::PositionalCallMismatch,
                            (eloc, msg),
                            (dloc, defn_msg)
                        );
                        diag.add_note("Remove '()' after the variant name");
                        context.env.add_diag(diag);
                    } else if !is_positional {
                        let msg = "Invalid variant instantiation. Named variant fields \
                                   require named instantiations.";
                        let defn_msg = "Variant is defined here.";
                        context.env.add_diag(diag!(
                            NameResolution::PositionalCallMismatch,
                            (eloc, msg),
                            (dloc, defn_msg)
                        ));
                    }

                    NE::PackVariant(
                        m,
                        en,
                        vn,
                        tys_opt,
                        UniqueMap::maybe_from_iter(nes.value.into_iter().enumerate().map(
                            |(idx, e)| {
                                let field = Field::add_loc(e.loc, format!("{idx}").into());
                                (field, (idx, e))
                            },
                        ))
                        .unwrap(),
                    )
                }
            }
        }
        EE::Call(ma, false, tys_opt, rhs) => {
            let ty_args = tys_opt.map(|tys| types(context, tys));
            let nes = call_args(context, rhs);
            match resolve_function(context, eloc, ma, ty_args) {
                ResolvedFunction::Builtin(f) => NE::Builtin(f, nes),
                ResolvedFunction::Module(mf) => {
                    let ResolvedModuleFunction {
                        module,
                        function,
                        ty_args,
                    } = *mf;
                    NE::ModuleCall(module, function, ty_args, nes)
                }
                ResolvedFunction::Unbound => {
                    assert!(context.env.has_errors());
                    NE::UnresolvedError
                }
            }
        }
        EE::MethodCall(edot, n, tys_opt, rhs) => match dotted(context, *edot) {
            None => {
                assert!(context.env.has_errors());
                NE::UnresolvedError
            }
            Some(d) => {
                let ty_args = tys_opt.map(|tys| types(context, tys));
                let nes = call_args(context, rhs);
                NE::MethodCall(d, n, ty_args, nes)
            }
        },
        EE::Vector(vec_loc, tys_opt, rhs) => {
            let ty_args = tys_opt.map(|tys| types(context, tys));
            let nes = call_args(context, rhs);
            let ty_opt = check_builtin_ty_args_impl(
                context,
                vec_loc,
                || "Invalid 'vector' instantation".to_string(),
                eloc,
                1,
                ty_args,
            )
            .map(|mut v| {
                assert!(v.len() == 1);
                v.pop().unwrap()
            });
            NE::Vector(vec_loc, ty_opt, nes)
        }

        EE::UnresolvedError => {
            assert!(context.env.has_errors());
            NE::UnresolvedError
        }
        // `Name` matches name variants only allowed in specs (we handle the allowed ones above)
        EE::Index(..) | EE::Lambda(..) | EE::Quant(..) | EE::Name(_, Some(_)) => {
            panic!("ICE unexpected specification construct")
        }
    };
    sp(eloc, ne_)
}

fn access_constant(context: &mut Context, ma: E::ModuleAccess) -> N::Exp_ {
    match context.resolve_constant(ma) {
        None => {
            assert!(context.env.has_errors());
            N::Exp_::UnresolvedError
        }
        Some((m, c)) => N::Exp_::Constant(m, c),
    }
}

fn dotted(context: &mut Context, edot: E::ExpDotted) -> Option<N::ExpDotted> {
    let sp!(loc, edot_) = edot;
    let nedot_ = match edot_ {
        E::ExpDotted_::Exp(e) => {
            let ne = exp(context, *e);
            match &ne.value {
                N::Exp_::UnresolvedError => return None,
                _ => N::ExpDotted_::Exp(ne),
            }
        }
        E::ExpDotted_::Dot(d, f) => N::ExpDotted_::Dot(Box::new(dotted(context, *d)?), Field(f)),
    };
    Some(sp(loc, nedot_))
}

//************************************************
// Match Arms and Patterns
//************************************************

fn match_arm(context: &mut Context, sp!(aloc, arm): E::MatchArm) -> N::MatchArm {
    let E::MatchArm_ {
        pattern,
        binders,
        guard,
        rhs,
    } = arm;
    context.new_local_scope();
    // NB: we already checked the binders for duplicates and listed them all during expansion, so
    // now we just need to set up the map and recur down everything.
    for binder in &binders {
        context.declare_local(false, binder.0);
    }

    let mut binder_map: BTreeMap<P::Var, N::Var> = binders
        .clone()
        .into_iter()
        .map(|binder| {
            let rhs = context
                .as_var(binder.loc(), "pattern", binder.0)
                .expect("ICE pattern binder failure");
            (binder, rhs)
        })
        .collect::<BTreeMap<_, _>>();

    // Guards are a little tricky: we need them to have similar binders, but they must be different
    // because they may be typed differently than the actual binders (as the are always immutable
    // references). So we push a new scope with new binders that map to the old ones, process the
    // guard, then see which of those new binders are actually used in the the guard.
    context.new_local_scope();
    for binder in &binders {
        context.declare_local(false, binder.0);
    }
    let guard_binder_map: BTreeMap<P::Var, N::Var> = binders
        .clone()
        .into_iter()
        .map(|binder| {
            let rhs = context
                .as_var(binder.loc(), "pattern", binder.0)
                .expect("ICE pattern binder failure");
            (binder, rhs)
        })
        .collect::<BTreeMap<_, _>>();
    let guard = guard.map(|guard| exp(context, *guard));
    // To compute the variables used in the guard, we process the guard expression and then filter
    // our current binders into those that are marked as used locals.
    let mut guard_binders: UniqueMap<N::Var, N::Var> = UniqueMap::new();
    for (var, guard_name) in guard_binder_map {
        if context.used_locals.contains(&guard_name.value) {
            let pat_name = binder_map
                .remove(&var)
                .expect("ICE pattern binder naming failure");
            guard_binders
                .add(pat_name, guard_name)
                .expect("ICE pattern binder naming failure");
        }
    }
    context.close_local_scope();

    // Next we visit the right-hand side to mark binder usage there.
    let rhs = exp(context, *rhs);

    let binders: Vec<N::Var> = binders
        .iter()
        .map(|binder| {
            context
                .as_var(binder.loc(), "pattern", binder.0)
                .expect("ICE pattern binder failure")
        })
        .collect();

    let rhs_binders: BTreeSet<N::Var> = binders
        .clone()
        .into_iter()
        .filter(|binder| context.used_locals.contains(&binder.value))
        .collect();

    // Mark all pattern variables used in the guard as used so that they don't get reported as
    // unused. For example `Temp::Celcius { temp } if (temp > 0) => true` should makr `temp` as
    // used.
    for (key, _) in guard_binders.key_cloned_iter() {
        context.used_locals.insert(key.value);
    }

    // Finally we handle the pattern. We record any binders used in the right-hand side separately
    // so that we can avoid binding unused ones later.

    let pattern = pat(context, pattern);

    context.close_local_scope();
    let arm = N::MatchArm_ {
        pattern,
        binders,
        guard,
        guard_binders,
        rhs_binders,
        rhs,
    };
    sp(aloc, arm)
}

fn pat(context: &mut Context, sp!(ploc, pat_): E::MatchPattern) -> N::MatchPattern {
    use E::MatchPattern_ as EP;
    use N::MatchPattern_ as NP;

    let pat_: N::MatchPattern_ = match pat_ {
        EP::PositionalConstructor(name, etys_opt, args) => {
            if let Some((mident, enum_, variant, tys_opt, _, is_positional, is_empty)) =
                context.resolve_variant_name(ploc, "pattern", name, etys_opt)
            {
                if is_empty {
                    let msg = "Invalid variant pattern. Empty variants \
                               are not matched with positional variant syntax";
                    let mut diag = diag!(NameResolution::PositionalCallMismatch, (ploc, msg));
                    diag.add_note("Remove '()' after the variant name");
                    context.env.add_diag(diag);
                } else if !is_positional {
                    let msg = "Invalid variant pattern. Named variant declarations \
                                   require named patterns.";
                    context
                        .env
                        .add_diag(diag!(NameResolution::PositionalCallMismatch, (ploc, msg)));
                }

                let args = UniqueMap::maybe_from_iter(args.value.into_iter().enumerate().map(
                    |(idx, p)| {
                        let field = Field::add_loc(p.loc, format!("{idx}").into());
                        (field, (idx, pat(context, p)))
                    },
                ))
                .unwrap();
                NP::Constructor(mident, enum_, variant, tys_opt, args)
            } else {
                assert!(context.env.has_errors());
                NP::ErrorPat
            }
        }
        EP::FieldConstructor(name, etys_opt, args) => {
            if let Some((mident, enum_, variant, tys_opt, _, is_positional, is_empty)) =
                context.resolve_variant_name(ploc, "pattern", name, etys_opt)
            {
                if is_empty {
                    let msg = "Invalid variant pattern. Empty variants \
                               are not matched with variant field syntax";
                    let mut diag = diag!(NameResolution::PositionalCallMismatch, (ploc, msg));
                    diag.add_note("Remove '{}' after the variant name");
                    context.env.add_diag(diag);
                } else if is_positional {
                    let msg = "Invalid variant pattern. Positional variant declarations \
                                   require positional patterns.";
                    context
                        .env
                        .add_diag(diag!(NameResolution::PositionalCallMismatch, (ploc, msg)));
                }

                let args = args.map(|_, (idx, p)| (idx, pat(context, p)));
                NP::Constructor(mident, enum_, variant, tys_opt, args)
            } else {
                assert!(context.env.has_errors());
                NP::ErrorPat
            }
        }
        EP::HeadConstructor(name, etys_opt) => {
            if let Some((mident, enum_, variant, tys_opt, _, _is_positional, _is_empty)) =
                context.resolve_variant_name(ploc, "pattern", name, etys_opt)
            {
                // No need to chck is_empty / is_positional because typing will report the errors.
                NP::Constructor(mident, enum_, variant, tys_opt, UniqueMap::new())
            } else {
                assert!(context.env.has_errors());
                NP::ErrorPat
            }
        }
        EP::Binder(binder) => {
            let binder = context
                .resolve_local(
                    binder.loc(),
                    |name| format!("Could not resolve '{name}' pattern"),
                    binder.0,
                )
                .expect("ICE pattern naming error");
            NP::Binder(binder)
        }
        EP::Literal(v) => NP::Literal(v),
        EP::Wildcard => NP::Wildcard,
        EP::Or(lhs, rhs) => NP::Or(Box::new(pat(context, *lhs)), Box::new(pat(context, *rhs))),
        EP::At(binder, body) => {
            let binder = context
                .resolve_local(
                    binder.loc(),
                    |name| format!("Could not resolve '{name}' pattern"),
                    binder.0,
                )
                .expect("ICE pattern naming error");
            NP::At(binder, Box::new(pat(context, *body)))
        }
    };
    sp(ploc, pat_)
}

//************************************************
// LValues
//************************************************

#[derive(Clone, Copy)]
enum LValueCase {
    Bind,
    Assign,
}

fn lvalue(
    context: &mut Context,
    seen_locals: &mut UniqueMap<Name, ()>,
    case: LValueCase,
    sp!(loc, l_): E::LValue,
) -> Option<N::LValue> {
    use LValueCase as C;
    use E::LValue_ as EL;
    use N::LValue_ as NL;
    let nl_ = match l_ {
        EL::Var(mut_, sp!(_, E::ModuleAccess_::Name(n)), None) => {
            let v = P::Var(n);
            if v.is_underscore() {
                NL::Ignore
            } else {
                if let Err((var, prev_loc)) = seen_locals.add(n, ()) {
                    let (primary, secondary) = match case {
                        C::Bind => {
                            let msg = format!(
                                "Duplicate declaration for local '{}' in a given 'let'",
                                &var
                            );
                            ((var.loc, msg), (prev_loc, "Previously declared here"))
                        }
                        C::Assign => {
                            let msg = format!(
                                "Duplicate usage of local '{}' in a given assignment",
                                &var
                            );
                            ((var.loc, msg), (prev_loc, "Previously assigned here"))
                        }
                    };
                    context
                        .env
                        .add_diag(diag!(Declarations::DuplicateItem, primary, secondary));
                }
                let nv = match case {
                    C::Bind => {
                        let is_parameter = false;
                        context.declare_local(is_parameter, n)
                    }
                    C::Assign => context.resolve_local(
                        loc,
                        |name| format!("Invalid assignment. Unbound variable '{name}'"),
                        n,
                    )?,
                };
                NL::Var {
                    mut_,
                    var: nv,
                    // set later
                    unused_binding: false,
                }
            }
        }
        EL::Unpack(tn, etys_opt, efields) => {
            let msg = match case {
                C::Bind => "deconstructing binding",
                C::Assign => "deconstructing assignment",
            };
            let (m, sn, tys_opt, is_positional) =
                context.resolve_struct_name(loc, msg, tn, etys_opt)?;
            if is_positional && !matches!(efields, E::FieldBindings::Positional(_)) {
                let msg = "Invalid deconstruction. Positional struct field declarations require \
                           positional deconstruction";
                context
                    .env
                    .add_diag(diag!(NameResolution::PositionalCallMismatch, (loc, msg)));
            }

            if !is_positional && matches!(efields, E::FieldBindings::Positional(_)) {
                let msg = "Invalid deconstruction. Named struct field declarations require \
                           named deconstruction";
                context
                    .env
                    .add_diag(diag!(NameResolution::PositionalCallMismatch, (loc, msg)));
            }
            let efields = match efields {
                E::FieldBindings::Named(efields) => efields,
                E::FieldBindings::Positional(lvals) => {
                    let lvals = lvals.into_iter().enumerate().map(|(idx, l)| {
                        let field_name = Field::add_loc(l.loc, format!("{idx}").into());
                        (field_name, (idx, l))
                    });
                    UniqueMap::maybe_from_iter(lvals).unwrap()
                }
            };
            let nfields =
                UniqueMap::maybe_from_opt_iter(efields.into_iter().map(|(k, (idx, inner))| {
                    Some((k, (idx, lvalue(context, seen_locals, case, inner)?)))
                }))?;
            NL::Unpack(
                m,
                sn,
                tys_opt,
                nfields.expect("ICE fields were already unique"),
            )
        }
        EL::Var(_, _, _) => panic!("unexpected specification construct"),
    };
    Some(sp(loc, nl_))
}

fn bind_list(context: &mut Context, ls: E::LValueList) -> Option<N::LValueList> {
    lvalue_list(context, &mut UniqueMap::new(), LValueCase::Bind, ls)
}

fn assign_list(context: &mut Context, ls: E::LValueList) -> Option<N::LValueList> {
    lvalue_list(context, &mut UniqueMap::new(), LValueCase::Assign, ls)
}

fn lvalue_list(
    context: &mut Context,
    seen_locals: &mut UniqueMap<Name, ()>,
    case: LValueCase,
    sp!(loc, b_): E::LValueList,
) -> Option<N::LValueList> {
    Some(sp(
        loc,
        b_.into_iter()
            .map(|inner| lvalue(context, seen_locals, case, inner))
            .collect::<Option<_>>()?,
    ))
}

fn resolve_function(
    context: &mut Context,
    loc: Loc,
    sp!(mloc, ma_): E::ModuleAccess,
    ty_args: Option<Vec<N::Type>>,
) -> ResolvedFunction {
    use E::ModuleAccess_ as EA;
    match ma_ {
        EA::Name(n) if N::BuiltinFunction_::all_names().contains(&n.value) => {
            match resolve_builtin_function(context, loc, &n, ty_args) {
                None => {
                    assert!(context.env.has_errors());
                    ResolvedFunction::Unbound
                }
                Some(f) => ResolvedFunction::Builtin(sp(mloc, f)),
            }
        }
        EA::Name(n) => {
            context.env.add_diag(diag!(
                NameResolution::UnboundUnscopedName,
                (n.loc, format!("Unbound function '{}' in current scope", n)),
            ));
            ResolvedFunction::Unbound
        }
        EA::ModuleAccess(m, n) => match context.resolve_module_function(mloc, &m, &n) {
            None => {
                assert!(context.env.has_errors());
                ResolvedFunction::Unbound
            }
            Some(_) => ResolvedFunction::Module(Box::new(ResolvedModuleFunction {
                module: m,
                function: FunctionName(n),
                ty_args,
            })),
        },
        EA::Variant(_, _) => panic!("ICE tried to resolve variant as function"),
    }
}

fn resolve_builtin_function(
    context: &mut Context,
    loc: Loc,
    b: &Name,
    ty_args: Option<Vec<N::Type>>,
) -> Option<N::BuiltinFunction_> {
    use N::{BuiltinFunction_ as B, BuiltinFunction_::*};
    Some(match b.value.as_str() {
        B::FREEZE => Freeze(check_builtin_ty_arg(context, loc, b, ty_args)),
        B::ASSERT_MACRO => {
            let dep_msg = format!(
                "'{}' function syntax has been deprecated and will be removed",
                B::ASSERT_MACRO
            );
            // TODO make this a tip/hint?
            let help_msg = format!(
                "Replace with '{0}!'. '{0}' has been replaced with a '{0}!' built-in macro so \
                 that arguments are no longer eagerly evaluated",
                B::ASSERT_MACRO
            );
            context.env.add_diag(diag!(
                Uncategorized::DeprecatedWillBeRemoved,
                (b.loc, dep_msg),
                (b.loc, help_msg),
            ));
            check_builtin_ty_args(context, loc, b, 0, ty_args);
            Assert(false)
        }
        _ => {
            context.env.add_diag(diag!(
                NameResolution::UnboundUnscopedName,
                (b.loc, format!("Unbound function: '{}'", b)),
            ));
            return None;
        }
    })
}

fn check_builtin_ty_arg(
    context: &mut Context,
    loc: Loc,
    b: &Name,
    ty_args: Option<Vec<N::Type>>,
) -> Option<N::Type> {
    let res = check_builtin_ty_args(context, loc, b, 1, ty_args);
    res.map(|mut v| {
        assert!(v.len() == 1);
        v.pop().unwrap()
    })
}

fn check_builtin_ty_args(
    context: &mut Context,
    loc: Loc,
    b: &Name,
    arity: usize,
    ty_args: Option<Vec<N::Type>>,
) -> Option<Vec<N::Type>> {
    check_builtin_ty_args_impl(
        context,
        b.loc,
        || format!("Invalid call to builtin function: '{}'", b),
        loc,
        arity,
        ty_args,
    )
}

fn check_builtin_ty_args_impl(
    context: &mut Context,
    msg_loc: Loc,
    fmsg: impl Fn() -> String,
    targs_loc: Loc,
    arity: usize,
    ty_args: Option<Vec<N::Type>>,
) -> Option<Vec<N::Type>> {
    let mut msg_opt = None;
    ty_args.map(|mut args| {
        let args_len = args.len();
        if args_len != arity {
            let diag_code = if args_len > arity {
                NameResolution::TooManyTypeArguments
            } else {
                NameResolution::TooFewTypeArguments
            };
            let msg = msg_opt.get_or_insert_with(fmsg);
            let targs_msg = format!("Expected {} type argument(s) but got {}", arity, args_len);
            context
                .env
                .add_diag(diag!(diag_code, (msg_loc, msg), (targs_loc, targs_msg)));
        }

        while args.len() > arity {
            args.pop();
        }

        while args.len() < arity {
            args.push(sp(targs_loc, N::Type_::UnresolvedError));
        }

        args
    })
}

//**************************************************************************************************
// Unused locals
//**************************************************************************************************

fn remove_unused_bindings_function(
    context: &mut Context,
    used: &BTreeSet<N::Var_>,
    f: &mut N::Function,
) {
    match &mut f.body.value {
        N::FunctionBody_::Defined(seq) => remove_unused_bindings_seq(context, used, seq),
        // no warnings for natives
        N::FunctionBody_::Native => return,
    }
    for (_, v, _) in &mut f.signature.parameters {
        if !used.contains(&v.value) {
            report_unused_local(context, v);
        }
    }
}

fn remove_unused_bindings_seq(
    context: &mut Context,
    used: &BTreeSet<N::Var_>,
    seq: &mut N::Sequence,
) {
    for sp!(_, item_) in &mut seq.1 {
        match item_ {
            N::SequenceItem_::Seq(e) => remove_unused_bindings_exp(context, used, e),
            N::SequenceItem_::Declare(lvalues, _) => {
                // unused bindings will be reported as unused assignments
                remove_unused_bindings_lvalues(
                    context, used, lvalues, /* report unused */ true,
                )
            }
            N::SequenceItem_::Bind(lvalues, e) => {
                remove_unused_bindings_lvalues(
                    context, used, lvalues, /* report unused */ false,
                );
                remove_unused_bindings_exp(context, used, e)
            }
        }
    }
}

fn remove_unused_bindings_lvalues(
    context: &mut Context,
    used: &BTreeSet<N::Var_>,
    sp!(_, lvalues): &mut N::LValueList,
    report: bool,
) {
    for lvalue in lvalues {
        remove_unused_bindings_lvalue(context, used, lvalue, report)
    }
}

fn remove_unused_bindings_lvalue(
    context: &mut Context,
    used: &BTreeSet<N::Var_>,
    sp!(_, lvalue_): &mut N::LValue,
    report: bool,
) {
    match lvalue_ {
        N::LValue_::Ignore => (),
        N::LValue_::Var {
            var,
            unused_binding,
            ..
        } if used.contains(&var.value) => {
            debug_assert!(!*unused_binding);
        }
        N::LValue_::Var {
            var,
            unused_binding,
            ..
        } => {
            debug_assert!(!*unused_binding);
            if report {
                report_unused_local(context, var);
            }
            *unused_binding = true;
        }
        N::LValue_::Unpack(_, _, _, lvalues) => {
            for (_, _, (_, lvalue)) in lvalues {
                remove_unused_bindings_lvalue(context, used, lvalue, report)
            }
        }
    }
}

fn remove_unused_bindings_exp(
    context: &mut Context,
    used: &BTreeSet<N::Var_>,
    sp!(_, e_): &mut N::Exp,
) {
    match e_ {
        N::Exp_::Value(_)
        | N::Exp_::Var(_)
        | N::Exp_::Constant(_, _)
        | N::Exp_::Continue(_)
        | N::Exp_::Unit { .. }
        | N::Exp_::UnresolvedError => (),
        N::Exp_::Return(e)
        | N::Exp_::Abort(e)
        | N::Exp_::Dereference(e)
        | N::Exp_::UnaryExp(_, e)
        | N::Exp_::Cast(e, _)
        | N::Exp_::Assign(_, e)
        | N::Exp_::Loop(_, e)
        | N::Exp_::Give(_, e)
        | N::Exp_::Annotate(e, _) => remove_unused_bindings_exp(context, used, e),
        N::Exp_::IfElse(econd, et, ef) => {
            remove_unused_bindings_exp(context, used, econd);
            remove_unused_bindings_exp(context, used, et);
            remove_unused_bindings_exp(context, used, ef);
        }
        N::Exp_::Match(esubject, arms) => {
            // TODO: account for and remove unused pattern bindings
            remove_unused_bindings_exp(context, used, esubject);
            for arm in &mut arms.value {
                if let Some(guard) = arm.value.guard.as_mut() {
                    remove_unused_bindings_exp(context, used, guard)
                }
                remove_unused_bindings_exp(context, used, &mut arm.value.rhs);
            }
        }
        N::Exp_::While(econd, _, ebody) => {
            remove_unused_bindings_exp(context, used, econd);
            remove_unused_bindings_exp(context, used, ebody)
        }
        N::Exp_::NamedBlock(_, s) => remove_unused_bindings_seq(context, used, s),
        N::Exp_::Block(s) => remove_unused_bindings_seq(context, used, s),
        N::Exp_::FieldMutate(ed, e) => {
            remove_unused_bindings_exp_dotted(context, used, ed);
            remove_unused_bindings_exp(context, used, e)
        }
        N::Exp_::Mutate(el, er) | N::Exp_::BinopExp(el, _, er) => {
            remove_unused_bindings_exp(context, used, el);
            remove_unused_bindings_exp(context, used, er)
        }
        N::Exp_::Pack(_, _, _, fields) => {
            for (_, _, (_, e)) in fields {
                remove_unused_bindings_exp(context, used, e)
            }
        }
        N::Exp_::PackVariant(_, _, _, _, fields) => {
            for (_, _, (_, e)) in fields {
                remove_unused_bindings_exp(context, used, e)
            }
        }

        N::Exp_::Builtin(_, sp!(_, es))
        | N::Exp_::Vector(_, _, sp!(_, es))
        | N::Exp_::ModuleCall(_, _, _, sp!(_, es))
        | N::Exp_::ExpList(es) => {
            for e in es {
                remove_unused_bindings_exp(context, used, e)
            }
        }
        N::Exp_::MethodCall(ed, _, _, sp!(_, es)) => {
            remove_unused_bindings_exp_dotted(context, used, ed);
            for e in es {
                remove_unused_bindings_exp(context, used, e)
            }
        }

        N::Exp_::ExpDotted(_, ed) => remove_unused_bindings_exp_dotted(context, used, ed),
    }
}

fn remove_unused_bindings_exp_dotted(
    context: &mut Context,
    used: &BTreeSet<N::Var_>,
    sp!(_, ed_): &mut N::ExpDotted,
) {
    match ed_ {
        N::ExpDotted_::Exp(e) => remove_unused_bindings_exp(context, used, e),
        N::ExpDotted_::Dot(ed, _) => remove_unused_bindings_exp_dotted(context, used, ed),
    }
}

fn report_unused_local(context: &mut Context, sp!(loc, unused_): &N::Var) {
    if !unused_.name.starts_with(|c: char| c.is_ascii_lowercase()) {
        return;
    }
    let N::Var_ { name, id, color } = unused_;
    debug_assert!(*color == 0);
    let is_parameter = *id == 0;
    let kind = if is_parameter {
        "parameter"
    } else {
        "local variable"
    };
    let msg = format!(
        "Unused {kind} '{name}'. Consider removing or prefixing with an underscore: '_{name}'",
    );
    context
        .env
        .add_diag(diag!(UnusedItem::Variable, (*loc, msg)));
}
