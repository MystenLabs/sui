// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    diagnostics::Diagnostics,
    expansion::ast::{Attributes, ModuleIdent, ModuleIdent_, SpecId},
    hlir::ast as H,
    parser::ast::{FunctionName, ModuleName},
    shared::{unique_map::UniqueMap, Name, NumericalAddress},
};
use move_binary_format::file_format as F;
use move_bytecode_source_map::source_map::SourceMap;
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier as MoveCoreIdentifier,
    language_storage::ModuleId,
};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::collections::BTreeMap;

//**************************************************************************************************
// Compiled Unit
//**************************************************************************************************

#[derive(Debug, Clone)]
pub struct VarInfo {
    pub type_: H::SingleType,
    pub index: F::LocalIndex,
}

#[derive(Debug, Clone)]
pub struct SpecInfo {
    pub offset: F::CodeOffset,
    // Free locals that are used but not declared in the block
    pub used_locals: UniqueMap<H::Var, VarInfo>,
}

#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub spec_info: BTreeMap<SpecId, SpecInfo>,
    pub parameters: Vec<(H::Var, VarInfo)>,
    pub attributes: Attributes,
}

#[derive(Debug, Clone)]
pub struct NamedCompiledModule {
    // package name metadata from compiler arguments
    pub package_name: Option<Symbol>,
    pub address: NumericalAddress,
    pub name: Symbol,
    pub module: F::CompiledModule,
    pub source_map: SourceMap,
}

#[derive(Debug, Clone)]
pub struct NamedCompiledScript {
    // package name metadata from compiler arguments
    pub package_name: Option<Symbol>,
    pub name: Symbol,
    pub script: F::CompiledScript,
    pub source_map: SourceMap,
}

#[derive(Debug, Clone)]
pub struct AnnotatedCompiledModule {
    pub loc: Loc,
    pub module_name_loc: Loc,
    pub address_name: Option<Name>,
    pub named_module: NamedCompiledModule,
    pub function_infos: UniqueMap<FunctionName, FunctionInfo>,
}

#[derive(Debug, Clone)]
pub struct AnnotatedCompiledScript {
    pub loc: Loc,
    pub named_script: NamedCompiledScript,
    pub function_info: FunctionInfo,
}

pub trait TargetModule {}
pub trait TargetScript {}
impl TargetScript for AnnotatedCompiledScript {}
impl TargetScript for NamedCompiledScript {}
impl TargetModule for AnnotatedCompiledModule {}
impl TargetModule for NamedCompiledModule {}

#[derive(Debug, Clone)]
pub enum CompiledUnitEnum<TModule: TargetModule, TScript: TargetScript> {
    Module(TModule),
    Script(TScript),
}

pub type CompiledUnit = CompiledUnitEnum<NamedCompiledModule, NamedCompiledScript>;
pub type AnnotatedCompiledUnit = CompiledUnitEnum<AnnotatedCompiledModule, AnnotatedCompiledScript>;

impl AnnotatedCompiledModule {
    pub fn module_ident(&self) -> ModuleIdent {
        use crate::expansion::ast::Address;
        let address =
            Address::Numerical(self.address_name, sp(self.loc, self.named_module.address));
        sp(
            self.loc,
            ModuleIdent_::new(
                address,
                ModuleName(sp(self.module_name_loc, self.named_module.name)),
            ),
        )
    }

    pub fn module_id(&self) -> (Option<Name>, ModuleId) {
        let id = ModuleId::new(
            AccountAddress::new(self.named_module.address.into_bytes()),
            MoveCoreIdentifier::new(self.named_module.name.to_string()).unwrap(),
        );
        (self.address_name, id)
    }
}

impl AnnotatedCompiledUnit {
    pub fn verify(&self) -> Diagnostics {
        match self {
            Self::Module(AnnotatedCompiledModule {
                loc,
                named_module:
                    NamedCompiledModule {
                        module, source_map, ..
                    },
                ..
            }) => verify_module(source_map, *loc, module),
            Self::Script(AnnotatedCompiledScript {
                loc,
                named_script:
                    NamedCompiledScript {
                        script, source_map, ..
                    },
                ..
            }) => verify_script(source_map, *loc, script),
        }
    }

