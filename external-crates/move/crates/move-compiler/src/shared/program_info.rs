// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt::Display, sync::Arc, sync::OnceLock};

use self::known_attributes::AttributePosition;
use crate::{
    PreCompiledProgramInfo,
    expansion::ast::{AbilitySet, Attributes, ModuleIdent, Visibility},
    naming::ast::{
        self as N, BuiltinTypeName_, DatatypeTypeParameter, EnumDefinition, FunctionSignature,
        ResolvedUseFuns, StructDefinition, StructFields, SyntaxMethods, Type, Type_, TypeName_,
        VariantFields,
    },
    parser::ast::{
        ConstantName, DatatypeName, DocComment, Field, FunctionName, TargetKind, VariantName,
    },
    shared::{unique_map::UniqueMap, *},
    sui_mode::info::{SuiInfo, SuiModInfo},
    typing::ast::{self as T},
};
use move_core_types::runtime_value;
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub doc: DocComment,
    pub index: usize,
    pub attributes: Attributes,
    pub defined_loc: Loc,
    pub full_loc: Loc,
    pub visibility: Visibility,
    pub entry: Option<Loc>,
    pub macro_: Option<Loc>,
    pub signature: FunctionSignature,
}

#[derive(Debug, Clone)]
pub struct ConstantInfo {
    pub doc: DocComment,
    pub index: usize,
    pub attributes: Attributes,
    pub defined_loc: Loc,
    pub signature: Type,
    // Set after compilation
    pub value: OnceLock<runtime_value::MoveValue>,
}

#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub doc: DocComment,
    pub defined_loc: Loc,
    pub target_kind: TargetKind,
    pub attributes: Attributes,
    pub package: Option<Symbol>,
    /// The named address map used by this module during `expansion`.
    pub named_address_map: Arc<NamedAddressMap>,
    pub dependency_order: Option<usize>,
    pub use_funs: ResolvedUseFuns,
    pub syntax_methods: SyntaxMethods,
    pub friends: UniqueMap<ModuleIdent, Loc>,
    pub structs: UniqueMap<DatatypeName, StructDefinition>,
    pub enums: UniqueMap<DatatypeName, EnumDefinition>,
    pub functions: UniqueMap<FunctionName, FunctionInfo>,
    pub constants: UniqueMap<ConstantName, ConstantInfo>,
    pub sui_info: Option<SuiModInfo>,
}

