// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{normalized, CompiledModule};
use move_core_types::{account_address::AccountAddress, identifier::IdentStr};
use move_symbol_pool::Symbol;
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct Packages {
    pub packages: BTreeMap<AccountAddress, Package>,
}

#[derive(Debug)]
pub struct Package {
    pub package: AccountAddress,
    pub modules: BTreeMap<Symbol, Module>,
}

pub type ModuleId = normalized::ModuleId<Symbol>;
pub type QualifiedMemberId = (ModuleId, Symbol);

pub type Signature = normalized::Signature<Symbol>;
pub type Type = normalized::Type<Symbol>;
pub type Datatype = normalized::Datatype<Symbol>;

pub type Module = normalized::Module<Symbol>;

pub type Constant = normalized::Constant<Symbol>;
pub type Struct = normalized::Struct<Symbol>;
pub type Field = normalized::Field<Symbol>;
pub type Function = normalized::Function<Symbol>;
pub type Enum = normalized::Enum<Symbol>;
pub type Variant = normalized::Variant<Symbol>;

pub type ConstantRef = normalized::ConstantRef<Symbol>;
pub type StructRef = normalized::StructRef<Symbol>;
pub type FieldRef = normalized::FieldRef<Symbol>;
pub type FunctionRef = normalized::FunctionRef<Symbol>;
pub type VariantRef = normalized::VariantRef<Symbol>;

pub type Bytecode = normalized::Bytecode<Symbol>;

impl Packages {
    pub fn new<'a>(compiled_modules: impl IntoIterator<Item = &'a CompiledModule>) -> Self {
        let mut packages = BTreeMap::new();

        for compiled_module in compiled_modules {
            let module = Module::new(
                &mut SymbolPool,
                compiled_module,
                /* include code */ true,
            );
            let package = packages
                .entry(*module.address())
                .or_insert_with(|| Package::new(*module.address()));
            package.insert(module);
        }
        Self { packages }
    }
}

impl Package {
    fn new(package: AccountAddress) -> Self {
        Self {
            package,
            modules: BTreeMap::new(),
        }
    }

    fn insert(&mut self, module: Module) {
        let prev = self.modules.insert(*module.name(), module);
        assert!(prev.is_none());
    }
}

pub struct SymbolPool;

impl normalized::StringPool for SymbolPool {
    type String = Symbol;

    fn intern(&mut self, s: &IdentStr) -> Self::String {
        Symbol::from(s.as_str())
    }

    fn as_ident_str<'a>(
        &'a self,
        s: &'a Self::String,
    ) -> &'a move_core_types::identifier::IdentStr {
        IdentStr::new(s.as_str()).unwrap()
    }
}

pub trait TModuleId {
    fn module_id(&self) -> ModuleId;
}
