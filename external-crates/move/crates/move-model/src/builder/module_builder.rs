// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use itertools::Itertools;

use move_binary_format::{
    file_format::{Constant, EnumDefinitionIndex, FunctionDefinitionIndex, StructDefinitionIndex},
    CompiledModule,
};
use move_bytecode_source_map::source_map::SourceMap;
use move_compiler::{
    expansion::ast as EA,
    parser::ast as PA,
    shared::{unique_map::UniqueMap, Name, TName},
};
use move_ir_types::ast::ConstantName;

use crate::{
    ast::{Attribute, AttributeValue, ModuleName, QualifiedSymbol, Value},
    builder::{
        exp_translator::ExpTranslator,
        model_builder::{ConstEntry, DatatypeData, ModelBuilder},
    },
    model::{
        DatatypeId, EnumData, FunId, FunctionData, Loc, ModuleId, NamedConstantData,
        NamedConstantId, StructData, SCRIPT_BYTECODE_FUN_NAME,
    },
    project_1st,
    symbol::{Symbol, SymbolPool},
    ty::Type,
};

#[derive(Debug)]
pub(crate) struct ModuleBuilder<'env, 'translator> {
    pub parent: &'translator mut ModelBuilder<'env>,
    /// Id of the currently build module.
    pub module_id: ModuleId,
    /// Name of the currently build module.
    pub module_name: ModuleName,
}

/// # Entry Points

impl<'env, 'translator> ModuleBuilder<'env, 'translator> {
    pub fn new(
        parent: &'translator mut ModelBuilder<'env>,
        module_id: ModuleId,
        module_name: ModuleName,
    ) -> Self {
        Self {
            parent,
            module_id,
            module_name,
        }
    }

    /// Translates the given module definition from the Move compiler's expansion phase,
    /// combined with a compiled module (bytecode) and a source map, and enters it into
    /// this global environment. Any type check or others errors encountered will be collected
    /// in the environment for later processing. Dependencies of this module are guaranteed to
    /// have been analyzed and being already part of the environment.
    ///
    /// Translation happens in three phases:
    ///
    /// 1. In the *declaration analysis*, we collect all information about structs, functions,
    ///    spec functions, spec vars, and schemas in a module. We do not yet analyze function
    ///    bodies, conditions, and invariants, which we can only analyze after we know all
    ///    global declarations (declaration of globals is order independent, and they can have
    ///    cyclic references).
    /// 2. In the *definition analysis*, we visit the definitions we have skipped in step (1),
    ///    specifically analyzing and type checking expressions and schema inclusions.
    /// 3. In the *population phase*, we populate the global environment with the information
    ///    from this module.
    pub fn translate(
        &mut self,
        loc: Loc,
        module_def: EA::ModuleDefinition,
        compiled_module: CompiledModule,
        source_map: SourceMap,
    ) {
        self.decl_ana(&module_def, &compiled_module, &source_map);
        self.def_ana(&module_def);
        let attrs = self.translate_attributes(&module_def.attributes);
        self.populate_env_from_result(loc, attrs, compiled_module, source_map);
    }
}

impl<'env, 'translator> ModuleBuilder<'env, 'translator> {
    /// Shortcut for accessing the symbol pool.
    pub fn symbol_pool(&self) -> &SymbolPool {
        self.parent.env.symbol_pool()
    }

    /// Qualifies the given symbol by the current module.
    pub fn qualified_by_module(&self, sym: Symbol) -> QualifiedSymbol {
        QualifiedSymbol {
            module_name: self.module_name.clone(),
            symbol: sym,
        }
    }

    /// Qualifies the given name by the current module.
    fn qualified_by_module_from_name(&self, name: &Name) -> QualifiedSymbol {
        let sym = self.symbol_pool().make(&name.value);
        self.qualified_by_module(sym)
    }

    /// Converts a ModuleAccess into its parts, an optional ModuleName and base name.
    pub fn module_access_to_parts(
        &self,
        access: &EA::ModuleAccess,
    ) -> (Option<ModuleName>, Symbol) {
        match &access.value {
            EA::ModuleAccess_::Name(n) => (None, self.symbol_pool().make(n.value.as_str())),
            EA::ModuleAccess_::ModuleAccess(m, n) => {
                let loc = self.parent.to_loc(&m.loc);
                let addr_bytes = self.parent.resolve_address(&loc, &m.value.address);
                let module_name = ModuleName::from_address_bytes_and_name(
                    addr_bytes,
                    self.symbol_pool().make(m.value.module.0.value.as_str()),
                );
                (Some(module_name), self.symbol_pool().make(n.value.as_str()))
            }
            EA::ModuleAccess_::Variant(..) => panic!("Variants are not supported by move model."),
        }
    }