#[derive(Debug, Clone)]
pub struct ProgramInfo<const AFTER_TYPING: bool> {
    pub modules: UniqueMap<ModuleIdent, ModuleInfo>,
}
pub type NamingProgramInfo = ProgramInfo<false>;
pub type TypingProgramInfo = ProgramInfo<true>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatatypeKind {
    Struct,
    Enum,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamedMemberKind {
    Struct,
    Enum,
    Function,
    Constant,
}

macro_rules! program_info {
    ($pre_compiled_lib:ident, $converter:expr, $prog:ident, $module_use_funs:ident, $dependency_order:expr) => {{
        let all_modules = $prog.modules.key_cloned_iter();
        let mut modules = UniqueMap::maybe_from_iter(all_modules.map(|(mident, mdef)| {
            let structs = mdef.structs.clone();
            let enums = mdef.enums.clone();
            let functions = mdef.functions.ref_map(|fname, fdef| FunctionInfo {
                doc: fdef.doc.clone(),
                index: fdef.index,
                attributes: fdef.attributes.clone(),
                defined_loc: fname.loc(),
                full_loc: fdef.loc,
                visibility: fdef.visibility.clone(),
                entry: fdef.entry,
                macro_: fdef.macro_,
                signature: fdef.signature.clone(),
            });
            let constants = mdef.constants.ref_map(|cname, cdef| ConstantInfo {
                doc: cdef.doc.clone(),
                index: cdef.index,
                attributes: cdef.attributes.clone(),
                defined_loc: cname.loc(),
                signature: cdef.signature.clone(),
                value: OnceLock::new(),
            });
            let use_funs = $module_use_funs
                .as_mut()
                .map(|module_use_funs| module_use_funs.remove(&mident).unwrap())
                .unwrap_or_default();
            let minfo = ModuleInfo {
                doc: mdef.doc.clone(),
                defined_loc: mdef.loc,
                target_kind: mdef.target_kind,
                attributes: mdef.attributes.clone(),
                package: mdef.package_name,
                named_address_map: mdef.named_address_map.clone(),
                dependency_order: $dependency_order(&mdef),
                use_funs,
                syntax_methods: mdef.syntax_methods.clone(),
                friends: mdef.friends.ref_map(|_, friend| friend.loc),
                structs,
                enums,
                functions,
                constants,
                sui_info: None,
            };
            (mident, minfo)
        }))
        .unwrap();

        if let Some(pre_compiled_lib) = $pre_compiled_lib {
            for (mident, minfo) in pre_compiled_lib.iter() {
                if !modules.contains_key(&mident) {
                    modules
                        .add(mident.clone(), $converter(&minfo.info))
                        .unwrap();
                }
            }
        }
        ProgramInfo { modules }
    }};
}

impl TypingProgramInfo {
    pub fn new(
        env: &CompilationEnv,
        pre_compiled_lib: Option<Arc<PreCompiledProgramInfo>>,
        modules: &UniqueMap<ModuleIdent, T::ModuleDefinition>,
        mut module_use_funs: BTreeMap<ModuleIdent, ResolvedUseFuns>,
    ) -> Self {
        /// Identity function for cloning typing module info to be used in program_info macro
        /// to create typing program info from pre-compiled module info.
        fn typing_module_info_clone(minfo: &ModuleInfo) -> ModuleInfo {
            minfo.clone()
        }
        /// Used to populate `dependency_order` field in `ModuleInfo`
        fn typing_dependency_order(mdef: &T::ModuleDefinition) -> Option<usize> {
            Some(mdef.dependency_order)
        }
        struct Prog<'a> {
            modules: &'a UniqueMap<ModuleIdent, T::ModuleDefinition>,
        }
        let mut module_use_funs = Some(&mut module_use_funs);
        let prog = Prog { modules };
        let pcmi = pre_compiled_lib.clone();
        let mut info = program_info!(
            pcmi,
            typing_module_info_clone,
            prog,
            module_use_funs,
            typing_dependency_order
        );
        // TODO we should really have an idea of root package flavor here
        // but this feels roughly equivalent
        if env
            .package_configs()
            .any(|(_, config)| config.flavor == Flavor::Sui)
        {
            let mut sui_flavor_info = SuiInfo::new(modules, &info);
            for (mident, module_info) in info.modules.key_cloned_iter_mut() {
                let uid_holders = sui_flavor_info
                    .uid_holders
                    .remove(&mident)
                    .unwrap_or_default();
                let transferred = sui_flavor_info
                    .transferred
                    .remove(&mident)
                    .unwrap_or_default();
                module_info.sui_info = Some(SuiModInfo {
                    uid_holders,
                    transferred,
                });
            }
        }
        info
    }
}

