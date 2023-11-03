// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

use crate::{
    expansion::ast::{AbilitySet, Attributes, ModuleIdent, Visibility},
    naming::ast::{
        self as N, DatatypeTypeParameter, EnumDefinition, FunctionSignature, ResolvedUseFuns,
        StructDefinition, Type,
    },
    parser::ast::{ConstantName, DatatypeName, FunctionName},
    shared::unique_map::UniqueMap,
    shared::*,
    typing::ast::{self as T},
    FullyCompiledProgram,
};

#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub attributes: Attributes,
    pub defined_loc: Loc,
    pub visibility: Visibility,
    pub entry: Option<Loc>,
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
    pub attributes: Attributes,
    pub package: Option<Symbol>,
    pub use_funs: ResolvedUseFuns,
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
                attributes: mdef.attributes.clone(),
                package: mdef.package_name,
                use_funs,
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
        pre_compiled_lib: Option<&FullyCompiledProgram>,
        prog: &T::Program_,
        mut module_use_funs: BTreeMap<ModuleIdent, ResolvedUseFuns>,
    ) -> Self {
        let mut module_use_funs = Some(&mut module_use_funs);
        program_info!(pre_compiled_lib, prog, typing, module_use_funs)
    }
}

impl NamingProgramInfo {
    pub fn new(pre_compiled_lib: Option<&FullyCompiledProgram>, prog: &N::Program_) -> Self {
        // use_funs will be populated later
        let mut module_use_funs: Option<&mut BTreeMap<ModuleIdent, ResolvedUseFuns>> = None;
        program_info!(pre_compiled_lib, prog, naming, module_use_funs)
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

    pub fn datatype_kind(&self, m: &ModuleIdent, n: &DatatypeName) -> DatatypeKind {
        match (
            self.module(m).structs.contains_key(n),
            self.module(m).enums.contains_key(n),
        ) {
            (true, false) => DatatypeKind::Struct,
            (false, true) => DatatypeKind::Enum,
            (false, false) | (true, true) => panic!("ICE should have failed in naming"),
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
}

impl NamingProgramInfo {
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
}
