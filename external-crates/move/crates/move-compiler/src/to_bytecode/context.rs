// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    expansion::ast::{Address, ModuleIdent, ModuleIdent_},
    parser::ast::{ConstantName, DatatypeName, FunctionName, VariantName},
    shared::{CompilationEnv, NumericalAddress},
};
use move_core_types::account_address::AccountAddress as MoveAddress;
use move_ir_types::ast as IR;
use move_symbol_pool::Symbol;
use std::{
    clone::Clone,
    collections::{BTreeMap, BTreeSet, HashMap},
};

#[derive(Debug)]
pub struct FunctionDeclaration {
    pub seen_datatypes: BTreeSet<(ModuleIdent, DatatypeName)>,
    pub signature: IR::FunctionSignature,
}

pub type DatatypeDeclarations =
    HashMap<(ModuleIdent, DatatypeName), (BTreeSet<IR::Ability>, Vec<IR::DatatypeTypeParameter>)>;

/// Compilation context for a single compilation unit (module).
/// Contains all of the dependencies actually used in the module
pub struct Context<'a> {
    pub env: &'a CompilationEnv,
    current_package: Option<Symbol>,
    current_module: Option<&'a ModuleIdent>,
    seen_datatypes: BTreeSet<(ModuleIdent, DatatypeName)>,
    seen_functions: BTreeSet<(ModuleIdent, FunctionName)>,
}

impl<'a> Context<'a> {
    pub fn new(
        env: &'a CompilationEnv,
        current_package: Option<Symbol>,
        current_module: Option<&'a ModuleIdent>,
    ) -> Self {
        Self {
            env,
            current_package,
            current_module,
            seen_datatypes: BTreeSet::new(),
            seen_functions: BTreeSet::new(),
        }
    }

    #[allow(unused)]
    pub fn current_package(&self) -> Option<Symbol> {
        self.current_package
    }