    /// Converts a ModuleAccess into a qualified symbol which can be used for lookup of
    /// types or functions.
    pub fn module_access_to_qualified(&self, access: &EA::ModuleAccess) -> QualifiedSymbol {
        let (module_name_opt, symbol) = self.module_access_to_parts(access);
        let module_name = module_name_opt.unwrap_or_else(|| self.module_name.clone());
        QualifiedSymbol {
            module_name,
            symbol,
        }
    }
}

/// # Attribute Analysis

impl<'env, 'translator> ModuleBuilder<'env, 'translator> {
    pub fn translate_attributes<T: TName>(
        &mut self,
        attrs: &UniqueMap<T, EA::Attribute>,
    ) -> Vec<Attribute> {
        attrs
            .iter()
            .map(|(_, _, attr)| self.translate_attribute(attr))
            .collect()
    }

    pub fn translate_attribute(&mut self, attr: &EA::Attribute) -> Attribute {
        let node_id = self
            .parent
            .env
            .new_node(self.parent.to_loc(&attr.loc), Type::Tuple(vec![]));
        match &attr.value {
            EA::Attribute_::Name(n) => {
                let sym = self.symbol_pool().make(n.value.as_str());
                Attribute::Apply(node_id, sym, vec![])
            }
            EA::Attribute_::Parameterized(n, vs) => {
                let sym = self.symbol_pool().make(n.value.as_str());
                Attribute::Apply(node_id, sym, self.translate_attributes(vs))
            }
            EA::Attribute_::Assigned(n, v) => {
                let value_node_id = self
                    .parent
                    .env
                    .new_node(self.parent.to_loc(&v.loc), Type::Tuple(vec![]));
                let v = match &v.value {
                    EA::AttributeValue_::Value(val) => {
                        let val =
                            if let Some((val, _)) = ExpTranslator::new(self).translate_value(val) {
                                val
                            } else {
                                // Error reported
                                Value::Bool(false)
                            };
                        AttributeValue::Value(value_node_id, val)
                    }
                    EA::AttributeValue_::Address(a) => {
                        let val = move_ir_types::location::sp(v.loc, EA::Value_::Address(*a));
                        let val = if let Some((val, _)) =
                            ExpTranslator::new(self).translate_value(&val)
                        {
                            val
                        } else {
                            // Error reported
                            Value::Bool(false)
                        };
                        AttributeValue::Value(value_node_id, val)
                    }
                    EA::AttributeValue_::Module(mident) => {
                        let addr_bytes = self.parent.resolve_address(
                            &self.parent.to_loc(&mident.loc),
                            &mident.value.address,
                        );
                        let module_name = ModuleName::from_address_bytes_and_name(
                            addr_bytes,
                            self.symbol_pool()
                                .make(mident.value.module.0.value.as_str()),
                        );
                        // TODO support module attributes more than via empty string
                        AttributeValue::Name(
                            value_node_id,
                            Some(module_name),
                            self.symbol_pool().make(""),
                        )
                    }
                    EA::AttributeValue_::ModuleAccess(macc) => match macc.value {
                        EA::ModuleAccess_::Name(n) => AttributeValue::Name(
                            value_node_id,
                            None,
                            self.symbol_pool().make(n.value.as_str()),
                        ),
                        EA::ModuleAccess_::ModuleAccess(mident, n) => {
                            let addr_bytes = self.parent.resolve_address(
                                &self.parent.to_loc(&macc.loc),
                                &mident.value.address,
                            );
                            let module_name = ModuleName::from_address_bytes_and_name(
                                addr_bytes,
                                self.symbol_pool()
                                    .make(mident.value.module.0.value.as_str()),
                            );
                            AttributeValue::Name(
                                value_node_id,
                                Some(module_name),
                                self.symbol_pool().make(n.value.as_str()),
                            )
                        }
                        EA::ModuleAccess_::Variant(..) => {
                            panic!("Variants are not supported by move model.")
                        }
                    },
                };
                Attribute::Assign(node_id, self.symbol_pool().make(n.value.as_str()), v)
            }
        }
    }
}

/// # Declaration Analysis