/// Re-create naming module info from typing module info to be used in program_info macro
/// to create naming program info from pre-compiled module info.
fn typing_module_info_to_naming(minfo: &ModuleInfo) -> ModuleInfo {
    // Typing ProgramInfo contains abilities that would not yet be inferred at naming
    // (for user-defined data types and vector element types). We need to strip these
    // down so that ProgramInfo does not trip subsequent typing analysis.
    fn strip_type_abilities(ty: &mut Type) {
        match &mut ty.value {
            Type_::Apply(abilities, type_name, type_args) => {
                let should_strip = matches!(
                    &type_name.value,
                    TypeName_::Builtin(sp!(_, BuiltinTypeName_::Vector))
                        | TypeName_::ModuleType(_, _)
                        | TypeName_::Multiple(_)
                );
                if should_strip {
                    *abilities = None;
                }
                // Recursively strip abilities from type arguments
                for ty_arg in type_args {
                    strip_type_abilities(ty_arg);
                }
            }
            Type_::Ref(_, ty) => strip_type_abilities(ty),
            Type_::Fun(args, result) => {
                for arg in args {
                    strip_type_abilities(arg);
                }
                strip_type_abilities(result);
            }
            Type_::Unit
            | Type_::Param(_)
            | Type_::Var(_)
            | Type_::Anything
            | Type_::Void
            | Type_::UnresolvedError => (),
        }
    }

    let mut minfo = minfo.clone();

    // update structs
    for (_, _, sdef) in minfo.structs.iter_mut() {
        if let StructFields::Defined(_, fields) = &mut sdef.fields {
            for (_, _, (_, (_, ty))) in fields.iter_mut() {
                strip_type_abilities(ty);
            }
        }
    }

    // update enums
    for (_, _, edef) in minfo.enums.iter_mut() {
        for (_, _, vdef) in edef.variants.iter_mut() {
            if let VariantFields::Defined(_, fields) = &mut vdef.fields {
                for (_, _, (_, (_, ty))) in fields.iter_mut() {
                    strip_type_abilities(ty);
                }
            }
        }
    }

    // update constants
    for (_, _, cdef) in minfo.constants.iter_mut() {
        strip_type_abilities(&mut cdef.signature);
    }

    // update functions
    for (_, _, fdef) in minfo.functions.iter_mut() {
        for (_, _, ty) in fdef.signature.parameters.iter_mut() {
            strip_type_abilities(ty);
        }
        strip_type_abilities(&mut fdef.signature.return_type);
    }

    minfo
}

impl NamingProgramInfo {
    pub fn new(pre_compiled_lib: Option<Arc<PreCompiledProgramInfo>>, prog: &N::Program_) -> Self {
        /// Used to populate `dependency_order` field in `ModuleInfo`
        fn dependency_order(_mdef: &N::ModuleDefinition) -> Option<usize> {
            // No dependency order available in the naming pass
            None
        }
        // use_funs will be populated later
        let mut module_use_funs: Option<&mut BTreeMap<ModuleIdent, ResolvedUseFuns>> = None;
        program_info!(
            pre_compiled_lib,
            typing_module_info_to_naming,
            prog,
            module_use_funs,
            dependency_order
        )
    }

    pub fn set_use_funs(&mut self, module_use_funs: BTreeMap<ModuleIdent, ResolvedUseFuns>) {
        for (mident, use_funs) in module_use_funs {
            let use_funs_ref = &mut self.modules.get_mut(&mident).unwrap().use_funs;
            assert!(use_funs_ref.is_empty());
            *use_funs_ref = use_funs;
        }
    }

    pub fn take_use_funs(self) -> BTreeMap<ModuleIdent, ResolvedUseFuns> {
        self.modules
            .into_iter()
            .map(|(mident, minfo)| (mident, minfo.use_funs))
            .collect()
    }

    pub fn set_module_syntax_methods(
        &mut self,
        mident: ModuleIdent,
        syntax_methods: SyntaxMethods,
    ) {
        let syntax_methods_ref = &mut self.modules.get_mut(&mident).unwrap().syntax_methods;
        *syntax_methods_ref = syntax_methods;
    }
}

impl<const AFTER_TYPING: bool> ProgramInfo<AFTER_TYPING> {
    pub fn module(&self, m: &ModuleIdent) -> &ModuleInfo {
        self.module_opt(m)
            .expect("ICE should have failed in naming")
    }

    pub fn module_opt(&self, m: &ModuleIdent) -> Option<&ModuleInfo> {
        self.modules.get(m)
    }

    pub fn named_member_kind(&self, m: ModuleIdent, n: Name) -> NamedMemberKind {
        let minfo = self.module(&m);
        if minfo.structs.contains_key(&DatatypeName(n)) {
            NamedMemberKind::Struct
        } else if minfo.enums.contains_key(&DatatypeName(n)) {
            NamedMemberKind::Enum
        } else if minfo.functions.contains_key(&FunctionName(n)) {
            NamedMemberKind::Function
        } else if minfo.constants.contains_key(&ConstantName(n)) {
            NamedMemberKind::Constant
        } else {
            panic!("ICE should have failed in naming")
        }
    }

    pub fn function_info(&self, m: &ModuleIdent, n: &FunctionName) -> &FunctionInfo {
        self.function_info_opt(m, n)
            .expect("ICE should have failed in naming")
    }