    pub fn current_module(&self) -> Option<&'a ModuleIdent> {
        self.current_module
    }

    fn is_current_module(&self, m: &ModuleIdent) -> bool {
        self.current_module.map(|cur| cur == m).unwrap_or(false)
    }

    //**********************************************************************************************
    // Dependency item building
    //**********************************************************************************************

    pub fn materialize(
        self,
        dependency_orderings: &HashMap<ModuleIdent, usize>,
        datatype_declarations: &DatatypeDeclarations,
        function_declarations: &HashMap<(ModuleIdent, FunctionName), FunctionDeclaration>,
    ) -> (Vec<IR::ImportDefinition>, Vec<IR::ModuleDependency>) {
        let Context {
            current_module: _current_module,
            mut seen_datatypes,
            seen_functions,
            ..
        } = self;
        let mut module_dependencies = BTreeMap::new();
        Self::function_dependencies(
            function_declarations,
            &mut module_dependencies,
            &mut seen_datatypes,
            seen_functions,
        );

        Self::datatype_dependencies(
            datatype_declarations,
            &mut module_dependencies,
            seen_datatypes,
        );

        let mut imports = vec![];
        let mut ordered_dependencies = vec![];
        for (module, (structs, functions)) in module_dependencies {
            let dependency_order = dependency_orderings[&module];
            let ir_name = Self::ir_module_alias(&module);
            let ir_ident = Self::translate_module_ident(module);
            imports.push(IR::ImportDefinition::new(ir_ident, Some(ir_name)));
            ordered_dependencies.push((
                dependency_order,
                IR::ModuleDependency {
                    name: ir_name,
                    datatypes: structs,
                    functions,
                },
            ));
        }
        ordered_dependencies.sort_by_key(|(ordering, _)| *ordering);
        let dependencies = ordered_dependencies.into_iter().map(|(_, m)| m).collect();
        (imports, dependencies)
    }

    fn insert_datatype_dependency(
        module_dependencies: &mut BTreeMap<
            ModuleIdent,
            (Vec<IR::DatatypeDependency>, Vec<IR::FunctionDependency>),
        >,
        module: ModuleIdent,
        datatype_dep: IR::DatatypeDependency,
    ) {
        module_dependencies
            .entry(module)
            .or_insert_with(|| (vec![], vec![]))
            .0
            .push(datatype_dep);
    }

    fn insert_function_dependency(
        module_dependencies: &mut BTreeMap<
            ModuleIdent,
            (Vec<IR::DatatypeDependency>, Vec<IR::FunctionDependency>),
        >,
        module: ModuleIdent,
        function_dep: IR::FunctionDependency,
    ) {
        module_dependencies
            .entry(module)
            .or_insert_with(|| (vec![], vec![]))
            .1
            .push(function_dep);
    }

    fn datatype_dependencies(
        datatype_declarations: &DatatypeDeclarations,
        module_dependencies: &mut BTreeMap<
            ModuleIdent,
            (Vec<IR::DatatypeDependency>, Vec<IR::FunctionDependency>),
        >,
        seen_datatypes: BTreeSet<(ModuleIdent, DatatypeName)>,
    ) {
        for (module, sname) in seen_datatypes {
            let datatype_dep = Self::datatype_dependency(datatype_declarations, &module, sname);
            Self::insert_datatype_dependency(module_dependencies, module, datatype_dep);
        }
    }

    fn datatype_dependency(
        datatype_declarations: &DatatypeDeclarations,
        module: &ModuleIdent,
        sname: DatatypeName,
    ) -> IR::DatatypeDependency {
        let key = (*module, sname);
        let (abilities, type_formals) = datatype_declarations.get(&key).unwrap().clone();
        let name = Self::translate_datatype_name(sname);
        IR::DatatypeDependency {
            abilities,
            name,
            type_formals,
        }
    }

    fn function_dependencies(
        function_declarations: &HashMap<(ModuleIdent, FunctionName), FunctionDeclaration>,
        module_dependencies: &mut BTreeMap<
            ModuleIdent,
            (Vec<IR::DatatypeDependency>, Vec<IR::FunctionDependency>),
        >,
        seen_datatypes: &mut BTreeSet<(ModuleIdent, DatatypeName)>,
        seen_functions: BTreeSet<(ModuleIdent, FunctionName)>,
    ) {
        for (module, fname) in seen_functions {
            let (function_seen_datatypes, function_dep) =
                Self::function_dependency(function_declarations, &module, fname);
            Self::insert_function_dependency(module_dependencies, module, function_dep);
            seen_datatypes.extend(function_seen_datatypes);
        }
    }

    fn function_dependency(
        function_declarations: &HashMap<(ModuleIdent, FunctionName), FunctionDeclaration>,
        module: &ModuleIdent,
        fname: FunctionName,
    ) -> (
        BTreeSet<(ModuleIdent, DatatypeName)>,
        IR::FunctionDependency,
    ) {
        let key = (*module, fname);
        let FunctionDeclaration {
            seen_datatypes,
            signature,
        } = function_declarations.get(&key).unwrap();
        let name = Self::translate_function_name(fname);
        (
            seen_datatypes.clone(),
            IR::FunctionDependency {
                name,
                signature: signature.clone(),
            },
        )
    }

    //**********************************************************************************************
    // Name translation
    //**********************************************************************************************

    fn ir_module_alias(sp!(_, ModuleIdent_ { address, module }): &ModuleIdent) -> IR::ModuleName {
        let s = match address {
            Address::Numerical {
                value: sp!(_, a_), ..
            } => format!("{:X}::{}", a_, module),
            Address::NamedUnassigned(name) => format!("{}::{}", name, module),
        };
        IR::ModuleName(s.into())
    }

    pub fn resolve_address(&self, addr: Address) -> NumericalAddress {
        addr.into_addr_bytes()
    }

    pub fn translate_module_ident(
        sp!(_, ModuleIdent_ { address, module }): ModuleIdent,
    ) -> IR::ModuleIdent {
        let address_bytes = address.into_addr_bytes();
        let name = Self::translate_module_name_(module.0.value);
        IR::ModuleIdent::new(name, MoveAddress::new(address_bytes.into_bytes()))
    }

    fn translate_module_name_(s: Symbol) -> IR::ModuleName {
        IR::ModuleName(s)
    }

    fn translate_datatype_name(n: DatatypeName) -> IR::DatatypeName {
        IR::DatatypeName(n.0.value)
    }

    fn translate_variant_name(n: VariantName) -> IR::VariantName {
        IR::VariantName(n.0.value)
    }

    fn translate_constant_name(n: ConstantName) -> IR::ConstantName {
        IR::ConstantName(n.0.value)
    }

    fn translate_function_name(n: FunctionName) -> IR::FunctionName {
        IR::FunctionName(n.0.value)
    }

    //**********************************************************************************************
    // Name resolution
    //**********************************************************************************************

    pub fn struct_definition_name(&self, m: &ModuleIdent, s: DatatypeName) -> IR::DatatypeName {
        assert!(
            self.is_current_module(m),
            "ICE invalid struct definition lookup"
        );
        Self::translate_datatype_name(s)
    }

    pub fn enum_definition_name(&self, m: &ModuleIdent, e: DatatypeName) -> IR::DatatypeName {
        assert!(
            self.is_current_module(m),
            "ICE invalid enum definition lookup"
        );
        Self::translate_datatype_name(e)
    }

    pub fn variant_name(&self, v: VariantName) -> IR::VariantName {
        Self::translate_variant_name(v)
    }

    pub fn qualified_datatype_name(
        &mut self,
        m: &ModuleIdent,
        s: DatatypeName,
    ) -> IR::QualifiedDatatypeIdent {
        let mname = if self.is_current_module(m) {
            IR::ModuleName::module_self()
        } else {
            self.seen_datatypes.insert((*m, s));
            Self::ir_module_alias(m)
        };
        let n = Self::translate_datatype_name(s);
        IR::QualifiedDatatypeIdent::new(mname, n)
    }

    pub fn function_definition_name(&self, m: &ModuleIdent, f: FunctionName) -> IR::FunctionName {
        assert!(
            self.current_module == Some(m),
            "ICE invalid function definition lookup"
        );
        Self::translate_function_name(f)
    }

    pub fn qualified_function_name(
        &mut self,
        m: &ModuleIdent,
        f: FunctionName,
    ) -> (IR::ModuleName, IR::FunctionName) {
        let mname = if self.is_current_module(m) {
            IR::ModuleName::module_self()
        } else {
            self.seen_functions.insert((*m, f));
            Self::ir_module_alias(m)
        };
        let n = Self::translate_function_name(f);
        (mname, n)
    }

    pub fn constant_definition_name(&self, m: &ModuleIdent, f: ConstantName) -> IR::ConstantName {
        assert!(
            self.current_module == Some(m),
            "ICE invalid constant definition lookup"
        );
        Self::translate_constant_name(f)
    }

    pub fn constant_name(&mut self, f: ConstantName) -> IR::ConstantName {
        Self::translate_constant_name(f)
    }
}
