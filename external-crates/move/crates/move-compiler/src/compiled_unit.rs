// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    diagnostics::Diagnostics,
    expansion::ast::{Attributes, ModuleIdent, ModuleIdent_},
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

//**************************************************************************************************
// Compiled Unit
//**************************************************************************************************

#[derive(Debug, Clone)]
pub struct VarInfo {
    pub type_: H::SingleType,
    pub index: F::LocalIndex,
}

#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub parameters: Vec<(H::Var, VarInfo)>,
    pub attributes: Attributes,
}

#[derive(Debug, Clone)]
pub struct NamedCompiledModule {
    // package name metadata from compiler arguments
    pub package_name: Option<Symbol>,
    pub address: NumericalAddress,
    pub address_name: Option<Name>,
    pub name: Symbol,
    pub module: F::CompiledModule,
    pub source_map: SourceMap,
}

#[derive(Debug, Clone)]
pub struct AnnotatedCompiledModule {
    pub loc: Loc,
    pub attributes: Attributes,
    pub module_name_loc: Loc,
    pub named_module: NamedCompiledModule,
    pub function_infos: UniqueMap<FunctionName, FunctionInfo>,
}

pub trait TargetModule {}
impl TargetModule for AnnotatedCompiledModule {}
impl TargetModule for NamedCompiledModule {}

pub type CompiledUnit = NamedCompiledModule;
pub type AnnotatedCompiledUnit = AnnotatedCompiledModule;

impl AnnotatedCompiledModule {
    pub fn module_ident(&self) -> ModuleIdent {
        use crate::expansion::ast::Address;
        let address = Address::Numerical {
            name: self.named_module.address_name,
            value: sp(self.loc, self.named_module.address),
            name_conflict: false,
        };
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
        (self.named_module.address_name, id)
    }

    pub fn verify(&self) -> Diagnostics {
        let AnnotatedCompiledModule {
            loc,
            named_module: NamedCompiledModule {
                module, source_map, ..
            },
            ..
        } = self;
        verify_module(source_map, *loc, module)
    }

    pub fn into_compiled_unit(self) -> CompiledUnit {
        self.named_module
    }

    pub fn package_name(&self) -> Option<Symbol> {
        self.named_module.package_name
    }

    pub fn loc(&self) -> &Loc {
        &self.loc
    }
}

impl NamedCompiledModule {
    pub fn name(&self) -> Symbol {
        self.name
    }

    pub fn package_name(&self) -> Option<Symbol> {
        self.package_name
    }

    pub fn address_name(&self) -> Option<Name> {
        self.address_name
    }

    pub fn source_map(&self) -> &SourceMap {
        &self.source_map
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut serialized = Vec::<u8>::new();
        self.module
            .serialize_with_version(self.module.version, &mut serialized)
            .unwrap();
        serialized
    }

    #[allow(dead_code)]
    pub fn serialize_debug(self) -> Vec<u8> {
        format!("{:?}", self.module).into()
    }

    pub fn serialize_source_map(&self) -> Vec<u8> {
        bcs::to_bytes(&self.source_map).unwrap()
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
    match move_bytecode_verifier::verifier::verify_module_unmetered(cm) {
        Ok(_) => Diagnostics::new(),
        Err(e) => bytecode_verifier_mismatch_bug(
            sm,
            loc,
            move_binary_format::errors::Location::Module(cm.self_id()),
            e,
        ),
    }
}

pub fn verify_units<'a>(units: impl IntoIterator<Item = &'a AnnotatedCompiledUnit>) -> Diagnostics {
    let mut diags = Diagnostics::new();
    for unit in units {
        diags.extend(unit.verify());
    }
    diags
}
