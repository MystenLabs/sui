//*************************************************************************************************
// Entry
//**************************************************************************************************

// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    editions::{FeatureGate, Flavor},
    expansion::ast::{self as E, Fields, ModuleIdent},
    hlir::ast::{self as H, Block, BlockLabel, MoveOpAnnotation},
    hlir::detect_dead_code::program as detect_dead_code_analysis,
    naming::ast as N,
    parser::ast::{
        Ability_, BinOp, BinOp_, ConstantName, DatatypeName, Field, FunctionName, VariantName,
    },
    shared::{ast_debug::AstDebug, process_binops, unique_map::UniqueMap, *},
    sui_mode::ID_FIELD_NAME,
    typing::ast as T,
    FullyCompiledProgram,
};

use move_ir_types::{ast::UnpackType, location::*};
use move_symbol_pool::Symbol;
use once_cell::sync::Lazy;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    convert::TryInto,
};

use super::match_compilation;

//**************************************************************************************************
// Vars
//**************************************************************************************************

const NEW_NAME_DELIM: &str = "#";

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

fn translate_block_label(N::BlockLabel(sp!(loc, v_)): N::BlockLabel) -> H::BlockLabel {
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
    H::BlockLabel(sp(loc, s))
}

const TEMP_PREFIX: &str = "%";
static TEMP_PREFIX_SYMBOL: Lazy<Symbol> = Lazy::new(|| TEMP_PREFIX.into());

const MATCH_TEMP_PREFIX: &str = "__match_tmp%";
static MATCH_TEMP_PREFIX_SYMBOL: Lazy<Symbol> = Lazy::new(|| MATCH_TEMP_PREFIX.into());

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

type VariantFieldIndicies = UniqueMap<
    ModuleIdent,
    UniqueMap<DatatypeName, UniqueMap<VariantName, UniqueMap<Field, usize>>>,
>;

pub struct Context<'env> {
    pub env: &'env mut CompilationEnv,
    current_package: Option<Symbol>,
    structs: UniqueMap<ModuleIdent, UniqueMap<DatatypeName, UniqueMap<Field, usize>>>,
    enum_variants: UniqueMap<ModuleIdent, UniqueMap<DatatypeName, Vec<VariantName>>>,
    variant_fields: VariantFieldIndicies,
    function_locals: UniqueMap<H::Var, H::SingleType>,
    signature: Option<H::FunctionSignature>,
    tmp_counter: usize,
    named_block_binders: UniqueMap<H::BlockLabel, Vec<H::LValue>>,
    named_block_types: UniqueMap<H::BlockLabel, H::Type>,
    /// collects all struct fields used in the current module
    pub used_fields: BTreeMap<Symbol, BTreeSet<Symbol>>,
}