impl<'env, 'translator> ModuleBuilder<'env, 'translator> {
    fn decl_ana(
        &mut self,
        module_def: &EA::ModuleDefinition,
        compiled_module: &CompiledModule,
        source_map: &SourceMap,
    ) {
        for (name, struct_def) in module_def.structs.key_cloned_iter() {
            self.decl_ana_struct(&name, struct_def);
        }
        for (name, enum_def) in module_def.enums.key_cloned_iter() {
            self.decl_ana_enum(&name, enum_def);
        }
        for (name, fun_def) in module_def.functions.key_cloned_iter() {
            if fun_def.macro_.is_none() {
                self.decl_ana_fun(&name, fun_def);
            }
        }
        for (name, const_def) in module_def.constants.key_cloned_iter() {
            self.decl_ana_const(&name, const_def, compiled_module, source_map);
        }
    }

    fn decl_ana_const(
        &mut self,
        name: &PA::ConstantName,
        def: &EA::Constant,
        compiled_module: &CompiledModule,
        source_map: &SourceMap,
    ) {
        let qsym = self.qualified_by_module_from_name(&name.0);
        let name = qsym.symbol;
        let const_name = ConstantName(self.symbol_pool().string(name).to_string().into());
        let const_idx = source_map
            .constant_map
            .get(&const_name)
            .expect("constant not in source map");
        let move_value =
            Constant::deserialize_constant(&compiled_module.constant_pool()[*const_idx as usize])
                .unwrap();
        let attributes = self.translate_attributes(&def.attributes);
        let mut et = ExpTranslator::new(self);
        let loc = et.to_loc(&def.loc);
        let ty = et.translate_type(&def.signature);
        let value = et.translate_from_move_value(&loc, &ty, &move_value);
        et.parent.parent.define_const(
            qsym,
            ConstEntry {
                loc,
                ty,
                value,
                attributes,
            },
        );
    }

    fn decl_ana_struct(&mut self, name: &PA::DatatypeName, def: &EA::StructDefinition) {
        let qsym = self.qualified_by_module_from_name(&name.0);
        let struct_id = DatatypeId::new(qsym.symbol);
        let attrs = self.translate_attributes(&def.attributes);
        let mut et = ExpTranslator::new(self);
        let type_params =
            et.analyze_and_add_type_params(def.type_parameters.iter().map(|param| &param.name));
        et.parent.parent.define_struct(
            et.to_loc(&def.loc),
            attrs,
            qsym,
            et.parent.module_id,
            struct_id,
            type_params,
            None, // will be filled in during definition analysis
        );
    }

    fn decl_ana_enum(&mut self, name: &PA::DatatypeName, def: &EA::EnumDefinition) {
        let qsym = self.qualified_by_module_from_name(&name.0);
        let struct_id = DatatypeId::new(qsym.symbol);
        let attrs = self.translate_attributes(&def.attributes);
        let mut et = ExpTranslator::new(self);
        let type_params =
            et.analyze_and_add_type_params(def.type_parameters.iter().map(|param| &param.name));
        et.parent.parent.define_enum(
            et.to_loc(&def.loc),
            attrs,
            qsym,
            et.parent.module_id,
            struct_id,
            type_params,
            BTreeMap::new(), // will be filled in during definition analysis
        );
    }

    fn decl_ana_fun(&mut self, name: &PA::FunctionName, def: &EA::Function) {
        let qsym = self.qualified_by_module_from_name(&name.0);
        let attrs = self.translate_attributes(&def.attributes);
        let mut et = ExpTranslator::new(self);
        et.enter_scope();
        let type_params = et.analyze_and_add_type_params(
            def.signature.type_parameters.iter().map(|(name, _)| name),
        );
        et.enter_scope();
        let params = et.analyze_and_add_params(&def.signature.parameters);
        let _result_type = et.translate_type(&def.signature.return_type);
        let loc = et.to_loc(&def.loc);
        et.parent.parent.define_fun(
            loc.clone(),
            attrs,
            qsym.clone(),
            type_params.clone(),
            params.clone(),
        );
    }
}

/// # Definition Analysis

impl<'env, 'translator> ModuleBuilder<'env, 'translator> {
    fn def_ana(&mut self, module_def: &EA::ModuleDefinition) {
        // Analyze all structs.
        for (name, def) in module_def.structs.key_cloned_iter() {
            self.def_ana_struct(&name, def);
        }

        // Analyze all enums.
        for (name, def) in module_def.enums.key_cloned_iter() {
            self.def_ana_enum(&name, def);
        }

        // Analyze all functions.
        for (name, fun_def) in module_def.functions.key_cloned_iter() {
            if fun_def.macro_.is_none() {
                self.def_ana_fun(&name, &fun_def.body);
            }
        }

        // Propagate the impurity of functions: a Move function which calls an
        // impure Move function is also considered impure.
        for (name, f) in module_def.functions.key_cloned_iter() {
            if f.macro_.is_none() {
                self.qualified_by_module_from_name(&name.0);
            }
        }
    }
}

