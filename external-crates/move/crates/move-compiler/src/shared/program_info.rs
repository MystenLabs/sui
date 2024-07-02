// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt::Display, sync::Arc};

use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

use crate::{
    expansion::ast::{AbilitySet, Attributes, ModuleIdent, TargetKind, Visibility},
    naming::ast::{
        self as N, DatatypeTypeParameter, EnumDefinition, FunctionSignature, ResolvedUseFuns,
        StructDefinition, SyntaxMethods, Type,
    },
    parser::ast::{ConstantName, DatatypeName, Field, FunctionName, VariantName},
    shared::unique_map::UniqueMap,
    shared::*,
    typing::ast::{self as T},
    FullyCompiledProgram,
};

use self::known_attributes::AttributePosition;

#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub attributes: Attributes,
    pub defined_loc: Loc,
    pub visibility: Visibility,
    pub entry: Option<Loc>,
    pub macro_: Option<Loc>,
    pub signature: FunctionSignature,
}

#[derive(Debug, Clone)]
pub struct ConstantInfo {
    pub attributes: Attributes,
    pub defined_loc: Loc,
    pub signature: Type,
}

#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub target_kind: TargetKind,
    pub attributes: Attributes,
    pub package: Option<Symbol>,
    pub use_funs: ResolvedUseFuns,
    pub syntax_methods: SyntaxMethods,
    pub friends: UniqueMap<ModuleIdent, Loc>,
    pub structs: UniqueMap<DatatypeName, StructDefinition>,
    pub enums: UniqueMap<DatatypeName, EnumDefinition>,
    pub functions: UniqueMap<FunctionName, FunctionInfo>,
    pub constants: UniqueMap<ConstantName, ConstantInfo>,
}

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

#[derive(Debug, Clone)]
pub struct ProgramInfo<const AFTER_TYPING: bool> {
    pub modules: UniqueMap<ModuleIdent, ModuleInfo>,
}
pub type NamingProgramInfo = ProgramInfo<false>;
pub type TypingProgramInfo = ProgramInfo<true>;

macro_rules! program_info {
    ($pre_compiled_lib:ident, $prog:ident, $pass:ident, $module_use_funs:ident) => {{
        let all_modules = $prog.modules.key_cloned_iter();
        let mut modules = UniqueMap::maybe_from_iter(all_modules.map(|(mident, mdef)| {
            let structs = mdef.structs.clone();
            let enums = mdef.enums.clone();
            let functions = mdef.functions.ref_map(|fname, fdef| FunctionInfo {
                attributes: fdef.attributes.clone(),
                defined_loc: fname.loc(),
                visibility: fdef.visibility.clone(),
                entry: fdef.entry,
                macro_: fdef.macro_,
                signature: fdef.signature.clone(),
            });
            let constants = mdef.constants.ref_map(|cname, cdef| ConstantInfo {
                attributes: cdef.attributes.clone(),
                defined_loc: cname.loc(),
                signature: cdef.signature.clone(),
            });
            let use_funs = $module_use_funs
                .as_mut()
                .map(|module_use_funs| module_use_funs.remove(&mident).unwrap())
                .unwrap_or_default();
            let minfo = ModuleInfo {
                target_kind: mdef.target_kind,
                attributes: mdef.attributes.clone(),
                package: mdef.package_name,
                use_funs,
                syntax_methods: mdef.syntax_methods.clone(),
                friends: mdef.friends.ref_map(|_, friend| friend.loc),
                structs,
                enums,
                functions,
                constants,
            };
            (mident, minfo)
        }))
        .unwrap();
        if let Some(pre_compiled_lib) = $pre_compiled_lib {
            for (mident, minfo) in pre_compiled_lib.$pass.info.modules.key_cloned_iter() {
                if !modules.contains_key(&mident) {
                    modules.add(mident, minfo.clone()).unwrap();
                }
            }
        }
        ProgramInfo { modules }
    }};
}

impl TypingProgramInfo {
    pub fn new(
        pre_compiled_lib: Option<Arc<FullyCompiledProgram>>,
        modules: &UniqueMap<ModuleIdent, T::ModuleDefinition>,
        mut module_use_funs: BTreeMap<ModuleIdent, ResolvedUseFuns>,
    ) -> Self {
        struct Prog<'a> {
            modules: &'a UniqueMap<ModuleIdent, T::ModuleDefinition>,
        }
        let mut module_use_funs = Some(&mut module_use_funs);
        let prog = Prog { modules };
        program_info!(pre_compiled_lib, prog, typing, module_use_funs)
    }
}

impl NamingProgramInfo {
    pub fn new(pre_compiled_lib: Option<Arc<FullyCompiledProgram>>, prog: &N::Program_) -> Self {
        // use_funs will be populated later
        let mut module_use_funs: Option<&mut BTreeMap<ModuleIdent, ResolvedUseFuns>> = None;
        program_info!(pre_compiled_lib, prog, naming, module_use_funs)
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
        self.modules
            .get(m)
            .expect("ICE should have failed in naming")
    }

    pub fn struct_definition(&self, m: &ModuleIdent, n: &DatatypeName) -> &StructDefinition {
        let minfo = self.module(m);
        minfo
            .structs
            .get(n)
            .expect("ICE should have failed in naming")
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

    pub fn enum_definition(&self, m: &ModuleIdent, n: &DatatypeName) -> &EnumDefinition {
        let minfo = self.module(m);
        minfo
            .enums
            .get(n)
            .expect("ICE should have failed in naming")
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

    pub fn datatype_kind(&self, m: &ModuleIdent, n: &DatatypeName) -> DatatypeKind {
        match self.named_member_kind(*m, n.0) {
            NamedMemberKind::Struct => DatatypeKind::Struct,
            NamedMemberKind::Enum => DatatypeKind::Enum,
            _ => panic!("ICE should have failed in naming"),
        }
    }

    pub fn function_info(&self, m: &ModuleIdent, n: &FunctionName) -> &FunctionInfo {
        self.module(m)
            .functions
            .get(n)
            .expect("ICE should have failed in naming")
    }

    pub fn constant_info(&mut self, m: &ModuleIdent, n: &ConstantName) -> &ConstantInfo {
        let constants = &self.module(m).constants;
        constants.get(n).expect("ICE should have failed in naming")
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
        let fields = match &self.struct_definition(module, struct_name).fields {
            N::StructFields::Defined(_, fields) => Some(fields.ref_map(|_, (ndx, _)| *ndx)),
            N::StructFields::Native(_) => None,
        };
        fields
    }

    /// Indicates if the struct is positional. Returns false on native.
    pub fn struct_is_positional(&self, module: &ModuleIdent, struct_name: &DatatypeName) -> bool {
        match self.struct_definition(module, struct_name).fields {
            N::StructFields::Defined(is_positional, _) => is_positional,
            N::StructFields::Native(_) => false,
        }
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