    pub fn into_compiled_unit(self) -> CompiledUnit {
        match self {
            Self::Module(AnnotatedCompiledModule {
                named_module: module,
                ..
            }) => CompiledUnitEnum::Module(module),
            Self::Script(AnnotatedCompiledScript {
                named_script: script,
                ..
            }) => CompiledUnitEnum::Script(script),
        }
    }

    pub fn package_name(&self) -> Option<Symbol> {
        match self {
            Self::Module(AnnotatedCompiledModule { named_module, .. }) => named_module.package_name,
            Self::Script(AnnotatedCompiledScript { named_script, .. }) => named_script.package_name,
        }
    }

    pub fn loc(&self) -> &Loc {
        match self {
            Self::Module(AnnotatedCompiledModule { loc, .. })
            | Self::Script(AnnotatedCompiledScript { loc, .. }) => loc,
        }
    }
}

impl CompiledUnit {
    pub fn name(&self) -> Symbol {
        match self {
            Self::Module(NamedCompiledModule { name, .. })
            | Self::Script(NamedCompiledScript { name, .. }) => *name,
        }
    }

    pub fn package_name(&self) -> Option<Symbol> {
        match self {
            Self::Module(NamedCompiledModule { package_name, .. })
            | Self::Script(NamedCompiledScript { package_name, .. }) => *package_name,
        }
    }

    pub fn source_map(&self) -> &SourceMap {
        match self {
            Self::Module(NamedCompiledModule { source_map, .. })
            | Self::Script(NamedCompiledScript { source_map, .. }) => source_map,
        }
    }

    pub fn serialize(&self, bytecode_version: Option<u32>) -> Vec<u8> {
        let mut serialized = Vec::<u8>::new();
        match self {
            Self::Module(NamedCompiledModule { module, .. }) => module
                .serialize_for_version(bytecode_version, &mut serialized)
                .unwrap(),
            Self::Script(NamedCompiledScript { script, .. }) => script
                .serialize_for_version(bytecode_version, &mut serialized)
                .unwrap(),
        };
        serialized
    }

    #[allow(dead_code)]
    pub fn serialize_debug(self) -> Vec<u8> {
        match self {
            Self::Module(NamedCompiledModule { module, .. }) => format!("{:?}", module),
            Self::Script(NamedCompiledScript { script, .. }) => format!("{:?}", script),
        }
        .into()
    }

    pub fn serialize_source_map(&self) -> Vec<u8> {
        match self {
            Self::Module(NamedCompiledModule { source_map, .. }) => {
                bcs::to_bytes(source_map).unwrap()
            }
            Self::Script(NamedCompiledScript { source_map, .. }) => {
                bcs::to_bytes(source_map).unwrap()
            }
        }
    }
}

fn bytecode_verifier_mismatch_bug(
    sm: &SourceMap,
    loc: Loc,
    location: move_binary_format::errors::Location,
    e: move_binary_format::errors::VMError,
) -> Diagnostics {
    let loc = match e.offsets().first() {
        Some((fdef_idx, offset)) if &location == e.location() => {
            sm.get_code_location(*fdef_idx, *offset).unwrap_or(loc)
        }
        _ => loc,
    };
    Diagnostics::from(vec![diag!(
        Bug::BytecodeVerification,
        (loc, format!("ICE failed bytecode verifier: {:#?}", e)),
    )])
}

fn verify_module(sm: &SourceMap, loc: Loc, cm: &F::CompiledModule) -> Diagnostics {
    match move_bytecode_verifier::verifier::verify_module(cm) {
        Ok(_) => Diagnostics::new(),
        Err(e) => bytecode_verifier_mismatch_bug(
            sm,
            loc,
            move_binary_format::errors::Location::Module(cm.self_id()),
            e,
        ),
    }
}

fn verify_script(sm: &SourceMap, loc: Loc, cs: &F::CompiledScript) -> Diagnostics {
    match move_bytecode_verifier::verifier::verify_script(cs) {
        Ok(_) => Diagnostics::new(),
        Err(e) => {
            bytecode_verifier_mismatch_bug(sm, loc, move_binary_format::errors::Location::Script, e)
        }
    }
}

pub fn verify_units<'a>(units: impl IntoIterator<Item = &'a AnnotatedCompiledUnit>) -> Diagnostics {
    let mut diags = Diagnostics::new();
    for unit in units {
        diags.extend(unit.verify());
    }
    diags
}