/// ## Struct and Enum Definition Analysis

impl<'env, 'translator> ModuleBuilder<'env, 'translator> {
    fn def_ana_struct(&mut self, name: &PA::DatatypeName, def: &EA::StructDefinition) {
        let qsym = self.qualified_by_module_from_name(&name.0);
        let type_params = self
            .parent
            .datatype_table
            .get(&qsym)
            .expect("struct invalid")
            .type_params
            .clone();
        let mut et = ExpTranslator::new(self);
        let loc = et.to_loc(&name.0.loc);
        for (name, ty) in type_params {
            et.define_type_param(&loc, name, ty);
        }
        let fields = match &def.fields {
            EA::StructFields::Named(fields) => {
                let mut field_map = BTreeMap::new();
                for (_name_loc, field_name_, (idx, (_, ty))) in fields {
                    let field_sym = et.symbol_pool().make(field_name_);
                    let field_ty = et.translate_type(ty);
                    field_map.insert(field_sym, (*idx, field_ty));
                }
                Some(field_map)
            }
            EA::StructFields::Positional(tys) => {
                let mut field_map = BTreeMap::new();
                for (idx, (_, ty)) in tys.iter().enumerate() {
                    let field_name_ = format!("{idx}");
                    let field_sym = et.symbol_pool().make(&field_name_);
                    let field_ty = et.translate_type(ty);
                    field_map.insert(field_sym, (idx, field_ty));
                }
                Some(field_map)
            }
            EA::StructFields::Native(_) => None,
        };
        self.parent
            .datatype_table
            .get_mut(&qsym)
            .expect("struct invalid")
            .data = DatatypeData::Struct { fields };
    }

    fn def_ana_enum(&mut self, name: &PA::DatatypeName, def: &EA::EnumDefinition) {
        let qsym = self.qualified_by_module_from_name(&name.0);
        let type_params = self
            .parent
            .datatype_table
            .get(&qsym)
            .expect("enum invalid")
            .type_params
            .clone();
        let mut et = ExpTranslator::new(self);
        let loc = et.to_loc(&name.0.loc);
        for (name, ty) in type_params {
            et.define_type_param(&loc, name, ty);
        }
        let variants: BTreeMap<_, _> = def
            .variants
            .key_cloned_iter()
            .map(|(key, variant)| {
                let variant_name = et.symbol_pool().make(&key.0.value);
                let variant_fields = match &variant.fields {
                    EA::VariantFields::Named(fields) => {
                        let mut field_map = BTreeMap::new();
                        for (_name_loc, field_name_, (idx, (_, ty))) in fields {
                            let field_sym = et.symbol_pool().make(field_name_);
                            let field_ty = et.translate_type(ty);
                            field_map.insert(field_sym, (*idx, field_ty));
                        }
                        Some(field_map)
                    }
                    EA::VariantFields::Positional(tys) => {
                        let mut field_map = BTreeMap::new();
                        for (idx, (_, ty)) in tys.iter().enumerate() {
                            let field_name_ = format!("{idx}");
                            let field_sym = et.symbol_pool().make(&field_name_);
                            let field_ty = et.translate_type(ty);
                            field_map.insert(field_sym, (idx, field_ty));
                        }
                        Some(field_map)
                    }
                    EA::VariantFields::Empty => None,
                };
                (variant_name, variant_fields)
            })
            .collect();
        self.parent
            .datatype_table
            .get_mut(&qsym)
            .expect("enum invalid")
            .data = DatatypeData::Enum { variants };
    }
}

/// ## Move Function Definition Analysis

impl<'env, 'translator> ModuleBuilder<'env, 'translator> {
    /// Definition analysis for Move functions.
    /// If the function is pure, we translate its body.
    fn def_ana_fun(&mut self, name: &PA::FunctionName, body: &EA::FunctionBody) {
        if let EA::FunctionBody_::Defined(_) = &body.value {
            let full_name = self.qualified_by_module_from_name(&name.0);
            let entry = self
                .parent
                .fun_table
                .get(&full_name)
                .expect("function defined");
            let type_params = entry.type_params.clone();
            let params = entry.params.clone();
            let mut et = ExpTranslator::new(self);
            let loc = et.to_loc(&body.loc);
            for (n, ty) in &type_params {
                et.define_type_param(&loc, *n, ty.clone());
            }
            et.enter_scope();
            for (n, _) in params.iter() {
                et.define_local(&loc, *n);
            }
            et.finalize_types();
        }
    }
}

/// # Environment Population

