// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    debug_display, diag,
    diagnostics::{self, codes::*, WarningFilters},
    editions::FeatureGate,
    expansion::{
        ast::{self as E, AbilitySet, Address, Ellipsis, ModuleIdent, Mutability, Visibility},
        valid_names::{is_valid_datatype_or_constant_name as is_constant_name, NameCase},
    },
    ice, ice_assert,
    naming::{
        ast::{self as N, BlockLabel, NominalBlockUsage, TParamID},
        fake_natives,
        name_resolver::{
            self, access_result as result, AccessChainResult, CoreNameResolver, NameResolver,
            ResolvedEnum, ResolvedLValueName, ResolvedMemberFunction, ResolvedPatternName,
            ResolvedStruct,
        },
        syntax_methods::resolve_syntax_attributes,
    },
    parser::ast::{
        self as P, ConstantName, DatatypeName, Field, FunctionName, VariantName, MACRO_MODIFIER,
    },
    shared::{program_info::NamingProgramInfo, unique_map::UniqueMap, *},
    FullyCompiledProgram,
};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use move_symbol_pool::Symbol;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use super::{
    ast::BuiltinFunction_,
    name_resolver::{
        FieldInfo, ModuleMembers, ResolvedCallSubject, ResolvedConstructor, ResolvedDefinition,
        ResolvedTerm, ResolvedType,
    },
};

//**************************************************************************************************
// Resolver Types
//**************************************************************************************************

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolveFunctionCase {
    UseFun,
    Call,
}

enum ModuleAccessKind {
    Function,
    Datatype,
    Constant,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
enum LoopType {
    While,
    Loop,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
enum NominalBlockType {
    Loop(LoopType),
    Block,
    LambdaReturn,
    LambdaLoopCapture,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
enum TypeAnnotation {
    StructField,
    VariantField,
    ConstantSignature,
    FunctionSignature,
    MacroSignature,
    Expression,
}

//**************************************************************************************************
// Context
//**************************************************************************************************

// The context is just a wrapper around the resolver, which is the "true" context with
// functionality attached. It carries the resolver in a dynamic box. We could consider passing that
// directly, but it seems less ideal.
pub(super) struct Context {
    pub resolver: Box<dyn NameResolver>,
}

impl Context {
    fn new(resolver: Box<dyn NameResolver>) -> Self {
        Self { resolver }
    }

    fn core(&mut self) -> &mut CoreNameResolver {
        self.resolver.get_core_resolver()
    }

    fn env(&mut self) -> &mut CompilationEnv {
        self.resolver.get_core_resolver().env
    }

    fn resolve_module(&mut self, m: &ModuleIdent) -> bool {
        let resolved = self.core().module_members.contains_key(m);
        if !resolved {
            self.env.add_diag(diag!(
                NameResolution::UnboundModule,
                (m.loc, format!("Unbound module '{}'", m))
            ))
        }
        resolved
    }