    pub fn function_info_opt(&self, m: &ModuleIdent, n: &FunctionName) -> Option<&FunctionInfo> {
        self.module_opt(m)?.functions.get(n)
    }

    pub fn constant_info(&self, m: &ModuleIdent, n: &ConstantName) -> &ConstantInfo {
        self.constant_info_opt(m, n)
            .expect("ICE should have failed in naming")
    }

    pub fn constant_info_opt(&self, m: &ModuleIdent, n: &ConstantName) -> Option<&ConstantInfo> {
        self.module_opt(m)?.constants.get(n)
    }

    pub fn datatype_kind(&self, m: &ModuleIdent, n: &DatatypeName) -> DatatypeKind {
        match self.named_member_kind(*m, n.0) {
            NamedMemberKind::Struct => DatatypeKind::Struct,
            NamedMemberKind::Enum => DatatypeKind::Enum,
            _ => panic!("ICE should have failed in naming"),
        }
    }

    pub fn datatype_declared_loc(&self, m: &ModuleIdent, n: &DatatypeName) -> Loc {
        match self.datatype_kind(m, n) {
            DatatypeKind::Struct => self.struct_declared_loc_(m, &n.0.value),
            DatatypeKind::Enum => self.enum_declared_loc_(m, &n.0.value),
        }
    }

    pub fn datatype_declared_abilities(&self, m: &ModuleIdent, n: &DatatypeName) -> &AbilitySet {
        match self.datatype_kind(m, n) {
            DatatypeKind::Struct => self.struct_declared_abilities(m, n),
            DatatypeKind::Enum => self.enum_declared_abilities(m, n),
        }
    }

    pub fn struct_definition(&self, m: &ModuleIdent, n: &DatatypeName) -> &StructDefinition {
        self.struct_definition_opt(m, n)
            .expect("ICE should have failed in naming")
    }

    pub fn struct_definition_opt(
        &self,
        m: &ModuleIdent,
        n: &DatatypeName,
    ) -> Option<&StructDefinition> {
        self.module_opt(m)?.structs.get(n)
    }

    pub fn struct_declared_abilities(&self, m: &ModuleIdent, n: &DatatypeName) -> &AbilitySet {
        &self.struct_definition(m, n).abilities
    }

    pub fn struct_declared_loc(&self, m: &ModuleIdent, n: &DatatypeName) -> Loc {
        self.struct_declared_loc_(m, &n.0.value)
    }

    pub fn struct_declared_loc_(&self, m: &ModuleIdent, n: &Symbol) -> Loc {
        let minfo = self.module(m);
        *minfo
            .structs
            .get_loc_(n)
            .expect("ICE should have failed in naming")
    }

    pub fn struct_type_parameters(
        &self,
        m: &ModuleIdent,
        n: &DatatypeName,
    ) -> &Vec<DatatypeTypeParameter> {
        &self.struct_definition(m, n).type_parameters
    }

    pub fn is_struct(&self, module: &ModuleIdent, datatype_name: &DatatypeName) -> bool {
        matches!(
            self.datatype_kind(module, datatype_name),
            DatatypeKind::Struct
        )
    }

    pub fn struct_fields(
        &self,
        module: &ModuleIdent,
        struct_name: &DatatypeName,
    ) -> Option<UniqueMap<Field, usize>> {
        match &self.struct_definition(module, struct_name).fields {
            N::StructFields::Defined(_, fields) => Some(fields.ref_map(|_, (ndx, _)| *ndx)),
            N::StructFields::Native(_) => None,
        }
    }

    /// Indicates if the struct is positional. Returns false on native.
    pub fn struct_is_positional(&self, module: &ModuleIdent, struct_name: &DatatypeName) -> bool {
        match self.struct_definition(module, struct_name).fields {
            N::StructFields::Defined(is_positional, _) => is_positional,
            N::StructFields::Native(_) => false,
        }
    }

    pub fn enum_definition(&self, m: &ModuleIdent, n: &DatatypeName) -> &EnumDefinition {
        self.enum_definition_opt(m, n)
            .expect("ICE should have failed in naming")
    }