impl<'env, 'translator> ModuleBuilder<'env, 'translator> {
    fn populate_env_from_result(
        &mut self,
        loc: Loc,
        attributes: Vec<Attribute>,
        module: CompiledModule,
        source_map: SourceMap,
    ) {
        let struct_data: BTreeMap<DatatypeId, StructData> = (0..module.struct_defs().len())
            .filter_map(|idx| {
                let def_idx = StructDefinitionIndex(idx as u16);
                let handle_idx = module.struct_def_at(def_idx).struct_handle;
                let handle = module.datatype_handle_at(handle_idx);
                let name = self.symbol_pool().make(module.identifier_at(handle.name).as_str());
                if let Some(entry) = self
                    .parent
                    .datatype_table
                    .get(&self.qualified_by_module(name))
                {
                    Some((
                        DatatypeId::new(name),
                        self.parent.env.create_move_struct_data(
                            &module,
                            def_idx,
                            name,
                            entry.loc.clone(),
                            entry.attributes.clone(),
                        ),
                    ))
                } else {
                    self.parent.error(
                        &self.parent.env.internal_loc(),
                        &format!("[internal] bytecode does not match AST: `{}` in bytecode but not in AST", name.display(self.symbol_pool())));
                    None
                }
            })
            .collect();
        let enum_data: BTreeMap<DatatypeId, EnumData> = (0..module.enum_defs().len())
            .filter_map(|idx| {
                let def_idx = EnumDefinitionIndex(idx as u16);
                let handle_idx = module.enum_def_at(def_idx).enum_handle;
                let handle = module.datatype_handle_at(handle_idx);
                let name = self.symbol_pool().make(module.identifier_at(handle.name).as_str());
                if let Some(entry) = self
                    .parent
                    .datatype_table
                    .get(&self.qualified_by_module(name))
                {
                    Some((
                        DatatypeId::new(name),
                        self.parent.env.create_move_enum_data(
                            &module,
                            def_idx,
                            name,
                            entry.loc.clone(),
                            Some(&source_map),
                            entry.attributes.clone(),
                        ),
                    ))
                } else {
                    self.parent.error(
                        &self.parent.env.internal_loc(),
                        &format!("[internal] bytecode does not match AST: `{}` in bytecode but not in AST", name.display(self.symbol_pool())));
                    None
                }
            })
            .collect();
        let function_data: BTreeMap<FunId, FunctionData> = (0..module.function_defs().len())
            .filter_map(|idx| {
                let def_idx = FunctionDefinitionIndex(idx as u16);
                let handle_idx = module.function_def_at(def_idx).function;
                let handle = module.function_handle_at(handle_idx);
                let name_str = module.identifier_at(handle.name).as_str();
                let name = if name_str == SCRIPT_BYTECODE_FUN_NAME {
                    // This is a pseudo script module, which has exactly one function. Determine
                    // the name of this function.
                    self.parent.fun_table.iter().find_map(|(k, _)| {
                        if k.module_name == self.module_name
                        { Some(k.symbol) } else { None }
                    }).expect("unexpected script with multiple or no functions")
                } else {
                    self.symbol_pool().make(name_str)
                };
                if let Some(entry) = self.parent.fun_table.get(&self.qualified_by_module(name)) {
                    let arg_names = project_1st(&entry.params);
                    let type_arg_names = project_1st(&entry.type_params);
                    Some((FunId::new(name), self.parent.env.create_function_data(
                        &module,
                        def_idx,
                        name,
                        entry.loc.clone(),
                        entry.attributes.clone(),
                        arg_names,
                        type_arg_names,
                    )))
                } else {
                    let funs = self.parent.fun_table.keys().map(|k| {
                        format!("{}", k.display_full(self.symbol_pool()))
                    }).join(", ");
                    self.parent.error(
                        &self.parent.env.internal_loc(),
                        &format!("[internal] bytecode does not match AST: `{}` in bytecode but not in AST (available in AST: {})", name.display(self.symbol_pool()), funs));
                    None
                }
            })
            .collect();
        let named_constants: BTreeMap<NamedConstantId, NamedConstantData> = self
            .parent
            .const_table
            .iter()
            .filter(|(name, _)| name.module_name == self.module_name)
            .map(|(name, const_entry)| {
                let ConstEntry {
                    loc,
                    value,
                    ty,
                    attributes,
                } = const_entry.clone();
                (
                    NamedConstantId::new(name.symbol),
                    self.parent.env.create_named_constant_data(
                        name.symbol,
                        loc,
                        ty,
                        value,
                        attributes,
                    ),
                )
            })
            .collect();
        self.parent.env.add(
            loc,
            attributes,
            module,
            source_map,
            named_constants,
            struct_data,
            enum_data,
            function_data,
        );
    }
}