    fn check_feature(&mut self, loc: Loc, feature: FeatureGate) {
        let cur_pkg = self.resolver.core().current_package;
        self.env().check_feature(cur_pkg, feature, loc);
    }
}

fn arity_string(arity: usize) -> &'static str {
    match arity {
        0 => "",
        1 => "<T>",
        _ => "<T0,...>",
    }
}

//**************************************************************************************************
// Entry
//**************************************************************************************************

pub fn program(
    compilation_env: &mut CompilationEnv,
    pre_compiled_lib: Option<Arc<FullyCompiledProgram>>,
    prog: E::Program,
) -> N::Program {
    let (modules, member_map) = name_resolver::build_member_map(pre_compiled_lib, &prog);
    let core_resolver = CoreNameResolver::new(compilation_env, &member_map);
    let E::Program {
        modules: emodules,
        named_address_maps,
    } = prog;
    let modules = modules(&mut core_resolver, emodules, &named_address_maps);
    let mut inner = N::Program_ { modules };
    let mut info = NamingProgramInfo::new(pre_compiled_lib, &inner);
    super::resolve_use_funs::program(compilation_env, &mut info, &mut inner);
    N::Program { info, inner }
}

fn modules(
    core: &mut CoreNameResolver,
    modules: UniqueMap<ModuleIdent, E::ModuleDefinition>,
    name_address_maps: &NamedAddressMaps,
) -> UniqueMap<ModuleIdent, N::ModuleDefinition> {
    modules.map(|ident, mdef| module(core, ident, mdef, name_address_maps))
}

fn module(
    core: &mut CoreNameResolver,
    ident: ModuleIdent,
    mdef: E::ModuleDefinition,
    named_address_maps: &NamedAddressMaps,
) -> N::ModuleDefinition {
    let E::ModuleDefinition {
        loc,
        warning_filter,
        package_name,
        attributes,
        target_kind,
        // use_funs: euse_funs,
        friends: efriends,
        structs: estructs,
        enums: eenums,
        functions: efunctions,
        constants: econstants,
        uses,
        name_address_map_index,
    } = mdef;
    core.enter_package(package_name);
    core.enter_module(ident);
    core.env.add_warning_filter_scope(warning_filter.clone());

    let name_address_map = named_address_maps.get(name_address_map_index);

    let resolver = if core
        .env
        .supports_feature(package_name, FeatureGate::Move2024Paths)
    {
        name_resolver::Move2024NameResolver::new(core, name_address_map)
    } else {
        name_resolver::LegacyNameResolver::new(core, name_address_map, todo!());
    };

    let context = Context::new(Box::new(resolver) /* is_source_module */);
    // FIXME: use funs
    // let mut use_funs = use_funs(&mut context, euse_funs);
    let mut syntax_methods = N::SyntaxMethods::new();
    let friends = efriends.filter_map(|mident, f| friend(&mut context, mident, f));
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
        context.push_unscoped_types_scope();
        let s = struct_def(&mut context, name, s);
        context.pop_unscoped_types_scope();
        s
    });
    // simply for compilation to continue in the presence of errors, we remove the duplicates
    let enums = eenums.filter_map(|name, e| {
        context.push_unscoped_types_scope();
        let result = if enum_struct_intersection.contains(&name) {
            None
        } else {
            Some(enum_def(&mut context, name, e))
        };
        context.pop_unscoped_types_scope();
        result
    });
    let functions = efunctions.map(|name, f| {
        context.push_unscoped_types_scope();
        let f = function(&mut context, &mut syntax_methods, ident, name, f);
        context.pop_unscoped_types_scope();
        f
    });
    let constants = econstants.map(|name, c| {
        context.push_unscoped_types_scope();
        let c = constant(&mut context, name, c);
        context.pop_unscoped_types_scope();
        c
    });
    // Silence unused use fun warnings if a module has macros.
    // For public macros, the macro will pull in the use fun, and we will which case we will be
    //   unable to tell if it is used or not
    // For private macros, we duplicate the scope of the module and when resolving the method
    //   fail to mark the outer scope as used (instead we only mark the modules scope cloned
    //   into the macro)
    // TODO we should approximate this by just checking for the name, regardless of the type
    let has_macro = functions.iter().any(|(_, _, f)| f.macro_.is_some());
    // FIXME if has_macro {
    // FIXME     mark_all_use_funs_as_used(&mut use_funs);
    // FIXME }
    core.env.pop_warning_filter_scope();
    core.exit_module();
    core.exit_package();
    N::ModuleDefinition {
        loc,
        warning_filter,
        package_name,
        attributes,
        target_kind,
        use_funs: todo!(),
        syntax_methods,
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

// FIXME: use funs

// fn use_funs(context: &mut Context, eufs: E::UseFuns) -> N::UseFuns {
//     let E::UseFuns {
//         explicit: eexplicit,
//         implicit: eimplicit,
//     } = eufs;
//     let mut resolved = N::ResolvedUseFuns::new();
//     let resolved_vec: Vec<_> = eexplicit
//         .into_iter()
//         .flat_map(|e| explicit_use_fun(context, e))
//         .collect();
//     for (tn, method, nuf) in resolved_vec {
//         let methods = resolved.entry(tn.clone()).or_default();
//         let nuf_loc = nuf.loc;
//         if let Err((_, prev)) = methods.add(method, nuf) {
//             let msg = format!("Duplicate 'use fun' for '{}.{}'", tn, method);
//             context.env.add_diag(diag!(
//                 Declarations::DuplicateItem,
//                 (nuf_loc, msg),
//                 (prev, "Previously declared here"),
//             ))
//         }
//     }
//     N::UseFuns {
//         color: 0, // used for macro substitution
//         resolved,
//         implicit_candidates: eimplicit,
//     }
// }
//
// fn explicit_use_fun(
//     context: &mut Context,
//     e: E::ExplicitUseFun,
// ) -> Option<(N::TypeName, Name, N::UseFun)> {
//     let E::ExplicitUseFun {
//         loc,
//         attributes,
//         is_public,
//         function,
//         ty,
//         method,
//     } = e;
//     let m_f_opt = match resolve_function(context, ResolveFunctionCase::UseFun, loc, function, None)
//     {
//         ResolvedFunction::Module(mf) => {
//             let ResolvedModuleFunction {
//                 module,
//                 function,
//                 ty_args,
//             } = *mf;
//             assert!(ty_args.is_none());
//             Some((module, function))
//         }
//         ResolvedFunction::Builtin(_) => {
//             let msg = "Invalid 'use fun'. Cannot use a builtin function as a method";
//             context
//                 .env
//                 .add_diag(diag!(Declarations::InvalidUseFun, (loc, msg)));
//             None
//         }
//         ResolvedFunction::Var(_) => {
//             unreachable!("ICE this case should be excluded from ResolveFunctionCase::UseFun")
//         }
//         ResolvedFunction::Unbound => {
//             assert!(context.env.has_errors());
//             None
//         }
//     };
//     let ty_loc = ty.loc;
//     let tn_opt = match context.resolve_type(ty) {
//         ResolvedType::Unbound => {
//             assert!(context.env.has_errors());
//             None
//         }
//         ResolvedType::Hole => {
//             let msg = "Invalid 'use fun'. Cannot associate a method with an inferred type";
//             let tmsg = "The '_' type is a placeholder for type inference";
//             context.env.add_diag(diag!(
//                 Declarations::InvalidUseFun,
//                 (loc, msg),
//                 (ty_loc, tmsg)
//             ));
//             None
//         }
//         ResolvedType::TParam(tloc, tp) => {
//             let msg = "Invalid 'use fun'. Cannot associate a method with a type parameter";
//             let tmsg = format!(
//                 "But '{}' was declared as a type parameter here",
//                 tp.user_specified_name
//             );
//             context.env.add_diag(diag!(
//                 Declarations::InvalidUseFun,
//                 (loc, msg,),
//                 (tloc, tmsg)
//             ));
//             None
//         }
//         ResolvedType::BuiltinType(bt_) => Some(N::TypeName_::Builtin(sp(ty.loc, bt_))),
//         ResolvedType::ModuleType(mt) => match mt.module_type {
//             ModuleType::Struct(stype) => Some(N::TypeName_::ModuleType(
//                 stype.original_mident,
//                 DatatypeName(mt.original_type_name),
//             )),
//             ModuleType::Enum(etype) => Some(N::TypeName_::ModuleType(
//                 etype.original_mident,
//                 DatatypeName(mt.original_type_name),
//             )),
//         },
//     };
//     let tn_ = tn_opt?;
//     let tn = sp(ty.loc, tn_);
//     if let Some(pub_loc) = is_public {
//         let current_module = context.current_module;
//         if let Err(def_loc_opt) = use_fun_module_defines(context, current_module, &tn) {
//             let msg = "Invalid 'use fun'. Cannot publicly associate a function with a \
//                 type defined in another module";
//             let pub_msg = format!(
//                 "Declared '{}' here. Consider removing to make a local 'use fun' instead",
//                 Visibility::PUBLIC
//             );
//             let mut diag = diag!(Declarations::InvalidUseFun, (loc, msg), (pub_loc, pub_msg));
//             if let Some(def_loc) = def_loc_opt {
//                 diag.add_secondary_label((def_loc, "Type defined in another module here"));
//             }
//             context.env.add_diag(diag);
//             return None;
//         }
//     }
//     let target_function = m_f_opt?;
//     let use_fun = N::UseFun {
//         loc,
//         attributes,
//         is_public,
//         tname: tn.clone(),
//         target_function,
//         kind: N::UseFunKind::Explicit,
//         used: is_public.is_some(), // suppress unused warning for public use funs
//     };
//     Some((tn, method, use_fun))
// }
//
// fn use_fun_module_defines(
//     context: &mut Context,
//     specified: Option<ModuleIdent>,
//     tn: &N::TypeName,
// ) -> Result<(), Option<Loc>> {
//     match &tn.value {
//         N::TypeName_::Builtin(sp!(_, b_)) => {
//             let definer_opt = context.env.primitive_definer(*b_);
//             match (definer_opt, &specified) {
//                 (None, _) => Err(None),
//                 (Some(d), None) => Err(Some(d.loc)),
//                 (Some(d), Some(s)) => {
//                     if d == s {
//                         Ok(())
//                     } else {
//                         Err(Some(d.loc))
//                     }
//                 }
//             }
//         }
//         N::TypeName_::ModuleType(m, n) => {
//             if specified.as_ref().is_some_and(|n| n == m) {
//                 Ok(())
//             } else {
//                 let mod_type = context
//                     .scoped_types
//                     .get(m)
//                     .unwrap()
//                     .get(&n.value())
//                     .unwrap();
//                 Err(Some(mod_type.decl_loc()))
//             }
//         }
//         ty @ N::TypeName_::Multiple(_) => {
//             let msg = format!(
//                 "ICE tuple type {} should not be reachable from use fun",
//                 debug_display!(ty)
//             );
//             context.env.add_diag(ice!((tn.loc, msg)));
//             // This is already reporting a bug, so let's continue for lack of something better to do.
//             Ok(())
//         }
//     }
// }
//
// fn mark_all_use_funs_as_used(use_funs: &mut N::UseFuns) {
//     let N::UseFuns {
//         color: _,
//         resolved,
//         implicit_candidates,
//     } = use_funs;
//     for methods in resolved.values_mut() {
//         for (_, _, uf) in methods {
//             uf.used = true;
//         }
//     }
//     for (_, _, uf) in implicit_candidates {
//         match &mut uf.kind {
//             E::ImplicitUseFunKind::UseAlias { used } => *used = true,
//             E::ImplicitUseFunKind::FunctionDeclaration => (),
//         }
//     }
// }

//**************************************************************************************************
// Friends
//**************************************************************************************************

fn friend(context: &mut Context, mident: ModuleIdent, friend: E::Friend) -> Option<E::Friend> {
    let current_mident = context.resolver.get_core_resolver(); // .current_module.as_ref().unwrap();
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
    syntax_methods: &mut N::SyntaxMethods,
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
        macro_,
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
    let case = if macro_.is_some() {
        TypeAnnotation::MacroSignature
    } else {
        TypeAnnotation::FunctionSignature
    };
    let signature = function_signature(context, case, signature);
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
        macro_,
        entry,
        signature,
        body,
    };
    resolve_syntax_attributes(context, syntax_methods, &module, &name, &f);
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

fn function_signature(
    context: &mut Context,
    case: TypeAnnotation,
    sig: E::FunctionSignature,
) -> N::FunctionSignature {
    let type_parameters = fun_type_parameters(context, sig.type_parameters);

    let mut declared = UniqueMap::new();
    let parameters = sig
        .parameters
        .into_iter()
        .map(|(mut mut_, param, param_ty)| {
            let is_underscore = param.is_underscore();
            if is_underscore {
                check_mut_underscore(context, Some(mut_));
                mut_ = Mutability::Imm;
            };
            if param.is_syntax_identifier() {
                if let Mutability::Mut(mutloc) = mut_ {
                    let msg = format!(
                        "Invalid 'mut' parameter. \
                        '{}' parameters cannot be declared as mutable",
                        MACRO_MODIFIER
                    );
                    let mut diag = diag!(NameResolution::InvalidMacroParameter, (mutloc, msg));
                    diag.add_note(ASSIGN_SYNTAX_IDENTIFIER_NOTE);
                    context.env.add_diag(diag);
                    mut_ = Mutability::Imm;
                }
            }
            if let Err((param, prev_loc)) = declared.add(param, ()) {
                if !is_underscore {
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
            let nparam_ty = type_(context, case, param_ty);
            (mut_, nparam, nparam_ty)
        })
        .collect();
    let return_type = type_(context, case, sig.return_type);
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

const ASSIGN_SYNTAX_IDENTIFIER_NOTE: &str = "'macro' parameters are substituted without \
    being evaluated. There is no local variable to assign to";

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
        E::StructFields::Named(em) => N::StructFields::Defined(
            false,
            em.map(|_f, (idx, t)| (idx, type_(context, TypeAnnotation::StructField, t))),
        ),
        E::StructFields::Positional(tys) => {
            let fields = tys
                .into_iter()
                .map(|ty| type_(context, TypeAnnotation::StructField, ty))
                .enumerate()
                .map(|(idx, ty)| {
                    let field_name = positional_field_name(ty.loc, idx);
                    (field_name, (idx, ty))
                });
            N::StructFields::Defined(true, UniqueMap::maybe_from_iter(fields).unwrap())
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
        E::VariantFields::Named(em) => N::VariantFields::Defined(
            false,
            em.map(|_f, (idx, t)| (idx, type_(context, TypeAnnotation::VariantField, t))),
        ),
        E::VariantFields::Positional(tys) => {
            let fields = tys
                .into_iter()
                .map(|ty| type_(context, TypeAnnotation::VariantField, ty))
                .enumerate()
                .map(|(idx, ty)| {
                    let field_name = positional_field_name(ty.loc, idx);
                    (field_name, (idx, ty))
                });
            N::VariantFields::Defined(true, UniqueMap::maybe_from_iter(fields).unwrap())
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
    let signature = type_(context, TypeAnnotation::ConstantSignature, esignature);
    let value = *exp(context, Box::new(evalue));
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

fn opt_types_with_arity_check<F: FnOnce() -> String>(
    context: &mut Context,
    case: TypeAnnotation,
    loc: Loc,
    name_f: F,
    ty_args: Option<Vec<E::Type>>,
    arity: usize,
) -> Option<Vec<N::Type>> {
    ty_args.map(|etys| {
        let tys = types(context, case, etys);
        check_type_argument_arity(context, loc, name_f, tys, arity)
    })
}

fn types(context: &mut Context, case: TypeAnnotation, tys: Vec<E::Type>) -> Vec<N::Type> {
    tys.into_iter().map(|t| type_(context, case, t)).collect()
}

fn type_(context: &mut Context, case: TypeAnnotation, sp!(loc, ety_): E::Type) -> N::Type {
    use ResolvedType as RT;
    use E::Type_ as ET;
    use N::{TypeName_ as NN, Type_ as NT};
    let ty_ = match ety_ {
        ET::Unit => NT::Unit,
        ET::Multiple(tys) => NT::multiple_(
            loc,
            tys.into_iter().map(|t| type_(context, case, t)).collect(),
        ),
        ET::Ref(mut_, inner) => NT::Ref(mut_, Box::new(type_(context, case, *inner))),
        ET::UnresolvedError => {
            assert!(context.env.has_errors());
            NT::UnresolvedError
        }
        ET::Apply(nac) => {
            let original_loc = nac.loc;
            let Some(type_access) = context.resolver.resolve_type(nac) else {
                assert!(context.env.has_errors());
                NT::UnresolvedError
            };
            let AccessChainResult {
                result: type_,
                ptys_opt,
                is_macro,
            } = type_access;

            let tyargs = ptys_opt.map(|value| value.value).unwrap_or_else(|| vec![]);

            if let Some(loc) = is_macro {
                context.env().add_diag(ice!((loc, "Macro on type")));
            }

            match type_ {
                RT::ModuleType(module_type) => {
                    let (tn, arity) = match module_type {
                        name_resolver::ResolvedDatatype::Struct(struct_) => {
                            let tn = NN::ModuleType(struct_.module, struct_.name);
                            (sp(original_loc, tn), struct_.tyarg_arity)
                        }
                        name_resolver::ResolvedDatatype::Enum(enum_) => {
                            let tn = NN::ModuleType(enum_.module, enum_.name);
                            (sp(original_loc, tn), enum_.tyarg_arity)
                        }
                    };
                    let tys = types(context, case, tyargs);
                    let name_f = || format!("{}", tn);
                    let tys = check_type_argument_arity(context, loc, name_f, tys, arity);
                    NT::Apply(None, tn, tys)
                }
                RT::TParam(_, tp) => {
                    if !tyargs.is_empty() {
                        context.env.add_diag(diag!(
                            NameResolution::NamePositionMismatch,
                            (loc, "Generic type parameters cannot take type arguments"),
                        ));
                        NT::UnresolvedError
                    } else {
                        // FIXME track tyargs
                        // if context.translating_fun {
                        //     context.used_fun_tparams.insert(tp.id);
                        // }
                        NT::Param(tp)
                    }
                }
                RT::BuiltinType(bn_) => {
                    let name_f = || format!("{}", &bn_);
                    let arity = bn_.tparam_constraints(loc).len();
                    let tys = types(context, case, tyargs);
                    let tys = check_type_argument_arity(context, loc, name_f, tys, arity);
                    NT::builtin_(sp(original_loc, bn_), tys)
                }
                RT::Hole => {
                    let case_str_opt = match case {
                        TypeAnnotation::StructField => {
                            Some(("Struct fields", " or consider adding a new type parameter"))
                        }
                        TypeAnnotation::VariantField => Some((
                            "Enum variant fields",
                            " or consider adding a new type parameter",
                        )),
                        TypeAnnotation::ConstantSignature => Some(("Constants", "")),
                        TypeAnnotation::FunctionSignature => {
                            Some(("Functions", " or consider adding a new type parameter"))
                        }
                        TypeAnnotation::MacroSignature | TypeAnnotation::Expression => None,
                    };
                    if let Some((case_str, help_str)) = case_str_opt {
                        let msg = format!(
                              "Invalid usage of a placeholder for type inference '_'. \
                              {case_str} require fully specified types. Replace '_' with a specific type{help_str}"
                          );
                        let mut diag = diag!(NameResolution::InvalidTypeAnnotation, (loc, msg));
                        if let TypeAnnotation::FunctionSignature = case {
                            diag.add_note("Only 'macro' functions can use '_' in their signatures");
                        }
                        context.env.add_diag(diag);
                        NT::UnresolvedError
                    } else {
                        // replaced with a type variable during type instantiation
                        NT::Anything
                    }
                }
            }
        }
        ET::Fun(tys, ty) => {
            let tys = types(context, case, tys);
            let ty = Box::new(type_(context, case, *ty));
            NT::Fun(tys, ty)
        }
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

#[growing_stack]
fn sequence(context: &mut Context, (euse_funs, seq): E::Sequence) -> N::Sequence {
    context.new_local_scope();
    let nuse_funs = use_funs(context, euse_funs);
    let nseq = seq.into_iter().map(|s| sequence_item(context, s)).collect();
    context.close_local_scope();
    (nuse_funs, nseq)
}

#[growing_stack]
fn sequence_item(context: &mut Context, sp!(loc, ns_): E::SequenceItem) -> N::SequenceItem {
    use E::SequenceItem_ as ES;
    use N::SequenceItem_ as NS;

    let s_ = match ns_ {
        ES::Seq(e) => NS::Seq(exp(context, e)),
        ES::Declare(b, ty_opt) => {
            let bind_opt = bind_list(context, b);
            let tys = ty_opt.map(|t| type_(context, TypeAnnotation::Expression, t));
            match bind_opt {
                None => {
                    assert!(context.env.has_errors());
                    NS::Seq(Box::new(sp(loc, N::Exp_::UnresolvedError)))
                }
                Some(bind) => NS::Declare(bind, tys),
            }
        }
        ES::Bind(b, e) => {
            let e = exp(context, e);
            let bind_opt = bind_list(context, b);
            match bind_opt {
                None => {
                    assert!(context.env.has_errors());
                    NS::Seq(Box::new(sp(loc, N::Exp_::UnresolvedError)))
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
    es.into_iter().map(|e| *exp(context, Box::new(e))).collect()
}

#[growing_stack]
fn exp(context: &mut Context, e: Box<E::Exp>) -> Box<N::Exp> {
    use E::Exp_ as EE;
    use N::Exp_ as NE;
    let sp!(eloc, e_) = *e;
    let ne_ = match e_ {
        EE::Unit { trailing } => NE::Unit { trailing },
        EE::Value(val) => NE::Value(val),
        EE::Name(nac) => {
            let original_loc = nac.loc;
            let Some(result!(term, ptys_opt, is_macro)) = context.resolver.resolve_term(nac) else {
                debug_assert!(context.env.has_errors());
                NE::UnresolvedError
            };
            match term {
                ResolvedTerm::Variable(x) => {
                    report_invalid_macro(context, is_macro, "Variables");
                    if let Some(tyargs) = ptys_opt {
                        context.env().add_diag(diag!(
                            NameResolution::TooManyTypeArguments,
                            (tyargs.loc, "Variables cannot take type arguments")
                        ));
                    };
                    NE::Var(x)
                }
                ResolvedTerm::Constant(c) => {
                    report_invalid_macro(context, is_macro, "Constants");
                    if let Some(tyargs) = ptys_opt {
                        context.env().add_diag(diag!(
                            NameResolution::TooManyTypeArguments,
                            (tyargs.loc, "Constants cannot take type arguments")
                        ));
                    };
                    N::Exp_::Constant(c.module, c.name)
                }
                ResolvedTerm::Variant(variant) => {
                    report_invalid_macro(context, is_macro, "Datatypes");
                    let current_package = context.core().current_package;
                    context.check_feature(FeatureGate::Enums, eloc);
                    let tys_opt = opt_types_with_arity_check(
                        context,
                        TypeAnnotation::Expression,
                        eloc,
                        || format!("{}::{}", &variant.module, &variant.enum_name),
                        ptys_opt,
                        variant.tyarg_arity,
                    );
                    check_constructor_form(
                        context,
                        eloc,
                        ConstructorForm::None,
                        "instantiation",
                        &ResolvedConstructor::Variant(variant),
                    );
                    NE::PackVariant(
                        variant.module,
                        variant.enum_name,
                        variant.name,
                        tys_opt,
                        UniqueMap::new(),
                    )
                }
            }
        }
        EE::IfElse(eb, et, ef) => NE::IfElse(exp(context, eb), exp(context, et), exp(context, ef)),
        EE::Match(esubject, sp!(_aloc, arms)) if arms.is_empty() => {
            exp(context, esubject); // for error effect
            let msg = "Invalid 'match' form. 'match' must have at least one arm";
            context
                .env
                .add_diag(diag!(Syntax::InvalidMatch, (eloc, msg)));
            NE::UnresolvedError
        }
        EE::Match(esubject, sp!(aloc, arms)) => NE::Match(
            exp(context, esubject),
            sp(
                aloc,
                arms.into_iter()
                    .map(|arm| match_arm(context, arm))
                    .collect(),
            ),
        ),
        EE::While(name_opt, eb, el) => {
            let cond = exp(context, eb);
            context.resolver.enter_nominal_block(
                eloc,
                name_opt,
                NominalBlockType::Loop(LoopType::While),
            );
            let body = exp(context, el);
            let (label, name_type) = context.exit_nominal_block();
            assert_eq!(name_type, NominalBlockType::Loop(LoopType::While));
            NE::While(label, cond, body)
        }
        EE::Loop(name_opt, el) => {
            context.resolver.enter_nominal_block(
                eloc,
                name_opt,
                NominalBlockType::Loop(LoopType::Loop),
            );
            let body = exp(context, el);
            let (label, name_type) = context.exit_nominal_block();
            assert_eq!(name_type, NominalBlockType::Loop(LoopType::Loop));
            NE::Loop(label, body)
        }
        EE::Block(Some(name), eseq) => {
            context.enter_nominal_block(eloc, Some(name), NominalBlockType::Block);
            let seq = sequence(context, eseq);
            let (label, name_type) = context.exit_nominal_block();
            assert_eq!(name_type, NominalBlockType::Block);
            NE::Block(N::Block {
                name: Some(label),
                from_macro_argument: None,
                seq,
            })
        }
        EE::Block(None, eseq) => NE::Block(N::Block {
            name: None,
            from_macro_argument: None,
            seq: sequence(context, eseq),
        }),
        EE::Lambda(elambda_binds, ety_opt, body) => {
            context.resolver.new_local_scope();
            let nlambda_binds_opt = lambda_bind_list(context, elambda_binds);
            let return_type = ety_opt.map(|t| type_(context, TypeAnnotation::Expression, t));
            context
                .resolver
                .enter_nominal_block(eloc, None, NominalBlockType::LambdaLoopCapture);
            context
                .resolver
                .enter_nominal_block(eloc, None, NominalBlockType::LambdaReturn);
            let body = exp(context, body);
            context.resolver.close_local_scope();
            let (return_label, return_name_type) = context.resolver.exit_nominal_block();
            assert_eq!(return_name_type, NominalBlockType::LambdaReturn);
            let (_, loop_name_type) = context.resolver.exit_nominal_block();
            assert_eq!(loop_name_type, NominalBlockType::LambdaLoopCapture);
            match nlambda_binds_opt {
                None => {
                    assert!(context.env().has_errors());
                    N::Exp_::UnresolvedError
                }
                Some(parameters) => NE::Lambda(N::Lambda {
                    parameters,
                    return_type,
                    return_label,
                    use_fun_color: 0, // used in macro expansion
                    body,
                }),
            }
        }

        EE::Assign(a, e) => {
            let na_opt = assign_list(context, a);
            let ne = exp(context, e);
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
            let ner = exp(context, er);
            match ndot_opt {
                None => {
                    assert!(context.env.has_errors());
                    NE::UnresolvedError
                }
                Some(ndot) => NE::FieldMutate(ndot, ner),
            }
        }
        EE::Mutate(el, er) => {
            let nel = exp(context, el);
            let ner = exp(context, er);
            NE::Mutate(nel, ner)
        }

        EE::Abort(es) => NE::Abort(exp(context, es)),
        EE::Return(Some(block_name), es) => {
            let out_rhs = exp(context, es);
            context
                .resolver
                .resolve_nominal_label(NominalBlockUsage::Return, block_name)
                .map(|name| NE::Give(NominalBlockUsage::Return, name, out_rhs))
                .unwrap_or_else(|| NE::UnresolvedError)
        }
        EE::Return(None, es) => {
            let out_rhs = exp(context, es);
            if let Some(return_name) = context.resolver.current_return(eloc) {
                NE::Give(NominalBlockUsage::Return, return_name, out_rhs)
            } else {
                NE::Return(out_rhs)
            }
        }
        EE::Break(name_opt, rhs) => {
            let out_rhs = exp(context, rhs);
            if let Some(loop_name) = name_opt {
                context
                    .resolver
                    .resolve_nominal_label(NominalBlockUsage::Break, loop_name)
                    .map(|name| NE::Give(NominalBlockUsage::Break, name, out_rhs))
                    .unwrap_or_else(|| NE::UnresolvedError)
            } else {
                context
                    .resolver
                    .current_break(eloc)
                    .map(|name| NE::Give(NominalBlockUsage::Break, name, out_rhs))
                    .unwrap_or_else(|| NE::UnresolvedError)
            }
        }
        EE::Continue(name_opt) => {
            if let Some(loop_name) = name_opt {
                context
                    .resolver
                    .resolve_nominal_label(NominalBlockUsage::Continue, loop_name)
                    .map(NE::Continue)
                    .unwrap_or_else(|| NE::UnresolvedError)
            } else {
                context
                    .resolver
                    .current_continue(eloc)
                    .map(NE::Continue)
                    .unwrap_or_else(|| NE::UnresolvedError)
            }
        }

        EE::Dereference(e) => NE::Dereference(exp(context, e)),
        EE::UnaryExp(uop, e) => NE::UnaryExp(uop, exp(context, e)),

        e_ @ EE::BinopExp(..) => {
            process_binops!(
                (P::BinOp, Loc),
                Box<N::Exp>,
                Box::new(sp(eloc, e_)),
                e,
                *e,
                sp!(loc, EE::BinopExp(lhs, op, rhs)) => { (lhs, (op, loc), rhs) },
                { exp(context, e) },
                value_stack,
                (bop, loc) => {
                    let el = value_stack.pop().expect("ICE binop naming issue");
                    let er = value_stack.pop().expect("ICE binop naming issue");
                    Box::new(sp(loc, NE::BinopExp(el, bop, er)))
                }
            )
            .value
        }

        EE::Pack(nac, efields) => {
            // Process fields for errors either way.
            let fields = efields.map(|_, (idx, e)| (idx, *exp(context, Box::new(e))));
            let Some(result!(ctor, ptys_opt, is_macro)) = context.resolver.resolve_constructor(nac)
            else {
                debug_assert!(context.env.has_errors());
                NE::UnresolvedError
            };
            report_invalid_macro(context, is_macro, "Datatypes");
            let tys_opt = opt_types_with_arity_check(
                context,
                TypeAnnotation::Expression,
                eloc,
                || format!("{}::{}", &ctor.module(), &ctor.type_name()),
                ptys_opt,
                ctor.type_arity(),
            );
            check_constructor_form(
                context,
                eloc,
                ConstructorForm::Braces,
                "instantiation",
                &ctor,
            );
            // TODO: We could check field exhaustiveness ahead of typing.
            match ctor {
                ResolvedConstructor::Struct(s) => NE::Pack(s.module, s.name, tys_opt, fields),
                ResolvedConstructor::Variant(v) => {
                    NE::PackVariant(v.module, v.enum_name, v.name, tys_opt, fields)
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
            Some(ndot) => NE::ExpDotted(case, ndot),
        },

        EE::Cast(e, t) => NE::Cast(
            exp(context, e),
            type_(context, TypeAnnotation::Expression, t),
        ),
        EE::Annotate(e, t) => NE::Annotate(
            exp(context, e),
            type_(context, TypeAnnotation::Expression, t),
        ),

        EE::Call(nac, args) => {
            let nloc = nac.loc;
            let nes = call_args(context, args);
            let Some(result!(operator, ptys_opt, is_macro)) =
                context.resolver.resolve_call_subject(nac)
            else {
                debug_assert!(context.env.has_errors());
                NE::UnresolvedError
            };
            resolved_call(context, eloc, operator, nes, ptys_opt, is_macro)
        }
        EE::MethodCall(edot, n, is_macro, tys_opt, rhs) => match dotted(context, *edot) {
            None => {
                assert!(context.env.has_errors());
                NE::UnresolvedError
            }
            Some(d) => {
                let ty_args = tys_opt.map(|tys| types(context, TypeAnnotation::Expression, tys));
                let nes = call_args(context, rhs);
                if is_macro.is_some() {
                    context.env.check_feature(
                        context.current_package,
                        FeatureGate::MacroFuns,
                        eloc,
                    );
                }
                NE::MethodCall(d, n, is_macro, ty_args, nes)
            }
        },
        EE::Vector(vec_loc, tys_opt, rhs) => {
            let ty_args = tys_opt.map(|tys| types(context, TypeAnnotation::Expression, tys));
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
        e @ (EE::Index(..) | EE::Quant(..)) => {
            let mut diag = ice!((
                eloc,
                "ICE compiler should not have parsed this form as a specification"
            ));
            diag.add_note(format!("Compiler parsed: {}", debug_display!(e)));
            context.env.add_diag(diag);
            NE::UnresolvedError
        }
    };
    Box::new(sp(eloc, ne_))
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

/// Resolve a call
fn resolved_call(
    context: &mut Context,
    eloc: Loc,
    subject: ResolvedCallSubject,
    args: Spanned<Vec<N::Exp>>,
    ptys_opt: Option<Spanned<Vec<P::Type>>>,
    is_macro: Option<Loc>,
) -> N::Exp_ {
    use N::Exp_ as NE;
    match subject {
        ResolvedCallSubject::Struct(ctor) => {
            let ctor = ResolvedConstructor::Struct(ctor);
            call_ctor(context, eloc, ctor, args, ptys_opt, is_macro)
        }
        ResolvedCallSubject::Variant(ctor) => {
            let ctor = ResolvedConstructor::Variant(ctor);
            call_ctor(context, eloc, ctor, args, ptys_opt, is_macro)
        }
        ResolvedCallSubject::Builtin(bt) => {
            call_builtin(context, eloc, bt, args, ptys_opt, is_macro)
        }
        ResolvedCallSubject::MemberFunction(fun) => {
            if let Some(mloc) = is_macro {
                context.check_feature(FeatureGate::MacroFuns, mloc);
            }
            let ResolvedMemberFunction {
                module,
                name,
                tyarg_arity,
                arity,
            } = fun;
            let tys_opt = opt_types_with_arity_check(
                context,
                TypeAnnotation::Expression,
                eloc,
                || format!("assert"),
                ptys_opt,
                tyarg_arity,
            );
            // TODO: we could check and enforce arity here
            NE::ModuleCall(module, name, is_macro, tys_opt, args)
        }
        ResolvedCallSubject::Variable(x) => {
            if let Some(mloc) = is_macro {
                let msg = "Unexpected macro invocation. Bound lambdas cannot be invoked as \
                    a macro";
                context
                    .env
                    .add_diag(diag!(TypeSafety::InvalidCallTarget, (mloc, msg)));
            }
            NE::VarCall(x, args)
        }
    }
}

/// Handles a constructor (struct or variant) called positionally.
fn call_ctor(
    context: &mut Context,
    eloc: Loc,
    constructor: ResolvedConstructor,
    args: Spanned<Vec<N::Exp>>,
    ptys_opt: Option<Spanned<Vec<P::Type>>>,
    is_macro: Option<Loc>,
) -> N::Exp_ {
    context
        .env
        .check_feature(context.current_package, FeatureGate::PositionalFields, eloc);
    report_invalid_macro(context, is_macro, "Datatypes");
    let tys_opt = ptys_opt.map(|etys| {
        let tys = types(context, TypeAnnotation::Expression, etys);
        let name_f = || format!("{}::{}", &constructor.module(), &constructor.type_name());
        check_type_argument_arity(context, eloc, name_f, tys, constructor.type_arity())
    });
    check_constructor_form(
        context,
        eloc,
        ConstructorForm::Parens,
        "instantiation",
        &constructor,
    );
    let fields = UniqueMap::maybe_from_iter(args.value.into_iter().enumerate().map(|(idx, e)| {
        let field = Field::add_loc(e.loc, format!("{idx}").into());
        (field, (idx, e))
    }))
    .unwrap();
    match constructor {
        ResolvedConstructor::Struct(s) => N::Exp_::Pack(s.module, s.name, tys_opt, fields),
        ResolvedConstructor::Variant(v) => {
            N::Exp_::PackVariant(v.module, v.enum_name, v.name, tys_opt, fields)
        }
    }
}

/// Handles a call to a builtin function.
fn call_builtin(
    context: &mut Context,
    eloc: Loc,
    builtin: BuiltinFunction_,
    args: Spanned<Vec<N::Exp>>,
    ptys_opt: Option<Spanned<Vec<P::Type>>>,
    is_macro: Option<Loc>,
) -> N::Exp_ {
    use N::BuiltinFunction_ as BF;
    use N::Exp_ as NE;
    match builtin {
        sp!(bloc, BF::Assert(_)) => {
            let mut args = args;
            if is_macro.is_none() {
                let dep_msg = format!(
                    "'{}' function syntax has been deprecated and will be removed",
                    BF::ASSERT_MACRO
                );
                // TODO make this a tip/hint?
                let help_msg = format!(
                    "Replace with '{0}!'. '{0}' has been replaced with a '{0}!' built-in \
                            macro so that arguments are no longer eagerly evaluated",
                    BF::ASSERT_MACRO
                );
                context.env.add_diag(diag!(
                    Uncategorized::DeprecatedWillBeRemoved,
                    (bloc, dep_msg),
                    (bloc, help_msg),
                ));
            }
            let tys_opt = opt_types_with_arity_check(
                context,
                TypeAnnotation::Expression,
                eloc,
                || format!("assert"),
                ptys_opt,
                0,
            );
            // If no abort code is given for the assert, we add in the abort code as the
            // bitset-line-number if `CleverAssertions` is set.
            if args.value.len() == 1 && is_macro.is_some() {
                context
                    .env
                    .check_feature(FeatureGate::CleverAssertions, bloc);
                args.value.push(sp(
                    bloc,
                    NE::ErrorConstant {
                        line_number_loc: bloc,
                    },
                ));
            }
            NE::Builtin(sp(bloc, BF::Assert(is_macro)), args)
        }
        sp!(bloc, BF::Freeze(_)) => {
            if let Some(mloc) = is_macro {
                let msg = format!(
                    "Unexpected macro invocation. '{}' cannot be invoked as a macro",
                    BF::FREEZE,
                );
                context
                    .env
                    .add_diag(diag!(TypeSafety::InvalidCallTarget, (mloc, msg)));
            }
            let tys_opt = opt_types_with_arity_check(
                context,
                TypeAnnotation::Expression,
                eloc,
                || format!("freeze"),
                ptys_opt,
                1,
            );
            let ty = match tys_opt {
                None => None,

                _ => unreachable!(),
            };
            NE::Builtin(sp(bloc, BF::Freeze(ty)), args)
        }
    }
}

fn dotted(context: &mut Context, edot: E::ExpDotted) -> Option<N::ExpDotted> {
    let sp!(loc, edot_) = edot;
    let nedot_ = match edot_ {
        E::ExpDotted_::Exp(e) => {
            let ne = exp(context, e);
            match &ne.value {
                N::Exp_::UnresolvedError => return None,
                N::Exp_::Var(n) if n.value.is_syntax_identifier() => {
                    let mut diag = diag!(
                        NameResolution::NamePositionMismatch,
                        (n.loc, "Macro parameters are not allowed to appear in paths")
                    );
                    diag.add_note(format!(
                        "To use a macro parameter as a value in a path expression, first bind \
                            it to a local variable, e.g. 'let {0} = ${0};'",
                        &n.value.name.to_string()[1..]
                    ));
                    diag.add_note(
                        "Macro parameters are always treated as value expressions, and are not \
                        modified by path operations.\n\
                        Path operations include 'move', 'copy', '&', '&mut', and field references",
                    );
                    context.env.add_diag(diag);
                    N::ExpDotted_::Exp(Box::new(sp(ne.loc, N::Exp_::UnresolvedError)))
                }
                _ => N::ExpDotted_::Exp(ne),
            }
        }
        E::ExpDotted_::Dot(d, f) => N::ExpDotted_::Dot(Box::new(dotted(context, *d)?), Field(f)),
        E::ExpDotted_::DotUnresolved(loc, d) => {
            N::ExpDotted_::DotUnresolved(loc, Box::new(dotted(context, *d)?))
        }
        E::ExpDotted_::Index(inner, args) => {
            let args = call_args(context, args);
            let inner = Box::new(dotted(context, *inner)?);
            N::ExpDotted_::Index(inner, args)
        }
    };
    Some(sp(loc, nedot_))
}

enum ConstructorForm {
    None,
    Parens,
    Braces,
}

fn check_constructor_form(
    context: &mut Context,
    loc: Loc,
    form: ConstructorForm,
    position: &str,
    ty: &ResolvedConstructor,
) {
    use ConstructorForm as CF;
    use ResolvedConstructor as RC;
    const NAMED_UPCASE: &str = "Named";
    const NAMED: &str = "named";
    const EMPTY_UPCASE: &str = "Empty";
    const EMPTY: &str = "empty";
    const POSNL_UPCASE: &str = "Positional";
    const POSNL: &str = "positional";

    fn defn_loc_error(name: &str) -> String {
        format!("'{name}' is declared here")
    }

    macro_rules! invalid_inst_msg {
        ($ty:expr, $upcase:ident, $kind:ident) => {{
            let ty = $ty;
            let upcase = $upcase;
            let kind = $kind;
            format!(
                "Invalid {ty} {position}. \
                {upcase} {ty} declarations require {kind} {position}s"
            )
        }};
    }
    macro_rules! posnl_note {
        () => {
            format!("{POSNL_UPCASE} {position}s take arguments using '()'")
        };
    }
    macro_rules! named_note {
        () => {
            format!("{NAMED_UPCASE} {position}s take arguments using '{{ }}'")
        };
    }

    let name = ty.name();
    match ty {
        RC::Struct(stype) => match form {
            CF::None => {
                let (form_upcase, form) = if stype.field_info.is_positional() {
                    (POSNL_UPCASE, POSNL)
                } else {
                    (NAMED_UPCASE, NAMED)
                };
                let msg = invalid_inst_msg!("struct", form_upcase, form);
                let mut diag = diag!(
                    NameResolution::PositionalCallMismatch,
                    (loc, msg),
                    (stype.decl_loc, defn_loc_error(&name)),
                );
                if stype.field_info.is_positional() {
                    diag.add_note(posnl_note!());
                } else {
                    diag.add_note(named_note!());
                }
                context.env.add_diag(diag);
            }
            CF::Parens if stype.field_info.is_positional() => (),
            CF::Parens => {
                let msg = invalid_inst_msg!("struct", NAMED_UPCASE, NAMED);
                let diag = diag!(
                    NameResolution::PositionalCallMismatch,
                    (loc, &msg),
                    (stype.decl_loc, defn_loc_error(&name)),
                );
                context.env.add_diag(diag);
            }
            CF::Braces if stype.field_info.is_positional() => {
                let msg = invalid_inst_msg!("struct", POSNL_UPCASE, POSNL);
                let diag = diag!(
                    NameResolution::PositionalCallMismatch,
                    (loc, &msg),
                    (stype.decl_loc, defn_loc_error(&name)),
                );
                context.env.add_diag(diag);
            }
            CF::Braces => (),
        },
        RC::Variant(variant) => {
            let vloc = variant.decl_loc;
            let vfields = variant.field_info;
            match form {
                CF::None if vfields.is_empty() => (),
                CF::None => {
                    let (form_upcase, form) = if vfields.is_positional() {
                        (POSNL_UPCASE, POSNL)
                    } else {
                        (NAMED_UPCASE, NAMED)
                    };
                    let msg = invalid_inst_msg!("variant", form_upcase, form);
                    let mut diag = diag!(
                        NameResolution::PositionalCallMismatch,
                        (loc, &msg),
                        (*vloc, defn_loc_error(&name)),
                    );
                    if vfields.is_positional() {
                        diag.add_note(posnl_note!());
                    } else {
                        diag.add_note(named_note!());
                    }
                    context.env.add_diag(diag);
                }
                CF::Parens if vfields.is_empty() => {
                    let msg = invalid_inst_msg!("variant", EMPTY_UPCASE, EMPTY);
                    let mut diag = diag!(
                        NameResolution::PositionalCallMismatch,
                        (loc, msg),
                        (*vloc, defn_loc_error(&name)),
                    );
                    diag.add_note(format!("Remove '()' arguments from this {position}"));
                    context.env.add_diag(diag);
                }
                CF::Parens if vfields.is_positional() => (),
                CF::Parens => {
                    let msg = invalid_inst_msg!("variant", NAMED_UPCASE, NAMED);
                    let mut diag = diag!(
                        NameResolution::PositionalCallMismatch,
                        (loc, &msg),
                        (*vloc, defn_loc_error(&name)),
                    );
                    diag.add_note(named_note!());
                    context.env.add_diag(diag);
                }
                CF::Braces if vfields.is_empty() => {
                    let msg = invalid_inst_msg!("variant", EMPTY_UPCASE, EMPTY);
                    let mut diag = diag!(
                        NameResolution::PositionalCallMismatch,
                        (loc, msg),
                        (*vloc, defn_loc_error(&name)),
                    );
                    diag.add_note(format!("Remove '{{ }}' arguments from this {position}"));
                    context.env.add_diag(diag);
                }
                CF::Braces if vfields.is_positional() => {
                    let msg = invalid_inst_msg!("variant", POSNL_UPCASE, POSNL);
                    let mut diag = diag!(
                        NameResolution::PositionalCallMismatch,
                        (loc, &msg),
                        (*vloc, defn_loc_error(&name)),
                    );
                    diag.add_note(posnl_note!());
                    context.env.add_diag(diag);
                }
                CF::Braces => (),
            }
        }
    }
}

fn report_invalid_macro(context: &mut Context, is_macro: Option<Loc>, kind: &str) {
    if let Some(mloc) = is_macro {
        let msg = format!(
            "Unexpected macro invocation. {} cannot be invoked as macros",
            kind
        );
        context
            .env
            .add_diag(diag!(NameResolution::PositionalCallMismatch, (mloc, msg)));
    }
}

//************************************************
// Match Arms and Patterns
//************************************************

fn match_arm(context: &mut Context, sp!(aloc, arm): E::MatchArm) -> N::MatchArm {
    let E::MatchArm_ {
        pattern,
        guard,
        rhs,
    } = arm;

    let pat_binders = unique_pattern_binders(context, &pattern);

    context.new_local_scope();
    // NB: we just checked the binders for duplicates and listed them all, so now we just need to
    // set up the map and recur down everything.
    let binders: Vec<(Mutability, N::Var)> = pat_binders
        .clone()
        .into_iter()
        .map(|(mut_, binder)| {
            (
                mut_,
                context.declare_local(/* is_parameter */ false, binder.0),
            )
        })
        .collect::<Vec<_>>();

    // Guards are a little tricky: we need them to have similar binders, but they must be different
    // because they may be typed differently than the actual binders (as they are always immutable
    // references). So we push a new scope with new binders paired with the pattern ones, process the
    // guard, and then update the usage of the old binders to account for guard usage.
    context.new_local_scope();
    let guard_binder_pairs: Vec<(N::Var, N::Var)> = binders
        .clone()
        .into_iter()
        .map(|(_, pat_var)| {
            let guard_var = context.declare_local(
                /* is_parameter */ false,
                sp(pat_var.loc, pat_var.value.name),
            );
            (pat_var, guard_var)
        })
        .collect::<Vec<_>>();
    // Next we process the guard to mark guard usage for the guard variables.
    let guard = guard.map(|guard| exp(context, guard));

    // Next we compute the used guard variables, and add the pattern/guard pairs to the guard
    // binders. We assume we don't need to mark unused guard bindings as used (to avoid incorrect
    // unused errors) because we will never check their usage in the unused-checking pass.
    //
    // We also need to mark usage for the pattern variables we do use, but we postpone that until
    // after we handle the right-hand side.
    let mut guard_binders = UniqueMap::new();
    for (pat_var, guard_var) in guard_binder_pairs {
        if context.used_locals.contains(&guard_var.value) {
            guard_binders
                .add(pat_var, guard_var)
                .expect("ICE guard pattern issue");
        }
    }
    context.close_local_scope();

    // Then we visit the right-hand side to mark binder usage there, then compute all the pattern
    // binders used in the right-hand side. Since we didn't mark pattern variables as used by the
    // guard yet, this allows us to record exactly those pattern variables used in the right-hand
    // side so that we can avoid binding them later.
    let rhs = exp(context, rhs);
    let rhs_binders: BTreeSet<N::Var> = binders
        .iter()
        .filter(|(_, binder)| context.used_locals.contains(&binder.value))
        .map(|(_, binder)| *binder)
        .collect();

    // Now we mark usage for the guard-used pattern variables.
    for (pat_var, _) in guard_binders.key_cloned_iter() {
        context.used_locals.insert(pat_var.value);
    }

    // Finally we handle the pattern, replacing unused variables with wildcards
    let pattern = *match_pattern(context, Box::new(pattern));

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

fn unique_pattern_binders(
    context: &mut Context,
    pattern: &E::MatchPattern,
) -> Vec<(Mutability, P::Var)> {
    use E::MatchPattern_ as EP;

    fn report_duplicate(context: &mut Context, var: P::Var, locs: &Vec<(Mutability, Loc)>) {
        assert!(locs.len() > 1, "ICE pattern duplicate detection error");
        let (_, first_loc) = locs.first().unwrap();
        let mut diag = diag!(
            NameResolution::InvalidPattern,
            (*first_loc, format!("binder '{}' is defined here", var))
        );
        for (_, loc) in locs.iter().skip(1) {
            diag.add_secondary_label((*loc, "and repeated here"));
        }
        diag.add_note("A pattern variable must be unique, and must appear once in each or-pattern alternative.");
        context.env.add_diag(diag);
    }

    enum OrPosn {
        Left,
        Right,
    }

    fn report_mismatched_or(context: &mut Context, posn: OrPosn, var: &P::Var, other_loc: Loc) {
        let (primary_side, secondary_side) = match posn {
            OrPosn::Left => ("left", "right"),
            OrPosn::Right => ("right", "left"),
        };
        let primary_msg = format!("{} or-pattern binds variable {}", primary_side, var);
        let secondary_msg = format!("{} or-pattern does not", secondary_side);
        let mut diag = diag!(NameResolution::InvalidPattern, (var.loc(), primary_msg));
        diag.add_secondary_label((other_loc, secondary_msg));
        diag.add_note("Both sides of an or-pattern must bind the same variables.");
        context.env.add_diag(diag);
    }

    fn report_mismatched_or_mutability(
        context: &mut Context,
        mutable_loc: Loc,
        immutable_loc: Loc,
        var: &P::Var,
        posn: OrPosn,
    ) {
        let (primary_side, secondary_side) = match posn {
            OrPosn::Left => ("left", "right"),
            OrPosn::Right => ("right", "left"),
        };
        let primary_msg = format!("{} or-pattern binds variable {} mutably", primary_side, var);
        let secondary_msg = format!("{} or-pattern binds it immutably", secondary_side);
        let mut diag = diag!(NameResolution::InvalidPattern, (mutable_loc, primary_msg));
        diag.add_secondary_label((immutable_loc, secondary_msg));
        diag.add_note(
            "Both sides of an or-pattern must bind the same variables with the same mutability.",
        );
        context.env.add_diag(diag);
    }

    type Bindings = BTreeMap<P::Var, Vec<(Mutability, Loc)>>;

    fn report_duplicates_and_combine(
        context: &mut Context,
        all_bindings: Vec<Bindings>,
    ) -> Bindings {
        match all_bindings.len() {
            0 => BTreeMap::new(),
            1 => all_bindings[0].clone(),
            _ => {
                let mut out_bindings = all_bindings[0].clone();
                let mut duplicates = BTreeSet::new();
                for bindings in all_bindings.into_iter().skip(1) {
                    for (key, mut locs) in bindings {
                        if out_bindings.contains_key(&key) {
                            duplicates.insert(key);
                        }
                        out_bindings.entry(key).or_default().append(&mut locs);
                    }
                }
                for key in duplicates {
                    report_duplicate(context, key, out_bindings.get(&key).unwrap());
                }
                out_bindings
            }
        }
    }

    fn check_duplicates(context: &mut Context, sp!(ploc, pattern): &E::MatchPattern) -> Bindings {
        match pattern {
            EP::Binder(_, var) if var.is_underscore() => BTreeMap::new(),
            EP::Binder(mut_, var) => [(*var, vec![(*mut_, *ploc)])].into_iter().collect(),
            EP::At(var, inner) => {
                let mut bindings: Bindings = BTreeMap::new();
                if !var.is_underscore() {
                    bindings
                        .entry(*var)
                        .or_default()
                        .push((Mutability::Imm, *ploc));
                }
                let new_bindings = check_duplicates(context, inner);
                bindings = report_duplicates_and_combine(context, vec![bindings, new_bindings]);
                bindings
            }
            EP::PositionalConstructor(_, _, sp!(_, patterns)) => {
                let bindings = patterns
                    .iter()
                    .filter_map(|pat| match pat {
                        E::Ellipsis::Binder(p) => Some(check_duplicates(context, p)),
                        E::Ellipsis::Ellipsis(_) => None,
                    })
                    .collect();
                report_duplicates_and_combine(context, bindings)
            }
            EP::NamedConstructor(_, _, fields, _) => {
                let mut bindings = vec![];
                for (_, _, (_, pat)) in fields {
                    bindings.push(check_duplicates(context, pat));
                }
                report_duplicates_and_combine(context, bindings)
            }
            EP::Or(left, right) => {
                let mut left_bindings = check_duplicates(context, left);
                let mut right_bindings = check_duplicates(context, right);
                for (key, mut_and_locs) in left_bindings.iter_mut() {
                    if !right_bindings.contains_key(key) {
                        report_mismatched_or(context, OrPosn::Left, key, right.loc);
                    } else {
                        let lhs_mutability = mut_and_locs.first().map(|(m, _)| *m).unwrap();
                        let rhs_mutability = right_bindings
                            .get(key)
                            .map(|mut_and_locs| mut_and_locs.first().map(|(m, _)| *m).unwrap())
                            .unwrap();
                        match (lhs_mutability, rhs_mutability) {
                            // LHS variable mutable, RHS variable immutable
                            (Mutability::Mut(lhs_loc), Mutability::Imm) => {
                                report_mismatched_or_mutability(
                                    context,
                                    lhs_loc,
                                    right.loc,
                                    key,
                                    OrPosn::Left,
                                );
                                // Mutabilities are mismatched so update them to all be mutable to
                                // avoid further errors further down the line.
                                if let Some(mut_and_locs) = right_bindings.get_mut(key) {
                                    for m in mut_and_locs
                                        .iter_mut()
                                        .filter(|(m, _)| matches!(m, Mutability::Imm))
                                    {
                                        m.0 = Mutability::Mut(lhs_loc);
                                    }
                                }
                            }
                            (Mutability::Imm, Mutability::Mut(rhs_loc)) => {
                                // RHS variable mutable, LHS variable immutable
                                report_mismatched_or_mutability(
                                    context,
                                    rhs_loc,
                                    key.loc(),
                                    key,
                                    OrPosn::Right,
                                );
                                // Mutabilities are mismatched so update them to all be mutable to
                                // avoid further errors further down the line.
                                for m in mut_and_locs
                                    .iter_mut()
                                    .filter(|(m, _)| matches!(m, Mutability::Imm))
                                {
                                    m.0 = Mutability::Mut(rhs_loc);
                                }
                            }
                            _ => (),
                        }
                    }
                }

                let right_keys = right_bindings.keys().copied().collect::<Vec<_>>();
                for key in right_keys {
                    let lhs_entry = left_bindings.get_mut(&key);
                    let rhs_entry = right_bindings.remove(&key);
                    match (lhs_entry, rhs_entry) {
                        (Some(left_locs), Some(mut right_locs)) => {
                            left_locs.append(&mut right_locs);
                        }
                        (None, Some(right_locs)) => {
                            report_mismatched_or(context, OrPosn::Right, &key, left.loc);
                            left_bindings.insert(key, right_locs);
                        }
                        (_, None) => panic!("ICE pattern key missing"),
                    }
                }
                left_bindings
            }
            EP::ModuleAccessName(_, _) | EP::Literal(_) | EP::ErrorPat => BTreeMap::new(),
        }
    }

    check_duplicates(context, pattern)
        .into_iter()
        .map(|(var, vs)| (vs.first().map(|x| x.0).unwrap(), var))
        .collect::<Vec<_>>()
}

fn expand_positional_ellipsis<T>(
    missing: isize,
    args: Vec<E::Ellipsis<Spanned<T>>>,
    replacement: impl Fn(Loc) -> Spanned<T>,
) -> Vec<(Field, (usize, Spanned<T>))> {
    args.into_iter()
        .flat_map(|p| match p {
            E::Ellipsis::Binder(p) => vec![p],
            E::Ellipsis::Ellipsis(eloc) => {
                (0..=missing).map(|_| replacement(eloc)).collect::<Vec<_>>()
            }
        })
        .enumerate()
        .map(|(idx, p)| {
            let field = Field::add_loc(p.loc, format!("{idx}").into());
            (field, (idx, p))
        })
        .collect()
}

fn expand_named_ellipsis<T>(
    field_info: &FieldInfo,
    head_loc: Loc,
    eloc: Loc,
    args: &mut UniqueMap<Field, (usize, Spanned<T>)>,
    replacement: impl Fn(Loc) -> Spanned<T>,
) {
    let mut fields = match field_info {
        FieldInfo::Empty => BTreeSet::new(),
        FieldInfo::Named(fields) => fields.clone(),
        FieldInfo::Positional(num_fields) => (0..*num_fields)
            .map(|i| Field::add_loc(head_loc, format!("{i}").into()))
            .collect(),
    };

    for (k, _) in args.key_cloned_iter() {
        fields.remove(&k);
    }

    let start_idx = args.len();
    for (i, f) in fields.into_iter().enumerate() {
        args.add(
            Field(sp(eloc, f.value())),
            (start_idx + i, replacement(eloc)),
        )
        .unwrap();
    }
}

fn match_pattern(context: &mut Context, in_pat: Box<E::MatchPattern>) -> Box<N::MatchPattern> {
    use E::MatchPattern_ as EP;
    use N::MatchPattern_ as NP;

    let sp!(ploc, pat_) = *in_pat;

    let pat_: N::MatchPattern_ = match pat_ {
        EP::PositionalConstructor(nac, args) => {
            let Some(result!(ctor, ptys_opt, is_macro)) = context.resolver.resolve_constructor(nac)
            else {
                assert!(context.env.has_errors());
                return Box::new(sp(ploc, NP::ErrorPat));
            };
            ice_assert!(context.env(), is_macro.is_none(), ploc, "Macro in pattern");
            let tys_opt = opt_types_with_arity_check(
                context,
                TypeAnnotation::Expression,
                ploc,
                || format!("{}::{}", &ctor.module(), ctor.type_name()),
                ptys_opt,
                ctor.type_arity(),
            );

            check_constructor_form(context, ploc, ConstructorForm::Parens, "pattern", &ctor);

            let field_info = ctor.field_info();
            let n_pats = args
                .value
                .into_iter()
                .map(|ellipsis| match ellipsis {
                    Ellipsis::Binder(pat) => {
                        Ellipsis::Binder(*match_pattern(context, Box::new(pat)))
                    }
                    Ellipsis::Ellipsis(loc) => Ellipsis::Ellipsis(loc),
                })
                .collect::<Vec<_>>();
            // NB: We may have more args than fields! Since we allow `..` to be zero-or-more
            // wildcards.
            let missing = (field_info.field_count() as isize) - n_pats.len() as isize;
            let args = expand_positional_ellipsis(missing, n_pats, |eloc| sp(eloc, NP::Wildcard));
            let result_args =
                UniqueMap::maybe_from_iter(args.into_iter()).expect("ICE naming failed");

            match ctor {
                ResolvedConstructor::Struct(s) => {
                    NP::Struct(s.module, s.name, tys_opt, result_args)
                }
                ResolvedConstructor::Variant(v) => {
                    NP::Variant(v.module, v.enum_name, v.name, tys_opt, result_args)
                }
            }
        }
        EP::NamedConstructor(nac, args, ellipsis) => {
            let Some(result!(ctor, ptys_opt, is_macro)) = context.resolver.resolve_constructor(nac)
            else {
                assert!(context.env.has_errors());
                return Box::new(sp(ploc, NP::ErrorPat));
            };
            ice_assert!(context.env(), is_macro.is_none(), ploc, "Macro in pattern");
            let tys_opt = opt_types_with_arity_check(
                context,
                TypeAnnotation::Expression,
                ploc,
                || format!("{}::{}", &ctor.module(), ctor.type_name()),
                ptys_opt,
                ctor.type_arity(),
            );

            check_constructor_form(context, ploc, ConstructorForm::Braces, "pattern", &ctor);

            let field_info = ctor.field_info();
            let mut args = args.map(|_, (idx, p)| (idx, *match_pattern(context, Box::new(p))));
            // If we have an ellipsis fill in any missing patterns
            if let Some(ellipsis_loc) = ellipsis {
                expand_named_ellipsis(&field_info, ploc, ellipsis_loc, &mut args, |eloc| {
                    sp(eloc, NP::Wildcard)
                });
            }

            match ctor {
                ResolvedConstructor::Struct(s) => NP::Struct(s.module, s.name, tys_opt, args),
                ResolvedConstructor::Variant(v) => {
                    NP::Variant(v.module, v.enum_name, v.name, tys_opt, args)
                }
            }
        }
        EP::Name(mut_, nac) => {
            let Some(result!(pat, ptys_opt, is_macro)) = context.resolver.resolve_pattern_name(nac)
            else {
                assert!(context.env.has_errors());
                NP::ErrorPat
            };
            ice_assert!(context.env(), is_macro.is_none(), ploc, "Macro in pattern");
            match pat {
                ResolvedPatternName::Constant(const_) => {
                    if ptys_opt.is_some() {
                        context.env.add_diag(diag!(
                            NameResolution::TooManyTypeArguments,
                            (ploc, "Constants in patterns do not take type arguments")
                        ));
                    }
                    NP::Constant(const_.module, const_.name)
                }
                ResolvedPatternName::Variant(v) => {
                    ice_assert!(context.env(), is_macro.is_none(), ploc, "Macro in pattern");
                    let tys_opt = opt_types_with_arity_check(
                        context,
                        TypeAnnotation::Expression,
                        ploc,
                        || format!("{}::{}", &v.module, v.enum_name),
                        ptys_opt,
                        v.tyarg_arity,
                    );
                    let ctor = &ResolvedConstructor::Variant(v);
                    check_constructor_form(context, ploc, ConstructorForm::Braces, "pattern", ctor);
                    NP::Variant(v.module, v.enum_name, v.name, tys_opt, UniqueMap::new())
                }
                ResolvedPatternName::Variable(x) => {
                    if ptys_opt.is_some() {
                        let msg = "Invalid type arguments on a pattern variable";
                        let mut diag = diag!(Declarations::InvalidName, (x.loc, msg));
                        diag.add_note("Type arguments cannot appear on pattern variables");
                        context.env().add_diag(diag);
                    }
                    NP::Binder(mut_, x, false)
                }
                ResolvedPatternName::Wildcard => {
                    check_mut_underscore(context, Some(mut_));
                    NP::Wildcard
                }
            }
        }
        EP::ErrorPat => NP::ErrorPat,
        EP::Literal(v) => NP::Literal(v),
        EP::Or(lhs, rhs) => NP::Or(match_pattern(context, lhs), match_pattern(context, rhs)),
        EP::At(binder, body) => {
            if let Some(binder) = context.resolver.resolve_pattern_binder(binder.0) {
                NP::At(
                    binder,
                    /* unused_binding */ false,
                    match_pattern(context, body),
                )
            } else {
                assert!(context.env.has_errors());
                match_pattern(context, body).value
            }
        }
    };
    Box::new(sp(ploc, pat_))
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

    fn check_duplicate_assignment(
        context: &mut Context,
        seen_locals: &mut UniqueMap<Name, ()>,
        case: LValueCase,
        name: Name,
    ) {
        if let Err((var, prev_loc)) = seen_locals.add(name, ()) {
            let (primary, secondary) = match case {
                C::Bind => {
                    let msg = format!(
                        "Duplicate declaration for local '{}' in a given 'let'",
                        &var
                    );
                    ((var.loc, msg), (prev_loc, "Previously declared here"))
                }
                C::Assign => {
                    let msg = format!("Duplicate usage of local '{}' in a given assignment", &var);
                    ((var.loc, msg), (prev_loc, "Previously assigned here"))
                }
            };
            context
                .env()
                .add_diag(diag!(Declarations::DuplicateItem, primary, secondary));
        }
    }

    fn is_syntax(context: &mut Context, case: LValueCase, name: Name) -> bool {
        if name.is_syntax_identifier() {
            debug_assert!(
                matches!(case, C::Assign),
                "ICE this should fail during parsing"
            );
            let msg = format!(
                "Cannot assign to argument for parameter '{}'. \
                            Arguments must be used in value positions",
                name
            );
            let mut diag = diag!(TypeSafety::CannotExpandMacro, (name.loc, msg));
            diag.add_note(ASSIGN_SYNTAX_IDENTIFIER_NOTE);
            context.env().add_diag(diag);
            return false;
        }
        true
    }

    let nl_ = match l_ {
        EL::Var(mut_, nac) => {
            let nloc = nac.loc;
            let Some(lvalue_name) = context.resolver.resolve_lvalue_name(nac) else {
                assert!(context.env().has_errors());
                None
            };
            match lvalue_name {
                ResolvedLValueName::Variable(var) => {
                    check_duplicate_assignment(context, seen_locals, case, var);
                    if is_syntax(context, case, var.0) {
                        return None;
                    }
                    let nv = match case {
                        C::Bind => {
                            // Even if we resolved to a variable, this is a new binder actually, so
                            // we declare a new local from its base name.
                            let is_parameter = false;
                            context.declare_local(is_parameter, var.0)
                        }
                        C::Assign => var,
                    };
                    NL::Var {
                        mut_,
                        var: nv,
                        // set later
                        unused_binding: false,
                    }
                }
                ResolvedLValueName::UnresolvedName(name) => {
                    check_duplicate_assignment(context, seen_locals, case, name);
                    if is_syntax(context, case, name) {
                        return None;
                    }
                    match case {
                        C::Bind => {
                            // If we didn't resolve the name, this is a new binder.
                            let is_parameter = false;
                            let nv = context.declare_local(is_parameter, name);
                            NL::Var {
                                mut_,
                                var: nv,
                                // set later
                                unused_binding: false,
                            }
                        }
                        C::Assign => {
                            // If we're in assignment, failing to resolve is an error.
                            let msg = format!("Invalid assignment. Unbound variable '{name}'");
                            let diag = diag!(NameResolution::UnboundVariable, (loc, msg));
                            context.env().add_diag(diag);
                            return None;
                        }
                    }
                }
                ResolvedLValueName::Wildcard => {
                    check_mut_underscore(context, mut_);
                    NL::Ignore
                }
            }
        }
        EL::Unpack(nac, fields) => {
            let nloc = nac.loc;
            let msg = match case {
                C::Bind => "deconstructing binding",
                C::Assign => "deconstructing assignment",
            };
            let ctor_form = match &fields {
                E::FieldBindings::Named(_, _) => ConstructorForm::Braces,
                E::FieldBindings::Positional(_) => ConstructorForm::Parens,
            };
            let (struct_, ptys_opt) = match context.resolver.resolve_constructor(nac) {
                Some(result!(ctor, ptys_opt, is_macro)) => {
                    ice_assert!(context.env(), is_macro.is_none(), loc, "Found macro in lhs");
                    check_constructor_form(context, nloc, ctor_form, msg, &ctor);
                    match ctor {
                        ResolvedConstructor::Struct(struct_) => (struct_, ptys_opt),
                        ResolvedConstructor::Variant(variant) => {
                            context.env().add_diag(diag!(
                                NameResolution::NamePositionMismatch,
                                (nloc.loc, format!("Invalid {}. Expected a struct", msg)),
                                (
                                    variant.decl_loc,
                                    format!("But '{}' is a variant", variant.name)
                                )
                            ));
                            return None;
                        }
                    }
                }
                None => {
                    assert!(context.env.has_errors());
                    return None;
                }
            };
            let make_ignore = |loc| {
                let var = sp(loc, Symbol::from("_"));
                let name = E::ModuleAccess::new(loc, E::ModuleAccess_::Name(var));
                sp(loc, E::LValue_::Var(None, name))
            };
            let efields = match fields {
                E::FieldBindings::Named(mut efields, ellipsis) => {
                    if let Some(ellipsis_loc) = ellipsis {
                        expand_named_ellipsis(
                            &struct_.field_info,
                            loc,
                            ellipsis_loc,
                            &mut efields,
                            make_ignore,
                        );
                    }

                    efields
                }
                E::FieldBindings::Positional(lvals) => {
                    let fields = struct_.field_info.field_count();
                    let missing = (fields as isize) - lvals.len() as isize;

                    let expanded_lvals = expand_positional_ellipsis(missing, lvals, make_ignore);
                    UniqueMap::maybe_from_iter(expanded_lvals.into_iter()).unwrap()
                }
            };
            let nfields =
                UniqueMap::maybe_from_opt_iter(efields.into_iter().map(|(k, (idx, inner))| {
                    Some((k, (idx, lvalue(context, seen_locals, case, inner)?)))
                }))?;
            // TODO: We could type ptys_opt arity here, but typing will re-check it.
            NL::Unpack(
                struct_.module,
                struct_.name,
                ptys_opt,
                nfields.expect("ICE fields were already unique"),
            )
        }
    };
    Some(sp(loc, nl_))
}

fn check_mut_underscore(context: &mut Context, mut_: Option<Mutability>) {
    // no error if not a mut declaration
    let Some(Mutability::Mut(loc)) = mut_ else {
        return;
    };
    let msg = "Invalid 'mut' declaration. 'mut' is applied to variables and cannot be applied to the '_' pattern";
    context
        .env
        .add_diag(diag!(NameResolution::InvalidMut, (loc, msg)));
}

fn bind_list(context: &mut Context, ls: E::LValueList) -> Option<N::LValueList> {
    lvalue_list(context, &mut UniqueMap::new(), LValueCase::Bind, ls)
}

fn lambda_bind_list(
    context: &mut Context,
    sp!(loc, elambda): E::LambdaLValues,
) -> Option<N::LambdaLValues> {
    let nlambda = elambda
        .into_iter()
        .map(|(pbs, ty_opt)| {
            let bs = bind_list(context, pbs)?;
            let ety = ty_opt.map(|t| type_(context, TypeAnnotation::Expression, t));
            Some((bs, ety))
        })
        .collect::<Option<_>>()?;
    Some(sp(loc, nlambda))
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
                remove_unused_bindings_lvalues(context, used, lvalues)
            }
            N::SequenceItem_::Bind(lvalues, e) => {
                remove_unused_bindings_lvalues(context, used, lvalues);
                remove_unused_bindings_exp(context, used, e)
            }
        }
    }
}

fn remove_unused_bindings_lvalues(
    context: &mut Context,
    used: &BTreeSet<N::Var_>,
    sp!(_, lvalues): &mut N::LValueList,
) {
    for lvalue in lvalues {
        remove_unused_bindings_lvalue(context, used, lvalue)
    }
}

fn remove_unused_bindings_lvalue(
    context: &mut Context,
    used: &BTreeSet<N::Var_>,
    sp!(_, lvalue_): &mut N::LValue,
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
            report_unused_local(context, var);
            *unused_binding = true;
        }
        N::LValue_::Unpack(_, _, _, lvalues) => {
            for (_, _, (_, lvalue)) in lvalues {
                remove_unused_bindings_lvalue(context, used, lvalue)
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
        | N::Exp_::ErrorConstant { .. }
        | N::Exp_::UnresolvedError => (),
        N::Exp_::Return(e)
        | N::Exp_::Abort(e)
        | N::Exp_::Dereference(e)
        | N::Exp_::UnaryExp(_, e)
        | N::Exp_::Cast(e, _)
        | N::Exp_::Assign(_, e)
        | N::Exp_::Loop(_, e)
        | N::Exp_::Give(_, _, e)
        | N::Exp_::Annotate(e, _) => remove_unused_bindings_exp(context, used, e),
        N::Exp_::IfElse(econd, et, ef) => {
            remove_unused_bindings_exp(context, used, econd);
            remove_unused_bindings_exp(context, used, et);
            remove_unused_bindings_exp(context, used, ef);
        }
        N::Exp_::Match(esubject, arms) => {
            remove_unused_bindings_exp(context, used, esubject);
            for arm in &mut arms.value {
                remove_unused_bindings_pattern(context, used, &mut arm.value.pattern);
                if let Some(guard) = arm.value.guard.as_mut() {
                    remove_unused_bindings_exp(context, used, guard)
                }
                remove_unused_bindings_exp(context, used, &mut arm.value.rhs);
            }
        }
        N::Exp_::While(_, econd, ebody) => {
            remove_unused_bindings_exp(context, used, econd);
            remove_unused_bindings_exp(context, used, ebody)
        }
        N::Exp_::Block(N::Block {
            name: _,
            from_macro_argument: _,
            seq,
        }) => remove_unused_bindings_seq(context, used, seq),
        N::Exp_::Lambda(N::Lambda {
            parameters: sp!(_, parameters),
            return_label: _,
            return_type: _,
            use_fun_color: _,
            body,
        }) => {
            for (lvs, _) in parameters {
                remove_unused_bindings_lvalues(context, used, lvs)
            }
            remove_unused_bindings_exp(context, used, body)
        }
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
        | N::Exp_::ModuleCall(_, _, _, _, sp!(_, es))
        | N::Exp_::VarCall(_, sp!(_, es))
        | N::Exp_::ExpList(es) => {
            for e in es {
                remove_unused_bindings_exp(context, used, e)
            }
        }
        N::Exp_::MethodCall(ed, _, _, _, sp!(_, es)) => {
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
        N::ExpDotted_::Dot(ed, _) | N::ExpDotted_::DotUnresolved(_, ed) => {
            remove_unused_bindings_exp_dotted(context, used, ed)
        }
        N::ExpDotted_::Index(ed, sp!(_, es)) => {
            for e in es {
                remove_unused_bindings_exp(context, used, e);
            }
            remove_unused_bindings_exp_dotted(context, used, ed)
        }
    }
}

fn remove_unused_bindings_pattern(
    context: &mut Context,
    used: &BTreeSet<N::Var_>,
    sp!(_, pat_): &mut N::MatchPattern,
) {
    use N::MatchPattern_ as NP;
    match pat_ {
        NP::Constant(_, _) | NP::Literal(_) | NP::Wildcard | NP::ErrorPat => (),
        NP::Variant(_, _, _, _, fields) => {
            for (_, _, (_, pat)) in fields {
                remove_unused_bindings_pattern(context, used, pat)
            }
        }
        NP::Struct(_, _, _, fields) => {
            for (_, _, (_, pat)) in fields {
                remove_unused_bindings_pattern(context, used, pat)
            }
        }
        NP::Binder(_, var, unused_binding) => {
            if !used.contains(&var.value) {
                report_unused_local(context, var);
                *unused_binding = true;
            }
        }
        NP::Or(lhs, rhs) => {
            remove_unused_bindings_pattern(context, used, lhs);
            remove_unused_bindings_pattern(context, used, rhs);
        }
        NP::At(var, unused_binding, inner) => {
            if !used.contains(&var.value) {
                report_unused_local(context, var);
                *unused_binding = true;
                remove_unused_bindings_pattern(context, used, inner);
            } else {
                remove_unused_bindings_pattern(context, used, &mut *inner);
            }
        }
    }
}

fn report_unused_local(context: &mut Context, sp!(loc, unused_): &N::Var) {
    if unused_.starts_with_underscore() || !unused_.is_valid() {
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