    pub fn enum_definition_opt(
        &self,
        m: &ModuleIdent,
        n: &DatatypeName,
    ) -> Option<&EnumDefinition> {
        self.module_opt(m)?.enums.get(n)
    }

    pub fn enum_declared_abilities(&self, m: &ModuleIdent, n: &DatatypeName) -> &AbilitySet {
        &self.enum_definition(m, n).abilities
    }

    pub fn enum_declared_loc(&self, m: &ModuleIdent, n: &DatatypeName) -> Loc {
        self.enum_declared_loc_(m, &n.0.value)
    }

    pub fn enum_declared_loc_(&self, m: &ModuleIdent, n: &Symbol) -> Loc {
        let minfo = self.module(m);
        *minfo
            .enums
            .get_loc_(n)
            .expect("ICE should have failed in naming")
    }

    pub fn enum_type_parameters(
        &self,
        m: &ModuleIdent,
        n: &DatatypeName,
    ) -> &Vec<DatatypeTypeParameter> {
        &self.enum_definition(m, n).type_parameters
    }

    /// Returns the enum variant names in sorted order.
    pub fn enum_variants(
        &self,
        module: &ModuleIdent,
        enum_name: &DatatypeName,
    ) -> Vec<VariantName> {
        let mut names = self
            .enum_definition(module, enum_name)
            .variants
            .ref_map(|_, vdef| vdef.index)
            .clone()
            .into_iter()
            .collect::<Vec<_>>();
        names.sort_by(|(_, ndx0), (_, ndx1)| ndx0.cmp(ndx1));
        names.into_iter().map(|(name, _ndx)| name).collect()
    }

    pub fn enum_variant_fields(
        &self,
        module: &ModuleIdent,
        enum_name: &DatatypeName,
        variant_name: &VariantName,
    ) -> Option<UniqueMap<Field, usize>> {
        let Some(variant) = self
            .enum_definition(module, enum_name)
            .variants
            .get(variant_name)
        else {
            return None;
        };
        match &variant.fields {
            N::VariantFields::Defined(_, fields) => Some(fields.ref_map(|_, (ndx, _)| *ndx)),
            N::VariantFields::Empty => Some(UniqueMap::new()),
        }
    }

    /// Indicates if the enum variant is empty.
    pub fn enum_variant_is_empty(
        &self,
        module: &ModuleIdent,
        enum_name: &DatatypeName,
        variant_name: &VariantName,
    ) -> bool {
        let vdef = self
            .enum_definition(module, enum_name)
            .variants
            .get(variant_name)
            .expect("ICE should have failed during naming");
        match &vdef.fields {
            N::VariantFields::Empty => true,
            N::VariantFields::Defined(_, _m) => false,
        }
    }

    /// Indicates if the enum variant is positional. Returns false on empty or missing.
    pub fn enum_variant_is_positional(
        &self,
        module: &ModuleIdent,
        enum_name: &DatatypeName,
        variant_name: &VariantName,
    ) -> bool {
        let vdef = self
            .enum_definition(module, enum_name)
            .variants
            .get(variant_name)
            .expect("ICE should have failed during naming");
        match &vdef.fields {
            N::VariantFields::Empty => false,
            N::VariantFields::Defined(is_positional, _m) => *is_positional,
        }
    }
}

impl Display for NamedMemberKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NamedMemberKind::Struct => write!(f, "struct"),
            NamedMemberKind::Enum => write!(f, "enum"),
            NamedMemberKind::Function => write!(f, "function"),
            NamedMemberKind::Constant => write!(f, "constant"),
        }
    }
}

impl From<NamedMemberKind> for AttributePosition {
    fn from(nmk: NamedMemberKind) -> Self {
        match nmk {
            NamedMemberKind::Struct => AttributePosition::Struct,
            NamedMemberKind::Enum => AttributePosition::Enum,
            NamedMemberKind::Function => AttributePosition::Function,
            NamedMemberKind::Constant => AttributePosition::Constant,
        }
    }
}

impl From<DatatypeKind> for NamedMemberKind {
    fn from(dt: DatatypeKind) -> Self {
        match dt {
            DatatypeKind::Struct => NamedMemberKind::Struct,
            DatatypeKind::Enum => NamedMemberKind::Enum,
        }
    }
}