impl<'env> Context<'env> {
    pub fn new(
        env: &'env mut CompilationEnv,
        pre_compiled_lib_opt: Option<&FullyCompiledProgram>,
        prog: &T::Program_,
    ) -> Self {
        fn add_struct_fields(
            structs: &mut UniqueMap<ModuleIdent, UniqueMap<DatatypeName, UniqueMap<Field, usize>>>,
            mident: ModuleIdent,
            struct_defs: &UniqueMap<DatatypeName, N::StructDefinition>,
        ) {
            let mut cur_structs = UniqueMap::new();
            for (sname, sdef) in struct_defs.key_cloned_iter() {
                let mut fields = UniqueMap::new();
                let field_map = match &sdef.fields {
                    N::StructFields::Native(_) => continue,
                    N::StructFields::Defined(m) => m,
                };
                for (field, (idx, _)) in field_map.key_cloned_iter() {
                    fields.add(field, *idx).unwrap();
                }
                cur_structs.add(sname, fields).unwrap();
            }
            structs.remove(&mident);
            structs.add(mident, cur_structs).unwrap();
        }

        fn add_enums(
            enum_variants: &mut UniqueMap<ModuleIdent, UniqueMap<DatatypeName, Vec<VariantName>>>,
            variant_fields: &mut VariantFieldIndicies,
            mident: ModuleIdent,
            enum_defs: &UniqueMap<DatatypeName, N::EnumDefinition>,
        ) {
            let mut cur_enums_variants = UniqueMap::new();
            let mut cur_enums_variant_fields = UniqueMap::new();
            for (ename, edef) in enum_defs.key_cloned_iter() {
                let mut variant_fields = UniqueMap::new();
                let mut indexed_variants = vec![];
                for (variant_name, vdef) in edef.variants.key_cloned_iter() {
                    indexed_variants.push((variant_name, vdef.index));
                    let mut fields = UniqueMap::new();
                    match &vdef.fields {
                        N::VariantFields::Empty => (),
                        N::VariantFields::Defined(m) => {
                            for (field, (idx, _)) in m.key_cloned_iter() {
                                fields.add(field, *idx).unwrap();
                            }
                        }
                    }
                    variant_fields.add(variant_name, fields).unwrap();
                }
                indexed_variants.sort_by(|(_, ndx0), (_, ndx1)| ndx0.cmp(ndx1));
                cur_enums_variants
                    .add(
                        ename,
                        indexed_variants
                            .into_iter()
                            .map(|(key, _ndx)| key)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap();
                cur_enums_variant_fields.add(ename, variant_fields).unwrap();
            }
            enum_variants.remove(&mident);
            enum_variants.add(mident, cur_enums_variants).unwrap();
            variant_fields.remove(&mident);
            variant_fields
                .add(mident, cur_enums_variant_fields)
                .unwrap();
        }

        let mut structs = UniqueMap::new();
        let mut enum_variants = UniqueMap::new();
        let mut variant_fields = UniqueMap::new();
        if let Some(pre_compiled_lib) = pre_compiled_lib_opt {
            for (mident, mdef) in pre_compiled_lib.typing.inner.modules.key_cloned_iter() {
                add_struct_fields(&mut structs, mident, &mdef.structs);
                // add_enums(&mut enums, &mut variant_fields, mident, &mdef.enums);
                add_enums(&mut enum_variants, &mut variant_fields, mident, &mdef.enums);
            }
        }
        for (mident, mdef) in prog.modules.key_cloned_iter() {
            add_struct_fields(&mut structs, mident, &mdef.structs);
            add_enums(&mut enum_variants, &mut variant_fields, mident, &mdef.enums);
        }
        Context {
            env,
            current_package: None,
            structs,
            enum_variants,
            variant_fields,
            function_locals: UniqueMap::new(),
            signature: None,
            tmp_counter: 0,
            used_fields: BTreeMap::new(),
            named_block_binders: UniqueMap::new(),
            named_block_types: UniqueMap::new(),
        }
    }

    pub fn has_empty_locals(&self) -> bool {
        self.function_locals.is_empty()
    }

    pub fn extract_function_locals(&mut self) -> UniqueMap<H::Var, H::SingleType> {
        self.tmp_counter = 0;
        std::mem::replace(&mut self.function_locals, UniqueMap::new())
    }

    pub fn new_temp(&mut self, loc: Loc, t: H::SingleType) -> H::Var {
        let new_var = H::Var(sp(loc, new_temp_name(self)));
        self.function_locals.add(new_var, t).unwrap();

        new_var
    }

    /// Makes a new `naming/ast.rs` variable. Does _not_ record it as a function local.
    pub fn new_match_var(&mut self, name: String, loc: Loc) -> N::Var {
        let id = self.counter_next();
        let name = format!(
            "{}{}{}{}{}",
            *MATCH_TEMP_PREFIX_SYMBOL, NEW_NAME_DELIM, name, NEW_NAME_DELIM, id
        )
        .into();
        sp(
            loc,
            N::Var_ {
                name,
                id: id as u16,
                color: 1,
            },
        )
    }

    pub fn bind_local(&mut self, v: N::Var, t: H::SingleType) {
        let symbol = translate_var(v);
        if let Some(cur_t) = self.function_locals.get(&symbol) {
            assert!(cur_t == &t);
        } else {
            self.function_locals.add(symbol, t).unwrap();
        }
    }

    pub fn record_named_block_binders(
        &mut self,
        block_name: H::BlockLabel,
        binders: Vec<H::LValue>,
    ) {
        self.named_block_binders
            .add(block_name, binders)
            .expect("ICE reused block name");
    }

    pub fn record_named_block_type(&mut self, block_name: H::BlockLabel, ty: H::Type) {
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

    pub fn lookup_named_block_type(&mut self, block_name: &H::BlockLabel) -> Option<H::Type> {
        self.named_block_types.get(block_name).cloned()
    }

    pub fn is_struct(&self, module: &ModuleIdent, datatype_name: &DatatypeName) -> bool {
        self.structs
            .get(module)
            .map(|structs| structs.contains_key(datatype_name))
            .unwrap_or(false)
    }

    pub fn struct_fields(
        &self,
        module: &ModuleIdent,
        struct_name: &DatatypeName,
    ) -> Option<&UniqueMap<Field, usize>> {
        let fields = self
            .structs
            .get(module)
            .and_then(|structs| structs.get(struct_name));
        // if fields are none, the struct must be defined in another module,
        // in that case, there should be errors
        assert!(fields.is_some() || self.env.has_errors());
        fields
    }

    /// Returns the enum variant names in sorted order.
    pub fn enum_variants(
        &self,
        module: &ModuleIdent,
        enum_name: &DatatypeName,
    ) -> Vec<VariantName> {
        self.enum_variants
            .get(module)
            .and_then(|enums| enums.get(enum_name))
            .expect("ICE enum resolution should have failed during naming")
            .to_vec()
    }

    pub fn enum_variant_fields(
        &self,
        module: &ModuleIdent,
        enum_name: &DatatypeName,
        variant_name: &VariantName,
    ) -> Option<&UniqueMap<Field, usize>> {
        let fields = self
            .variant_fields
            .get(module)
            .and_then(|enums| enums.get(enum_name))
            .and_then(|variants| variants.get(variant_name));
        // if fields are none, the variant must be defined in another module,
        // in that case, there should be errors
        assert!(fields.is_some() || self.env.has_errors());
        fields
    }

    pub fn make_imm_ref_match_binders(
        &mut self,
        pattern_loc: Loc,
        arg_types: Fields<N::Type>,
    ) -> Vec<(Field, N::Var, N::Type)> {
        let fields = match_compilation::order_fields_by_decl(None, arg_types.clone());
        fields
            .into_iter()
            .map(|(_, field_name, field_type)| {
                (
                    field_name,
                    self.new_match_var(field_name.to_string(), pattern_loc),
                    make_imm_ref_ty(field_type),
                )
            })
            .collect::<Vec<_>>()
    }

    pub fn make_unpack_binders(
        &mut self,
        pattern_loc: Loc,
        arg_types: Fields<N::Type>,
    ) -> Vec<(Field, N::Var, N::Type)> {
        let fields = match_compilation::order_fields_by_decl(None, arg_types.clone());
        fields
            .into_iter()
            .map(|(_, field_name, field_type)| {
                (
                    field_name,
                    self.new_match_var(field_name.to_string(), pattern_loc),
                    field_type,
                )
            })
            .collect::<Vec<_>>()
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

//**************************************************************************************************
// Entry
//**************************************************************************************************

pub fn program(
    compilation_env: &mut CompilationEnv,
    pre_compiled_lib: Option<&FullyCompiledProgram>,
    prog: T::Program,
) -> H::Program {
    detect_dead_code_analysis(compilation_env, &prog);

    let mut context = Context::new(compilation_env, pre_compiled_lib, &prog.inner);
    let T::Program_ { modules: tmodules } = prog.inner;
    let modules = modules(&mut context, tmodules);

    H::Program { modules }
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
        loc: _,
        warning_filter,
        package_name,
        attributes,
        is_source_module,
        dependency_order,
        immediate_neighbors: _,
        used_addresses: _,
        friends,
        structs: tstructs,
        enums: tenums,
        functions: tfunctions,
        constants: tconstants,
    } = mdef;
    context.current_package = package_name;
    context.env.add_warning_filter_scope(warning_filter.clone());
    let structs = tstructs.map(|name, s| struct_def(context, name, s));
    let enums = tenums.map(|name, s| enum_def(context, name, s));

    let constants = tconstants.map(|name, c| constant(context, name, c));
    let functions = tfunctions.map(|name, f| function(context, name, f));

    gen_unused_warnings(context, is_source_module, &structs);

    context.current_package = None;
    context.env.pop_warning_filter_scope();
    (
        module_ident,
        H::ModuleDefinition {
            warning_filter,
            package_name,
            attributes,
            is_source_module,
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
        warning_filter,
        index,
        attributes,
        visibility: evisibility,
        entry,
        signature,
        body,
    } = f;
    // println!("---------- fn: {}", _name);
    context.env.add_warning_filter_scope(warning_filter.clone());
    let signature = function_signature(context, signature);
    let body = function_body(context, &signature, body);
    context.env.pop_warning_filter_scope();
    H::Function {
        warning_filter,
        index,
        attributes,
        visibility: visibility(evisibility),
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
        .map(|(_, v, tty)| {
            let ty = single_type(context, tty);
            context.bind_local(v, ty.clone());
            (translate_var(v), ty)
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
    sp!(loc, tb_): T::FunctionBody,
) -> H::FunctionBody {
    use H::FunctionBody_ as HB;
    use T::FunctionBody_ as TB;
    let b_ = match tb_ {
        TB::Native => {
            context.extract_function_locals();
            HB::Native
        }
        TB::Defined(seq) => {
            // seq.print_verbose();
            let (locals, body) = function_body_defined(context, sig, loc, seq);
            // println!("----------------------");
            // body.print_verbose();
            HB::Defined { locals, body }
        }
    };
    sp(loc, b_)
}

fn function_body_defined(
    context: &mut Context,
    signature: &H::FunctionSignature,
    loc: Loc,
    seq: T::Sequence,
) -> (UniqueMap<H::Var, H::SingleType>, Block) {
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
        warning_filter,
        index,
        attributes,
        loc,
        signature: tsignature,
        value: tvalue,
    } = cdef;
    context.env.add_warning_filter_scope(warning_filter.clone());
    let signature = base_type(context, tsignature);
    let eloc = tvalue.exp.loc;
    let tseq = {
        let mut v = T::Sequence::new();
        v.push_back(sp(eloc, T::SequenceItem_::Seq(Box::new(tvalue))));
        v
    };
    let function_signature = H::FunctionSignature {
        type_parameters: vec![],
        parameters: vec![],
        return_type: H::Type_::base(signature.clone()),
    };
    let (locals, body) = function_body_defined(context, &function_signature, loc, tseq);
    context.env.pop_warning_filter_scope();
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
        warning_filter,
        index,
        attributes,
        abilities,
        type_parameters,
        fields,
    } = sdef;
    context.env.add_warning_filter_scope(warning_filter.clone());
    let fields = struct_fields(context, fields);
    context.env.pop_warning_filter_scope();
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
        N::StructFields::Defined(m) => m,
    };
    let mut indexed_fields = tfields_map
        .into_iter()
        .map(|(f, (idx, t))| (idx, (f, base_type(context, t))))
        .collect::<Vec<_>>();
    indexed_fields.sort_by(|(idx1, _), (idx2, _)| idx1.cmp(idx2));
    H::StructFields::Defined(indexed_fields.into_iter().map(|(_, f_ty)| f_ty).collect())
}

//**************************************************************************************************
// Structs
//**************************************************************************************************

fn enum_def(
    context: &mut Context,
    _name: DatatypeName,
    edef: N::EnumDefinition,
) -> H::EnumDefinition {
    let N::EnumDefinition {
        warning_filter,
        index,
        attributes,
        abilities,
        type_parameters,
        variants,
    } = edef;
    context.env.add_warning_filter_scope(warning_filter.clone());
    let variants = variants.map(|_, defn| H::VariantDefinition {
        index: defn.index,
        loc: defn.loc,
        fields: variant_fields(context, defn.fields),
    });
    context.env.pop_warning_filter_scope();
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
        N::VariantFields::Defined(m) => m,
    };
    let mut indexed_fields = tfields_map
        .into_iter()
        .map(|(f, (idx, t))| (idx, (f, base_type(context, t))))
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
    context: &Context,
    tys: impl IntoIterator<Item = N::Type>,
) -> R {
    tys.into_iter().map(|t| base_type(context, t)).collect()
}

fn base_type(context: &Context, sp!(loc, nb_): N::Type) -> H::BaseType {
    use H::BaseType_ as HB;
    use N::Type_ as NT;
    let b_ = match nb_ {
        NT::Var(_) => panic!(
            "ICE tvar not expanded: {}:{}-{}",
            loc.file_hash(),
            loc.start(),
            loc.end()
        ),
        NT::Apply(None, n, tys) => {
            NT::Apply(None, n, tys).print_verbose();
            panic!("ICE kind not expanded: {:#?}", loc)
        }
        NT::Apply(Some(k), n, nbs) => HB::Apply(k, type_name(context, n), base_types(context, nbs)),
        NT::Param(tp) => HB::Param(tp),
        NT::UnresolvedError => HB::UnresolvedError,
        NT::Anything => HB::Unreachable,
        NT::Ref(_, _) | NT::Unit => {
            println!("found ref type:");
            nb_.print_verbose();
            panic!(
                "ICE type constraints failed {}:{}-{}",
                loc.file_hash(),
                loc.start(),
                loc.end()
            )
        }
    };
    sp(loc, b_)
}

fn expected_types(context: &Context, loc: Loc, nss: Vec<Option<N::Type>>) -> H::Type {
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

fn single_types(context: &Context, ss: Vec<N::Type>) -> Vec<H::SingleType> {
    ss.into_iter().map(|s| single_type(context, s)).collect()
}

fn single_type(context: &Context, sp!(loc, ty_): N::Type) -> H::SingleType {
    use H::SingleType_ as HS;
    use N::Type_ as NT;
    let s_ = match ty_ {
        NT::Ref(mut_, nb) => HS::Ref(mut_, base_type(context, *nb)),
        _ => HS::Base(base_type(context, sp(loc, ty_))),
    };
    sp(loc, s_)
}

fn type_(context: &Context, sp!(loc, ty_): N::Type) -> H::Type {
    use H::Type_ as HT;
    use N::{TypeName_ as TN, Type_ as NT};
    let t_ = match ty_ {
        NT::Unit => HT::Unit,
        NT::Apply(None, n, tys) => {
            NT::Apply(None, n, tys).print_verbose();
            panic!("ICE kind not expanded: {:#?}", loc)
        }
        NT::Apply(Some(_), sp!(_, TN::Multiple(_)), ss) => HT::Multiple(single_types(context, ss)),
        _ => HT::Single(single_type(context, sp(loc, ty_))),
    };
    sp(loc, t_)
}

//**************************************************************************************************
// Expression Processing
//**************************************************************************************************

macro_rules! make_block {
    () => { VecDeque::new() };
    ($($elems:expr),+) => { VecDeque::from([$($elems),*]) };
}

// fn match_subject(context: &mut Context, block: &mut Block, subject: T::Exp) -> Box<H::Exp> {
//     let eloc = subject.exp.loc;
//     let out_type = type_(context, subject.ty.clone());
//     let exp = value(context, block, Some(&out_type), subject);
//     let bound_exp = bind_exp(context, block, exp);
//     let tmp = match bound_exp.exp.value {
//         H::UnannotatedExp_::Move {
//             annotation: MoveOpAnnotation::InferredLastUsage,
//             var,
//         } => var,
//         _ => panic!("ICE invalid bind_exp for single value"),
//     };
//     Box::new(H::exp(
//         out_type,
//         sp(eloc, H::UnannotatedExp_::BorrowLocal(false, tmp)),
//     ))
// }

// -------------------------------------------------------------------------------------------------
// Tail Position
// -------------------------------------------------------------------------------------------------

fn body(
    context: &mut Context,
    expected_type: Option<&H::Type>,
    loc: Loc,
    seq: T::Sequence,
) -> (Block, Option<H::Exp>) {
    if seq.is_empty() {
        (make_block!(), Some(unit_exp(loc)))
    } else {
        let mut block = make_block!();
        let final_exp = tail_block(context, &mut block, expected_type, seq);
        (block, final_exp)
    }
}

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
        E::IfElse(test, conseq, alt) => {
            let cond = value(context, block, Some(&tbool(eloc)), *test);
            let mut if_block = make_block!();
            let conseq_exp = tail(context, &mut if_block, Some(&out_type), *conseq);
            let mut else_block = make_block!();
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
            // println!("compiling match!");
            // print!("subject:");
            // subject.print_verbose();
            // println!("\narms:");
            // for arm in &arms.value {
            //     arm.value.print_verbose();
            // }
            let compiled = match_compilation::compile_match(context, in_type, *subject, arms);
            // println!("-----\ncompiled:");
            // compiled.print();
            // let result = tail(context, block, expected_type, compiled);
            // println!("-----\nblock:");
            // block.print();
            // print!("result: ");
            // result.clone().unwrap().print_verbose();
            // result
            tail(context, block, expected_type, compiled)
        }

        E::VariantMatch(subject, enum_name, arms) => {
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
            context.record_named_block_binders(name, binders);
            context.record_named_block_type(name, out_type.clone());
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
        E::NamedBlock(name, seq) => {
            let name = translate_block_label(name);
            let (binders, bound_exp) = make_binders(context, eloc, out_type.clone());
            let result = if binders.is_empty() {
                // need to swap the implicit unit out for a trailing unit in tail position
                trailing_unit_exp(eloc)
            } else {
                maybe_freeze(context, block, expected_type.cloned(), bound_exp)
            };
            context.record_named_block_binders(name, binders.clone());
            context.record_named_block_type(name, out_type.clone());
            let mut body_block = make_block!();
            let final_exp = tail_block(context, &mut body_block, Some(&out_type), seq);
            final_exp.map(|exp| {
                bind_value_in_block(context, binders, Some(out_type), &mut body_block, exp);
                block.push_back(sp(
                    eloc,
                    S::NamedBlock {
                        name,
                        block: body_block,
                    },
                ));
                result
            })
        }
        E::Block(seq) => tail_block(context, block, expected_type, seq),

        // -----------------------------------------------------------------------------------------
        //  statements that need to be hoisted out
        // -----------------------------------------------------------------------------------------
        E::Return(_)
        | E::Abort(_)
        | E::Give(_, _)
        | E::Continue(_)
        | E::Assign(_, _, _)
        | E::Mutate(_, _) => panic!("ICE statement mishandled"),

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
    mut seq: T::Sequence,
) -> Option<H::Exp> {
    use T::SequenceItem_ as S;
    let last_exp = seq.pop_back();
    statement_block(context, block, seq);
    match last_exp {
        None => None,
        Some(sp!(_, S::Seq(last))) => tail(context, block, expected_type, *last),
        Some(_) => panic!("ICE last sequence item should be an exp"),
    }
}

// -------------------------------------------------------------------------------------------------
// Value Position
// -------------------------------------------------------------------------------------------------

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
        E::Builtin(bt, arguments) if matches!(&*bt, sp!(_, T::BuiltinFunction_::Assert(false))) => {
            use T::ExpListItem as TI;
            let [cond_item, code_item]: [TI; 2] = match arguments.exp.value {
                E::ExpList(arg_list) => arg_list.try_into().unwrap(),
                _ => panic!("ICE type checking failed"),
            };
            let (econd, ecode) = match (cond_item, code_item) {
                (TI::Single(econd, _), TI::Single(ecode, _)) => (econd, ecode),
                _ => panic!("ICE type checking failed"),
            };
            let cond_value = value(context, block, Some(&tbool(eloc)), econd);
            let code_value = value(context, block, None, ecode);
            let cond = bind_exp(context, block, cond_value);
            let code = bind_exp(context, block, code_value);
            let if_block = make_block!();
            let else_block = make_block!(make_command(eloc, C::Abort(code)));
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
        E::Builtin(bt, arguments) if matches!(&*bt, sp!(_, T::BuiltinFunction_::Assert(true))) => {
            use T::ExpListItem as TI;
            let [cond_item, code_item]: [TI; 2] = match arguments.exp.value {
                E::ExpList(arg_list) => arg_list.try_into().unwrap(),
                _ => panic!("ICE type checking failed"),
            };
            let (econd, ecode) = match (cond_item, code_item) {
                (TI::Single(econd, _), TI::Single(ecode, _)) => (econd, ecode),
                _ => panic!("ICE type checking failed"),
            };
            let cond = value(context, block, Some(&tbool(eloc)), econd);
            let mut else_block = make_block!();
            let code = value(context, &mut else_block, None, ecode);
            let if_block = make_block!();
            else_block.push_back(make_command(eloc, C::Abort(code)));
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
        E::IfElse(test, conseq, alt) => {
            let cond = value(context, block, Some(&tbool(eloc)), *test);
            let mut if_block = make_block!();
            let conseq_exp = value(context, &mut if_block, Some(&out_type), *conseq);
            let mut else_block = make_block!();
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
            // println!("compiling match!");
            // print!("subject:");
            // subject.print_verbose();
            // println!("\narms:");
            // for arm in &arms.value {
            //     arm.value.print_verbose();
            // }
            let compiled = match_compilation::compile_match(context, in_type, *subject, arms);
            // println!("-----\ncompiled:");
            // compiled.print_verbose();
            value(context, block, None, compiled)
        }

        E::VariantMatch(subject, enum_name, arms) => {
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
            if arms_unreachable {
                make_exp(HE::Unreachable)
            } else {
                bound_exp
            }
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
            context.record_named_block_binders(name, binders);
            context.record_named_block_type(name, out_type.clone());
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
                bound_exp
            } else {
                make_exp(HE::Unreachable)
            }
        }
        e_ @ E::Loop { .. } => {
            statement(context, block, T::exp(in_type.clone(), sp(eloc, e_)));
            make_exp(HE::Unreachable)
        }
        E::NamedBlock(name, seq) => {
            let name = translate_block_label(name);
            let (binders, bound_exp) = make_binders(context, eloc, out_type.clone());
            context.record_named_block_binders(name, binders.clone());
            context.record_named_block_type(name, out_type.clone());
            let mut body_block = make_block!();
            let final_exp = value_block(context, &mut body_block, Some(&out_type), seq);
            bind_value_in_block(context, binders, Some(out_type), &mut body_block, final_exp);
            block.push_back(sp(
                eloc,
                S::NamedBlock {
                    name,
                    block: body_block,
                },
            ));
            bound_exp
        }
        E::Block(seq) => value_block(context, block, Some(&out_type), seq),

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

            let decl_fields = context.struct_fields(&module_ident, &struct_name);

            let mut texp_fields: Vec<(usize, Field, usize, N::Type, T::Exp)> =
                if let Some(field_map) = decl_fields {
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
            // // all fields of a packed struct type are used
            // context
            //     .used_fields
            //     .entry(struct_name.value())
            //     .or_default()
            //     .extend(fields.iter().map(|(_, name, _)| *name));

            let base_types = base_types(context, arg_types);

            let decl_fields = context.enum_variant_fields(&module_ident, &enum_name, &variant_name);

            let mut texp_fields: Vec<(usize, Field, usize, N::Type, T::Exp)> =
                if let Some(field_map) = decl_fields {
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
                _ => panic!("ICE invalid bind_exp for single value"),
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
                _ => panic!("ICE typing failed for cast"),
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
        E::Value(ev) => make_exp(HE::Value(process_value(ev))),
        E::Constant(_m, c) => make_exp(HE::Constant(c)), // only private constants (for now)
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
        | E::Mutate(_, _) => panic!("ICE statement mishandled"),

        // -----------------------------------------------------------------------------------------
        // odds and ends -- things we need to deal with but that don't do much
        // -----------------------------------------------------------------------------------------
        E::Use(_) => panic!("ICE unexpanded use"),

        E::UnresolvedError => {
            assert!(context.env.has_errors());
            make_exp(HE::UnresolvedError)
        }
    };
    maybe_freeze(context, block, expected_type.cloned(), preresult)
}

fn value_block(
    context: &mut Context,
    block: &mut Block,
    expected_type: Option<&H::Type>,
    mut seq: T::Sequence,
) -> H::Exp {
    use T::SequenceItem_ as S;
    let last_exp = seq.pop_back();
    statement_block(context, block, seq);
    match last_exp {
        Some(sp!(_, S::Seq(last))) => value(context, block, expected_type, *last),
        _ => panic!("ICE last sequence item should be an exp"),
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
                T::ExpListItem::Splat(_, _, _) => panic!("ICE spalt is unsupported."),
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

// -------------------------------------------------------------------------------------------------
// Statement Position
// -------------------------------------------------------------------------------------------------

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
        E::IfElse(test, conseq, alt) => {
            let cond = value(context, block, Some(&tbool(eloc)), *test);
            let mut if_block = make_block!();
            statement(context, &mut if_block, *conseq);
            let mut else_block = make_block!();
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
            // println!("compiling match!");
            // print!("subject:");
            // subject.print_verbose();
            // println!("\narms:");
            // for arm in &arms.value {
            //     arm.value.print_verbose();
            // }
            let subject_type = subject.ty.clone();
            let compiled = match_compilation::compile_match(context, &subject_type, *subject, arms);
            // println!("-----\ncompiled:");
            // compiled.print_verbose();
            statement(context, block, compiled)
        }
        E::VariantMatch(subject, enum_name, arms) => {
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
        }
        E::While(test, name, body) => {
            let mut cond_block = make_block!();
            let cond_exp = value(context, &mut cond_block, Some(&tbool(eloc)), *test);
            let cond = (cond_block, Box::new(cond_exp));
            let name = translate_block_label(name);
            // While loops can still use break and continue so we build them dummy binders.
            context.record_named_block_binders(name, vec![]);
            context.record_named_block_type(name, tunit(eloc));
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
        }
        E::Loop { name, body, .. } => {
            let name = translate_block_label(name);
            let out_type = type_(context, ty.clone());
            let (binders, bound_exp) = make_binders(context, eloc, out_type.clone());
            context.record_named_block_binders(name, binders);
            context.record_named_block_type(name, out_type);
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
        }
        E::Block(seq) => statement_block(context, block, seq),
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
            block.push_back(make_command(eloc, C::Abort(exp)));
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
            make_assignments(context, block, eloc, assigns, exp);
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
        | E::Move { .. }
        | E::Copy { .. }
        | E::UnresolvedError
        | E::NamedBlock(_, _)) => value_statement(context, block, make_exp(e_)),

        E::Value(_) | E::Unit { .. } => (),

        // -----------------------------------------------------------------------------------------
        // odds and ends -- things we need to deal with but that don't do much
        // -----------------------------------------------------------------------------------------
        E::Use(_) => panic!("ICE unexpanded use"),
    }
}

fn statement_block(context: &mut Context, block: &mut Block, seq: T::Sequence) {
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
                make_assignments(context, block, sloc, bindings, rhs_exp);
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
            S::While { block, .. } => has_break_block(name, block),
            S::Loop { block, .. } => has_break_block(name, block),
            hcmd!(C::Break(break_name)) => break_name == name,
            _ => false,
        }
    }

    fn has_break_block(name: &BlockLabel, block: &Block) -> bool {
        block.iter().any(|stmt| has_break(name, stmt))
    }

    has_break_block(name, block)
}

pub fn make_imm_ref_ty(ty: N::Type) -> N::Type {
    match ty {
        sp!(_, N::Type_::Ref(false, _)) => ty,
        sp!(loc, N::Type_::Ref(true, inner)) => sp(loc, N::Type_::Ref(false, inner)),
        ty => {
            let loc = ty.loc;
            sp(loc, N::Type_::Ref(false, Box::new(ty)))
        }
    }
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
        L::Var { var: v, ty, .. } => {
            let st = single_type(context, *ty.clone());
            context.bind_local(*v, st)
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
    sp!(_, assigns): T::LValueList,
    rvalue: H::Exp,
) {
    use H::{Command_ as C, Statement_ as S};
    let mut lvalues = vec![];
    let mut after = Block::new();
    for (idx, a) in assigns.into_iter().enumerate() {
        let a_ty = rvalue.ty.value.type_at_index(idx);
        let (ls, mut af) = assign(context, a, a_ty);

        lvalues.push(ls);
        after.append(&mut af);
    }
    result.push_back(sp(loc, S::Command(sp(loc, C::Assign(lvalues, rvalue)))));
    result.append(&mut after);
}

fn assign(
    context: &mut Context,
    sp!(loc, ta_): T::LValue,
    rvalue_ty: &H::SingleType,
) -> (H::LValue, Block) {
    use H::{LValue_ as L, UnannotatedExp_ as E};
    use T::LValue_ as A;
    let mut after = Block::new();
    let l_ = match ta_ {
        A::Ignore => L::Ignore,
        A::Var { var: v, ty: st, .. } => {
            L::Var(translate_var(v), Box::new(single_type(context, *st)))
        }
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
                let (fa, mut fafter) = assign(context, tfa, st);
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
                make_assignments(context, &mut after, floc, sp(floc, vec![tfa]), borrow);
            }
            L::Var(tmp, Box::new(rvalue_ty.clone()))
        }
        A::UnpackVariant(m, e, v, tbs, tfields) => {
            // all fields of an unpacked struct type are used
            // context
            //     .used_fields
            //     .entry(e.value())
            //     .or_default()
            //     .extend(tfields.iter().map(|(_, s, _)| *s));

            let bs = base_types(context, tbs);

            let mut fields = vec![];
            for (decl_idx, f, st, tfa) in assign_variant_fields(context, &m, &e, &v, tfields) {
                assert!(fields.len() == decl_idx);
                assert!(!matches!(&st, sp!(_, H::SingleType_::Ref(_, _))));
                let (fa, mut fafter) = assign(context, tfa, &st);
                after.append(&mut fafter);
                fields.push((f, fa))
            }
            L::UnpackVariant(e, v, UnpackType::ByValue, loc, bs, fields)
        }
        A::BorrowUnpackVariant(mut_, m, e, v, tbs, tfields) => {
            // all fields of an unpacked struct type are used
            // context
            //     .used_fields
            //     .entry(e.value())
            //     .or_default()
            //     .extend(tfields.iter().map(|(_, s, _)| *s));

            let bs = base_types(context, tbs);

            let unpack = if mut_ {
                UnpackType::ByMutRef
            } else {
                UnpackType::ByImmRef
            };

            let mut fields = vec![];
            for (decl_idx, f, st, tfa) in assign_variant_fields(context, &m, &e, &v, tfields) {
                assert!(fields.len() == decl_idx);
                assert!(matches!(&st, sp!(_, H::SingleType_::Ref(st_mut, _)) if st_mut == &mut_));
                let (fa, mut fafter) = assign(context, tfa, &st);
                after.append(&mut fafter);
                fields.push((f, fa))
            }
            L::UnpackVariant(e, v, unpack, loc, bs, fields)
        }
    };
    (sp(loc, l_), after)
}

fn assign_struct_fields(
    context: &Context,
    m: &ModuleIdent,
    s: &DatatypeName,
    tfields: Fields<(N::Type, T::LValue)>,
) -> Vec<(usize, Field, H::BaseType, T::LValue)> {
    let decl_fields = context.struct_fields(m, s);
    let mut count = 0;
    let mut decl_field = |f: &Field| -> usize {
        match decl_fields {
            Some(m) => *m.get(f).unwrap(),
            None => {
                // none can occur with errors in typing
                let i = count;
                count += 1;
                i
            }
        }
    };
    let mut tfields_vec = tfields
        .into_iter()
        .map(|(f, (_idx, (tbt, tfa)))| (decl_field(&f), f, base_type(context, tbt), tfa))
        .collect::<Vec<_>>();
    tfields_vec.sort_by(|(idx1, _, _, _), (idx2, _, _, _)| idx1.cmp(idx2));
    tfields_vec
}

fn assign_variant_fields(
    context: &Context,
    m: &ModuleIdent,
    e: &DatatypeName,
    v: &VariantName,
    tfields: Fields<(N::Type, T::LValue)>,
) -> Vec<(usize, Field, H::SingleType, T::LValue)> {
    let decl_fields = context.enum_variant_fields(m, e, v);
    let mut count = 0;
    let mut decl_field = |f: &Field| -> usize {
        match decl_fields {
            Some(m) => *m.get(f).unwrap(),
            None => {
                // none can occur with errors in typing
                let i = count;
                count += 1;
                i
            }
        }
    };
    let mut tfields_vec = tfields
        .into_iter()
        .map(|(f, (_idx, (tbt, tfa)))| (decl_field(&f), f, single_type(context, tbt), tfa))
        .collect::<Vec<_>>();
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
    for sp!(_, lvalue) in &binders {
        match lvalue {
            H::LValue_::Var(_, _) => (),
            _ => panic!("ICE tried bind_value for non-var lvalue"),
        }
    }
    let rhs_exp = maybe_freeze(context, stmts, binders_type, value_exp);
    let loc = rhs_exp.exp.loc;
    stmts.push_back(sp(loc, S::Command(sp(loc, C::Assign(binders, rhs_exp)))));
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
    let lvalue = sp(loc, H::LValue_::Var(binder, Box::new(sp(loc, ty.clone()))));
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

fn process_value(sp!(loc, ev_): E::Value) -> H::Value {
    use E::Value_ as EV;
    use H::Value_ as HV;
    let v_ = match ev_ {
        EV::InferredNum(_) => panic!("ICE should have been expanded"),
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

fn process_binops(
    context: &mut Context,
    input_block: &mut Block,
    result_type: H::Type,
    e: T::Exp,
) -> H::Exp {
    use T::UnannotatedExp_ as E;
    let (mut block, exp) = process_binops!(
        (BinOp, H::Type, Loc),
        (Block, H::Exp),
        (e, result_type),
        (exp, ty),
        exp,
        T::Exp {
            exp: sp!(eloc, E::BinopExp(lhs, op, op_type, rhs)),
            ..
        } =>
        {
            let op = (op, ty, eloc);
            let op_type = freeze_ty(type_(context, *op_type));
            let rhs = (*rhs, op_type.clone());
            let lhs = (*lhs, op_type);
            (lhs, op, rhs)
        },
        {
            let mut exp_block = make_block!();
            let exp = value(context, &mut exp_block, Some(ty).as_ref(), exp);
            (exp_block, exp)
        },
        value_stack,
        (op, ty, eloc) =>
        {
            match op {
                sp!(loc, op @ BinOp_::And) => {
                    let test = value_stack.pop().expect("ICE binop hlir issue");
                    let if_ = value_stack.pop().expect("ICE binop hlir issue");
                    if simple_bool_binop_arg(&if_) {
                        let (mut test_block, test_exp) = test;
                        let (mut if_block, if_exp) = if_;
                        test_block.append(&mut if_block);
                        let exp = H::exp(ty, sp(eloc, make_binop(test_exp, sp(loc, op), if_exp)));
                        (test_block, exp)
                    } else {
                        let else_ = (make_block!(), bool_exp(loc, false));
                        make_boolean_binop(
                            context,
                            sp(loc, op),
                            test,
                            if_,
                            else_,
                        )
                    }
                }
                sp!(loc, op @ BinOp_::Or) => {
                    let test = value_stack.pop().expect("ICE binop hlir issue");
                    let else_ = value_stack.pop().expect("ICE binop hlir issue");
                    if simple_bool_binop_arg(&else_) {
                        let (mut test_block, test_exp) = test;
                        let (mut else_block, else_exp) = else_;
                        test_block.append(&mut else_block);
                        let exp = H::exp(ty, sp(eloc, make_binop(test_exp, sp(loc, op), else_exp)));
                        (test_block, exp)
                    } else {
                        let if_ = (make_block!(), bool_exp(loc, true));
                        make_boolean_binop(
                            context,
                            sp(loc, op),
                            test,
                            if_,
                            else_,
                        )
                    }
                }
                op => {
                    let (mut lhs_block, lhs_exp) = value_stack.pop().expect("ICE binop hlir issue");
                    let (mut rhs_block, rhs_exp) = value_stack.pop().expect("ICE binop hlir issue");
                    lhs_block.append(&mut rhs_block);
                    // NB: here we could check if the LHS and RHS are "large" terms and let-bind
                    // them if they are getting too big.
                    let exp = H::exp(ty, sp(eloc, make_binop(lhs_exp, op, rhs_exp)));
                    (lhs_block, exp)
                }
            }
        }
    );
    input_block.append(&mut block);
    exp
}

fn make_binop(lhs: H::Exp, op: BinOp, rhs: H::Exp) -> H::UnannotatedExp_ {
    H::UnannotatedExp_::BinopExp(Box::new(lhs), op, Box::new(rhs))
}

fn make_boolean_binop(
    context: &mut Context,
    op: BinOp,
    (mut test_block, test_exp): (Block, H::Exp),
    (mut if_block, if_exp): (Block, H::Exp),
    (mut else_block, else_exp): (Block, H::Exp),
) -> (Block, H::Exp) {
    let loc = op.loc;

    let bool_ty = tbool(loc);
    let (binders, bound_exp) = make_binders(context, loc, bool_ty.clone());
    let opty = Some(bool_ty);

    let arms_unreachable = if_exp.is_unreachable() && else_exp.is_unreachable();
    // one of these _must_ always bind by construction.
    bind_value_in_block(
        context,
        binders.clone(),
        opty.clone(),
        &mut if_block,
        if_exp,
    );
    bind_value_in_block(context, binders, opty, &mut else_block, else_exp);
    assert!(!arms_unreachable, "ICE boolean binop processing failure");

    let if_else = H::Statement_::IfElse {
        cond: Box::new(test_exp),
        if_block,
        else_block,
    };
    test_block.push_back(sp(loc, if_else));
    (test_block, bound_exp)
}

fn simple_bool_binop_arg((block, exp): &(Block, H::Exp)) -> bool {
    use H::UnannotatedExp_ as HE;
    if !block.is_empty() {
        false
    } else {
        matches!(
            exp.exp.value,
            HE::Value(_)
                | HE::Constant(_)
                | HE::Move { .. }
                | HE::Copy { .. }
                | HE::UnresolvedError
        )
    }
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

fn needs_freeze(context: &Context, sp!(_, actual): &H::Type, sp!(_, expected): &H::Type) -> Freeze {
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
        (_actual, _expected) => {
            assert!(context.env.has_errors());
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
                        _ => panic!("ICE list item has Multple type"),
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
    is_source_module: bool,
    structs: &UniqueMap<DatatypeName, H::StructDefinition>,
) {
    if !is_source_module {
        // generate warnings only for modules compiled in this pass rather than for all modules
        // including pre-compiled libraries for which we do not have source code available and
        // cannot be analyzed in this pass
        return;
    }
    let is_sui_mode = context.env.package_config(context.current_package).flavor == Flavor::Sui;

    for (_, sname, sdef) in structs {
        context
            .env
            .add_warning_filter_scope(sdef.warning_filter.clone());

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
                    context
                        .env
                        .add_diag(diag!(UnusedItem::StructField, (f.loc(), msg)));
                }
            }
        }

        context.env.pop_warning_filter_scope();
    }
}
