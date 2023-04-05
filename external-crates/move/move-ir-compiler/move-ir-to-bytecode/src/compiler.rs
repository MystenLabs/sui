// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::context::{CompiledDependency, Context, MaterializedPools, TABLE_MAX_SIZE};
use anyhow::{bail, format_err, Result};
use move_binary_format::{
    file_format::{
        Ability, AbilitySet, Bytecode, CodeOffset, CodeUnit, CompiledModule, CompiledScript,
        Constant, FieldDefinition, FunctionDefinition, FunctionSignature, ModuleHandle, Signature,
        SignatureToken, StructDefinition, StructDefinitionIndex, StructFieldInformation,
        StructHandleIndex, StructTypeParameter, TableIndex, TypeParameterIndex, TypeSignature,
        Visibility,
    },
    file_format_common::VERSION_MAX,
};
use move_bytecode_source_map::source_map::SourceMap;
use move_core_types::value::{MoveTypeLayout, MoveValue};
use move_ir_types::{
    ast::{self, Bytecode as IRBytecode, Bytecode_ as IRBytecode_, *},
    sp,
};
use move_symbol_pool::Symbol;
use std::{
    clone::Clone,
    collections::{
        hash_map::Entry::{Occupied, Vacant},
        BTreeSet, HashMap, HashSet,
    },
    fmt::Write,
};

macro_rules! record_src_loc {
    (local: $context:expr, $var:expr) => {{
        let source_name = ($var.value.0.as_str().to_owned(), $var.loc);
        $context
            .source_map
            .add_local_mapping($context.current_function_definition_index(), source_name)?;
    }};
    (parameter: $context:expr, $var:expr) => {{
        let source_name = ($var.value.0.as_str().to_owned(), $var.loc);
        $context
            .source_map
            .add_parameter_mapping($context.current_function_definition_index(), source_name)?;
    }};
    (field: $context:expr, $idx: expr, $field:expr) => {{
        $context
            .source_map
            .add_struct_field_mapping($idx, $field.loc)?;
    }};
    (function_type_formals: $context:expr, $var:expr) => {
        for (ty_var, _) in $var.iter() {
            let source_name = (ty_var.value.0.as_str().to_owned(), ty_var.loc);
            $context.source_map.add_function_type_parameter_mapping(
                $context.current_function_definition_index(),
                source_name,
            )?;
        }
    };
    (function_decl: $context:expr, $location:expr, $function_index:expr, $is_native:expr) => {{
        $context.set_function_index($function_index as TableIndex);
        $context.source_map.add_top_level_function_mapping(
            $context.current_function_definition_index(),
            $location,
            $is_native,
        )?;
    }};
    (struct_type_formals: $context:expr, $var:expr) => {
        for (_, ty_var, _) in $var.iter() {
            let source_name = (ty_var.value.0.as_str().to_owned(), ty_var.loc);
            $context.source_map.add_struct_type_parameter_mapping(
                $context.current_struct_definition_index(),
                source_name,
            )?;
        }
    };
    (struct_decl: $context:expr, $location:expr) => {
        $context
            .source_map
            .add_top_level_struct_mapping($context.current_struct_definition_index(), $location)?;
    };
    (const_decl: $context:expr, $const_index:expr, $name:expr) => {
        $context.source_map.add_const_mapping($const_index, $name)?;
    };
}

macro_rules! make_push_instr {
    ($context:ident, $code:ident) => {
        macro_rules! push_instr {
            ($loc:expr, $instr:expr) => {{
                let code_offset = $code.len() as CodeOffset;
                $context.source_map.add_code_mapping(
                    $context.current_function_definition_index(),
                    code_offset,
                    $loc,
                )?;
                $code.push($instr);
            }};
        }
    };
}

macro_rules! make_record_nop_label {
    ($context:ident, $code:ident) => {
        macro_rules! record_nop_label {
            ($label:expr) => {{
                let code_offset = $code.len() as CodeOffset;
                $context.source_map.add_nop_mapping(
                    $context.current_function_definition_index(),
                    $label,
                    code_offset,
                )?;
            }};
        }
    };
}

// Holds information about a function being compiled.
#[derive(Debug)]
struct FunctionFrame {
    locals: HashMap<Var_, u8>,
    local_types: Signature,
    // i64 to allow the bytecode verifier to catch errors of
    // - negative stack sizes
    // - excessivley large stack sizes
    // The max stack depth of the file_format is set as u16.
    // Theoretically, we could use a BigInt here, but that is probably overkill for any testing
    max_stack_depth: i64,
    cur_stack_depth: i64,
    type_parameters: HashMap<TypeVar_, TypeParameterIndex>,
}

impl FunctionFrame {
    fn new(type_parameters: HashMap<TypeVar_, TypeParameterIndex>) -> FunctionFrame {
        FunctionFrame {
            locals: HashMap::new(),
            local_types: Signature(vec![]),
            max_stack_depth: 0,
            cur_stack_depth: 0,
            type_parameters,
        }
    }

    // Manage the stack info for the function
    fn push(&mut self) -> Result<()> {
        if self.cur_stack_depth == i64::MAX {
            bail!("ICE Stack depth accounting overflow. The compiler can only support a maximum stack depth of up to i64::max_value")
        }
        self.cur_stack_depth += 1;
        self.max_stack_depth = std::cmp::max(self.max_stack_depth, self.cur_stack_depth);
        Ok(())
    }

    fn pop(&mut self) -> Result<()> {
        if self.cur_stack_depth == i64::MIN {
            bail!("ICE Stack depth accounting underflow. The compiler can only support a minimum stack depth of up to i64::min_value")
        }
        self.cur_stack_depth -= 1;
        Ok(())
    }

    fn get_local(&self, var: &Var_) -> Result<u8> {
        match self.locals.get(var) {
            None => bail!("variable {} undefined", var),
            Some(idx) => Ok(*idx),
        }
    }

    fn define_local(&mut self, var: &Var_, type_: SignatureToken) -> Result<u8> {
        if self.locals.len() >= TABLE_MAX_SIZE {
            bail!("Max number of locals reached");
        }

        let cur_loc_idx = self.locals.len() as u8;
        let loc = var.clone();
        let entry = self.locals.entry(loc);
        match entry {
            Occupied(_) => bail!("variable redefinition {}", var),
            Vacant(e) => {
                e.insert(cur_loc_idx);
                self.local_types.0.push(type_);
            }
        }
        Ok(cur_loc_idx)
    }

    fn type_parameters(&self) -> &HashMap<TypeVar_, TypeParameterIndex> {
        &self.type_parameters
    }
}

// Returns an error that lists any labels that have been redeclared, or used without being declared.
fn label_verification_error(
    redeclared: &[&BlockLabel_],
    undeclared: &[&BlockLabel_],
) -> Result<()> {
    let mut message = "Invalid block labels".to_string();
    if !redeclared.is_empty() {
        write!(
            &mut message,
            ", labels were declared twice ({})",
            redeclared
                .iter()
                .map(|l| l.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )?;
    }
    if !undeclared.is_empty() {
        write!(
            &mut message,
            ", labels were used without being declared ({})",
            undeclared
                .iter()
                .map(|l| l.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )?;
    }
    bail!(message);
}

fn verify_move_function_body(code: &[Block]) -> Result<()> {
    let mut labels = HashSet::new();
    let mut redeclared = vec![];
    for block in code {
        let label = &block.value.label.value;
        if labels.contains(&label) {
            redeclared.push(label);
        } else {
            labels.insert(label);
        }
    }

    let mut undeclared = vec![];
    for block in code {
        for statement in &block.value.statements {
            match &statement.value {
                Statement_::Jump(label)
                | Statement_::JumpIf(_, label)
                | Statement_::JumpIfFalse(_, label) => {
                    if !labels.contains(&label.value) {
                        undeclared.push(&label.value);
                    }
                }
                _ => {}
            }
        }
    }

    if redeclared.is_empty() && undeclared.is_empty() {
        Ok(())
    } else {
        label_verification_error(&redeclared, &undeclared)
    }
}

fn verify_bytecode_function_body(code: &[(BlockLabel_, BytecodeBlock)]) -> Result<()> {
    let mut labels = HashSet::new();
    let mut redeclared = vec![];
    for block in code {
        let label = &block.0;
        if labels.contains(&label) {
            redeclared.push(label);
        } else {
            labels.insert(label);
        }
    }

    let mut undeclared = vec![];
    for block in code {
        for statement in &block.1 {
            match &statement.value {
                IRBytecode_::Branch(label)
                | IRBytecode_::BrTrue(label)
                | IRBytecode_::BrFalse(label) => {
                    if !labels.contains(&label) {
                        undeclared.push(label);
                    }
                }
                _ => {}
            }
        }
    }

    if redeclared.is_empty() && undeclared.is_empty() {
        Ok(())
    } else {
        label_verification_error(&redeclared, &undeclared)
    }
}

/// Verify that, within a single function, no two blocks use the same label, and all jump statements
/// specify a destination label that exists on some block. If any block labels or statements don't
/// meet these conditions, return an error.
fn verify_function(function: &Function) -> Result<()> {
    match &function.value.body {
        FunctionBody::Move { code, .. } => verify_move_function_body(code),
        FunctionBody::Bytecode { code, .. } => verify_bytecode_function_body(code),
        _ => Ok(()),
    }
}

/// Verifies that the given module is semantically valid. Invoking this prior to compiling the
/// module to bytecode may help diagnose malformed programs.
fn verify_module(module: &ModuleDefinition) -> Result<()> {
    for function in &module.functions {
        verify_function(&function.1)?;
    }
    Ok(())
}

/// Verifies that the given script is semantically valid. Invoking this prior to compiling the
/// script to bytecode may help diagnose malformed programs.
fn verify_script(script: &Script) -> Result<()> {
    verify_function(&script.main)
}

/// Compile a transaction script.
pub fn compile_script<'a>(
    script: Script,
    dependencies: impl IntoIterator<Item = &'a CompiledModule>,
) -> Result<(CompiledScript, SourceMap)> {
    verify_script(&script)?;

    let mut context = Context::new(script.loc, HashMap::new(), None)?;
    for dep in dependencies {
        context.add_compiled_dependency(dep)?;
    }

    compile_imports(&mut context, script.imports.clone())?;
    // Add explicit handles/dependency declarations to `dependencies`
    compile_explicit_dependency_declarations(
        &mut context,
        script.imports,
        script.explicit_dependency_declarations,
    )?;

    for ir_constant in script.constants {
        let constant = compile_constant(&mut context, ir_constant.signature, ir_constant.value)?;
        context.declare_constant(ir_constant.name.clone(), constant.clone())?;
        let const_idx = context.constant_index(constant)?;
        record_src_loc!(const_decl: context, const_idx, ir_constant.name);
    }

    let function = script.main;

    let sig = function_signature(&mut context, &function.value.signature)?;
    let parameters_sig_idx = context.signature_index(Signature(sig.parameters))?;

    record_src_loc!(
        function_decl: context,
        function.loc,
        0,
        matches!(function.value.body, FunctionBody::Native)
    );
    record_src_loc!(
        function_type_formals: context,
        &function.value.signature.type_formals
    );
    let code = compile_function_body_impl(&mut context, function.value)?.unwrap();

    let (
        MaterializedPools {
            module_handles,
            struct_handles,
            function_handles,
            signatures,
            identifiers,
            address_identifiers,
            constant_pool,
            function_instantiations,
            ..
        },
        _compiled_deps,
        source_map,
    ) = context.materialize_pools();
    let script = CompiledScript {
        version: VERSION_MAX,
        module_handles,
        struct_handles,
        function_handles,
        function_instantiations,
        signatures,
        identifiers,
        address_identifiers,
        constant_pool,
        metadata: vec![],

        type_parameters: sig.type_parameters,
        parameters: parameters_sig_idx,
        code,
    };
    Ok((script, source_map))
}

/// Compile a module.
pub fn compile_module<'a>(
    module: ModuleDefinition,
    dependencies: impl IntoIterator<Item = &'a CompiledModule>,
) -> Result<(CompiledModule, SourceMap)> {
    verify_module(&module)?;

    let current_module = module.identifier;
    let mut context = Context::new(module.loc, HashMap::new(), Some(current_module))?;
    for dep in dependencies {
        context.add_compiled_dependency(dep)?;
    }

    // Compile friends
    let friend_decls = compile_friends(&mut context, module.friends)?;

    // Compile imports
    let self_name = ModuleName::module_self();
    let self_module_handle_idx = context.declare_import(current_module, self_name)?;
    // Explicitly declare all imports as they will be included even if not used
    compile_imports(&mut context, module.imports.clone())?;

    // Add explicit handles/dependency declarations to `dependencies`
    compile_explicit_dependency_declarations(
        &mut context,
        module.imports,
        module.explicit_dependency_declarations,
    )?;

    // Explicitly declare all structs as they will be included even if not used
    for s in &module.structs {
        let abilities = abilities(&s.value.abilities);
        let ident = QualifiedStructIdent {
            module: self_name,
            name: s.value.name.clone(),
        };
        let type_parameters = struct_type_parameters(&s.value.type_formals);
        context.declare_struct_handle_index(ident, abilities, type_parameters)?;
    }

    for ir_constant in module.constants {
        let constant = compile_constant(&mut context, ir_constant.signature, ir_constant.value)?;
        context.declare_constant(ir_constant.name.clone(), constant.clone())?;
        let const_idx = context.constant_index(constant)?;
        record_src_loc!(const_decl: context, const_idx, ir_constant.name);
    }

    for (name, function) in &module.functions {
        let sig = function_signature(&mut context, &function.value.signature)?;
        context.declare_function(self_name, name.clone(), sig)?;
    }

    // Compile definitions
    let struct_defs = compile_structs(&mut context, &self_name, module.structs)?;
    let function_defs = compile_functions(&mut context, &self_name, module.functions)?;

    let (
        MaterializedPools {
            module_handles,
            struct_handles,
            function_handles,
            field_handles,
            signatures,
            identifiers,
            address_identifiers,
            constant_pool,
            function_instantiations,
            struct_def_instantiations,
            field_instantiations,
        },
        _compiled_deps,
        source_map,
    ) = context.materialize_pools();
    let module = CompiledModule {
        version: VERSION_MAX,
        module_handles,
        self_module_handle_idx,
        struct_handles,
        function_handles,
        field_handles,
        friend_decls,
        struct_def_instantiations,
        function_instantiations,
        field_instantiations,
        signatures,
        identifiers,
        address_identifiers,
        constant_pool,
        metadata: vec![],
        struct_defs,
        function_defs,
    };
    Ok((module, source_map))
}

// Note: DO NOT try to recover from this function as it zeros out the `outer_contexts` dependencies
// and sets them after a successful result
// Any `Error` should stop compilation in the caller
fn compile_explicit_dependency_declarations(
    outer_context: &mut Context,
    imports: Vec<ImportDefinition>,
    dependencies: Vec<ModuleDependency>,
) -> Result<()> {
    let mut dependencies_acc = outer_context.take_dependencies();
    for dependency in dependencies {
        let ModuleDependency {
            name: mname,
            structs,
            functions,
        } = dependency;
        let current_module = outer_context.module_ident(&mname)?;
        let mut context = Context::new(
            outer_context.decl_location(),
            dependencies_acc,
            Some(*current_module),
        )?;
        compile_imports(&mut context, imports.clone())?;
        let self_module_handle_idx = context.module_handle_index(&mname)?;
        for struct_dep in structs {
            let StructDependency {
                abilities: abs,
                name,
                type_formals: tys,
            } = struct_dep;
            let sname = QualifiedStructIdent::new(mname, name);
            let ability_set = abilities(&abs);
            let kinds = struct_type_parameters(&tys);
            context.declare_struct_handle_index(sname, ability_set, kinds)?;
        }
        for function_dep in functions {
            let FunctionDependency { name, signature } = function_dep;
            let sig = function_signature(&mut context, &signature)?;
            context.declare_function(mname, name, sig)?;
        }

        let (
            MaterializedPools {
                module_handles,
                struct_handles,
                function_handles,
                field_handles,
                signatures,
                identifiers,
                address_identifiers,
                constant_pool,
                function_instantiations,
                struct_def_instantiations,
                field_instantiations,
            },
            compiled_deps,
            _source_map,
        ) = context.materialize_pools();
        let compiled_module = CompiledModule {
            version: VERSION_MAX,
            module_handles,
            self_module_handle_idx,
            struct_handles,
            function_handles,
            field_handles,
            friend_decls: vec![],
            struct_def_instantiations,
            function_instantiations,
            field_instantiations,
            signatures,
            identifiers,
            address_identifiers,
            constant_pool,
            metadata: vec![],
            struct_defs: vec![],
            function_defs: vec![],
        };
        dependencies_acc = compiled_deps;
        dependencies_acc.insert(
            *current_module,
            CompiledDependency::stored(compiled_module)?,
        );
    }
    outer_context.restore_dependencies(dependencies_acc);
    Ok(())
}

fn compile_friends(
    context: &mut Context,
    friends: Vec<ast::ModuleIdent>,
) -> Result<Vec<ModuleHandle>> {
    let mut friend_decls = vec![];
    for friend in friends {
        friend_decls.push(context.declare_friend(friend)?);
    }
    Ok(friend_decls)
}

fn compile_imports(context: &mut Context, imports: Vec<ImportDefinition>) -> Result<()> {
    Ok(for import in imports {
        context.declare_import(import.ident, import.alias)?;
    })
}

fn type_parameter_indexes<'a>(
    ast_tys: impl IntoIterator<Item = &'a TypeVar>,
) -> Result<HashMap<TypeVar_, TypeParameterIndex>> {
    let mut m = HashMap::new();
    for (idx, ty_var) in ast_tys.into_iter().enumerate() {
        if idx > TABLE_MAX_SIZE {
            bail!("Too many type parameters")
        }
        let old = m.insert(ty_var.value.clone(), idx as TypeParameterIndex);
        if old.is_some() {
            bail!("Type formal '{}'' already bound", ty_var)
        }
    }
    Ok(m)
}

fn struct_type_parameters(ast_tys: &[ast::StructTypeParameter]) -> Vec<StructTypeParameter> {
    ast_tys
        .iter()
        .map(|(is_phantom, _, abs)| StructTypeParameter {
            constraints: abilities(abs),
            is_phantom: *is_phantom,
        })
        .collect()
}

fn abilities(abilities: &BTreeSet<ast::Ability>) -> AbilitySet {
    abilities
        .iter()
        .map(ability)
        .fold(AbilitySet::EMPTY, |acc, a| acc | a)
}

fn ability(ab: &ast::Ability) -> Ability {
    match ab {
        ast::Ability::Copy => Ability::Copy,
        ast::Ability::Drop => Ability::Drop,
        ast::Ability::Store => Ability::Store,
        ast::Ability::Key => Ability::Key,
    }
}

fn compile_types(
    context: &mut Context,
    type_parameters: &HashMap<TypeVar_, TypeParameterIndex>,
    tys: &[Type],
) -> Result<Vec<SignatureToken>> {
    tys.iter()
        .map(|ty| compile_type(context, type_parameters, ty))
        .collect::<Result<_>>()
}

fn compile_type(
    context: &mut Context,
    type_parameters: &HashMap<TypeVar_, TypeParameterIndex>,
    ty: &Type,
) -> Result<SignatureToken> {
    Ok(match ty {
        Type::Address => SignatureToken::Address,
        Type::Signer => SignatureToken::Signer,
        Type::U8 => SignatureToken::U8,
        Type::U16 => SignatureToken::U16,
        Type::U32 => SignatureToken::U32,
        Type::U64 => SignatureToken::U64,
        Type::U128 => SignatureToken::U128,
        Type::U256 => SignatureToken::U256,
        Type::Bool => SignatureToken::Bool,
        Type::Vector(inner_type) => SignatureToken::Vector(Box::new(compile_type(
            context,
            type_parameters,
            inner_type,
        )?)),
        Type::Reference(is_mutable, inner_type) => {
            let inner_token = Box::new(compile_type(context, type_parameters, inner_type)?);
            if *is_mutable {
                SignatureToken::MutableReference(inner_token)
            } else {
                SignatureToken::Reference(inner_token)
            }
        }
        Type::Struct(ident, tys) => {
            let sh_idx = context.struct_handle_index(ident.clone())?;

            if tys.is_empty() {
                SignatureToken::Struct(sh_idx)
            } else {
                let tokens = compile_types(context, type_parameters, tys)?;
                SignatureToken::StructInstantiation(sh_idx, tokens)
            }
        }
        Type::TypeParameter(ty_var) => {
            let idx = match type_parameters.get(ty_var) {
                None => bail!("Unbound type parameter {}", ty_var),
                Some(idx) => *idx,
            };
            SignatureToken::TypeParameter(idx)
        }
    })
}

fn function_signature(
    context: &mut Context,
    f: &ast::FunctionSignature,
) -> Result<FunctionSignature> {
    let m = type_parameter_indexes(f.type_formals.iter().map(|formal| &formal.0))?;
    let return_ = compile_types(context, &m, &f.return_type)?;
    let parameters = f
        .formals
        .iter()
        .map(|(_, ty)| compile_type(context, &m, ty))
        .collect::<Result<_>>()?;
    let type_parameters = f
        .type_formals
        .iter()
        .map(|(_, abs)| abilities(abs))
        .collect();
    Ok(move_binary_format::file_format::FunctionSignature {
        return_,
        parameters,
        type_parameters,
    })
}

fn compile_structs(
    context: &mut Context,
    self_name: &ModuleName,
    structs: Vec<ast::StructDefinition>,
) -> Result<Vec<StructDefinition>> {
    let mut struct_defs = vec![];
    for s in structs {
        let sident = QualifiedStructIdent {
            module: *self_name,
            name: s.value.name.clone(),
        };
        let sh_idx = context.struct_handle_index(sident.clone())?;
        record_src_loc!(struct_decl: context, s.loc);
        record_src_loc!(struct_type_formals: context, &s.value.type_formals);
        let m = type_parameter_indexes(s.value.type_formals.iter().map(|formal| &formal.1))?;
        let sd_idx = context.declare_struct_definition_index(s.value.name)?;
        let field_information = compile_fields(context, &m, sh_idx, sd_idx, s.value.fields)?;
        struct_defs.push(StructDefinition {
            struct_handle: sh_idx,
            field_information,
        });
    }
    Ok(struct_defs)
}

fn compile_fields(
    context: &mut Context,
    type_parameters: &HashMap<TypeVar_, TypeParameterIndex>,
    sh_idx: StructHandleIndex,
    sd_idx: StructDefinitionIndex,
    sfields: StructDefinitionFields,
) -> Result<StructFieldInformation> {
    Ok(match sfields {
        StructDefinitionFields::Native => StructFieldInformation::Native,
        StructDefinitionFields::Move { fields } => {
            let mut decl_fields = vec![];
            for (decl_order, (f, ty)) in fields.into_iter().enumerate() {
                let name = context.identifier_index(f.value.0)?;
                record_src_loc!(field: context, sd_idx, f);
                let sig_token = compile_type(context, type_parameters, &ty)?;
                context.declare_field(sh_idx, sd_idx, f.value, sig_token.clone(), decl_order);
                decl_fields.push(FieldDefinition {
                    name,
                    signature: TypeSignature(sig_token),
                });
            }
            StructFieldInformation::Declared(decl_fields)
        }
    })
}

fn compile_functions(
    context: &mut Context,
    self_name: &ModuleName,
    functions: Vec<(FunctionName, Function)>,
) -> Result<Vec<FunctionDefinition>> {
    functions
        .into_iter()
        .enumerate()
        .map(|(func_index, (name, ast_function))| {
            compile_function(context, self_name, name, ast_function, func_index)
        })
        .collect()
}

fn compile_function_body_impl(
    context: &mut Context,
    ast_function: Function_,
) -> Result<Option<CodeUnit>> {
    Ok(match ast_function.body {
        FunctionBody::Move { locals, code } => {
            let m = type_parameter_indexes(
                ast_function
                    .signature
                    .type_formals
                    .iter()
                    .map(|formal| &formal.0),
            )?;
            Some(compile_function_body(
                context,
                m,
                ast_function.signature.formals,
                locals,
                code,
            )?)
        }
        FunctionBody::Bytecode { locals, code } => {
            let m = type_parameter_indexes(
                ast_function
                    .signature
                    .type_formals
                    .iter()
                    .map(|formal| &formal.0),
            )?;
            Some(compile_function_body_bytecode(
                context,
                m,
                ast_function.signature.formals,
                locals,
                code,
            )?)
        }

        FunctionBody::Native => {
            for (var, _) in ast_function.signature.formals.into_iter() {
                record_src_loc!(parameter: context, var)
            }
            None
        }
    })
}

fn compile_function(
    context: &mut Context,
    self_name: &ModuleName,
    name: FunctionName,
    ast_function: Function,
    function_index: usize,
) -> Result<FunctionDefinition> {
    record_src_loc!(
        function_decl: context,
        ast_function.loc,
        function_index,
        matches!(ast_function.value.body, FunctionBody::Native)
    );
    record_src_loc!(
        function_type_formals: context,
        &ast_function.value.signature.type_formals
    );
    let fh_idx = context.function_handle(*self_name, name)?.1;

    let ast_function = ast_function.value;

    let is_entry = ast_function.is_entry;
    let visibility = match ast_function.visibility {
        FunctionVisibility::Public => Visibility::Public,
        FunctionVisibility::Friend => Visibility::Friend,
        FunctionVisibility::Internal => Visibility::Private,
    };
    let acquires_global_resources = ast_function
        .acquires
        .iter()
        .map(|name| context.struct_definition_index(name))
        .collect::<Result<_>>()?;

    let code = compile_function_body_impl(context, ast_function)?;

    Ok(FunctionDefinition {
        function: fh_idx,
        visibility,
        is_entry,
        acquires_global_resources,
        code,
    })
}

fn compile_function_body(
    context: &mut Context,
    type_parameters: HashMap<TypeVar_, TypeParameterIndex>,
    formals: Vec<(Var, Type)>,
    locals: Vec<(Var, Type)>,
    blocks: Vec<Block>,
) -> Result<CodeUnit> {
    let mut function_frame = FunctionFrame::new(type_parameters);
    for (var, t) in formals {
        let sig = compile_type(context, function_frame.type_parameters(), &t)?;
        function_frame.define_local(&var.value, sig.clone())?;
        record_src_loc!(parameter: context, var);
    }

    let mut locals_signature = Signature(vec![]);
    for (var_, t) in locals {
        let sig = compile_type(context, function_frame.type_parameters(), &t)?;
        function_frame.define_local(&var_.value, sig.clone())?;
        locals_signature.0.push(sig);
        record_src_loc!(local: context, var_);
    }

    Ok(CodeUnit {
        locals: context.signature_index(locals_signature)?,
        code: compile_blocks(context, &mut function_frame, blocks)?,
    })
}

/// Translates each of the blocks that a function body is composed of to bytecode.
///
/// Once the initial translation of statements to bytecode instructions is complete, instructions
/// that jump to an offset in the bytecode are fixed up to refer to the correct offset.
fn compile_blocks(
    context: &mut Context,
    function_frame: &mut FunctionFrame,
    blocks: Vec<Block>,
) -> Result<Vec<Bytecode>> {
    let mut code = vec![];
    let mut label_to_index: HashMap<BlockLabel_, u16> = HashMap::new();
    for block in blocks {
        compile_block(
            context,
            function_frame,
            &mut label_to_index,
            &mut code,
            block.value,
        )?;
    }
    let fake_to_actual = context.build_index_remapping(label_to_index);
    remap_branch_offsets(&mut code, &fake_to_actual);
    Ok(code)
}

/// Translates a block's statements to bytecode instructions.
fn compile_block(
    context: &mut Context,
    function_frame: &mut FunctionFrame,
    label_to_index: &mut HashMap<BlockLabel_, u16>,
    code: &mut Vec<Bytecode>,
    block: Block_,
) -> Result<()> {
    label_to_index.insert(block.label.value.clone(), code.len() as u16);
    context.label_index(block.label.value)?;
    for statement in block.statements {
        compile_statement(context, function_frame, label_to_index, code, statement)?;
    }
    Ok(())
}

/// Translates a statement to one or more bytecode instructions.
///
/// Most statements do not impact the control flow of the program, except for the `assert`
/// statement. When translating this statement, additional labels are added to our mapping, and
/// jump instructions referring to those labels' offsets are inserted into the bytecode.
fn compile_statement(
    context: &mut Context,
    function_frame: &mut FunctionFrame,
    label_to_index: &mut HashMap<BlockLabel_, u16>,
    code: &mut Vec<Bytecode>,
    statement: Statement,
) -> Result<()> {
    make_push_instr!(context, code);
    match statement.value {
        Statement_::Abort(exp_opt) => {
            if let Some(exp) = exp_opt {
                compile_expression(context, function_frame, code, *exp)?;
            }
            push_instr!(statement.loc, Bytecode::Abort);
            function_frame.pop()?;
        }
        Statement_::Assert(cond, err) => {
            // First, compile the bytecode for the assert's condition.
            // The parser implicitly wraps the condition expression in a unary
            // expression `!(exp)`.
            let cond_loc = cond.loc;
            compile_expression(context, function_frame, code, *cond)?;

            // Create a conditional branch that continues execution if the condition holds,
            // and otherwise falls through to an abort. Because the condition expression is
            // evaluated as `!(exp)`, branch to the failure label if the condition is *false*.
            let cont_label = BlockLabel_(Symbol::from(format!("assert_cont_{}", code.len())));
            push_instr!(
                cond_loc,
                Bytecode::BrFalse(context.label_index(cont_label.clone())?)
            );

            // In case of a fallthrough, the assert has failed.
            // Compile the bytecode for the error expression, then abort.
            let err_loc = err.loc;
            compile_expression(context, function_frame, code, *err)?;
            push_instr!(err_loc, Bytecode::Abort);

            // Record the continue block index as coming after the abort.
            label_to_index.insert(cont_label, code.len() as u16);
        }
        Statement_::Assign(lvalues, rhs_expressions) => {
            compile_expression(context, function_frame, code, rhs_expressions)?;
            compile_lvalues(context, function_frame, code, lvalues)?;
        }
        Statement_::Exp(e) => {
            compile_expression(context, function_frame, code, *e)?;
        }
        Statement_::Jump(label) => push_instr!(
            label.loc,
            Bytecode::Branch(context.label_index(label.value)?)
        ),
        Statement_::JumpIf(cond, label) => {
            let loc = cond.loc;
            compile_expression(context, function_frame, code, *cond)?;
            push_instr!(loc, Bytecode::BrTrue(context.label_index(label.value)?));
        }
        Statement_::JumpIfFalse(cond, label) => {
            let loc = cond.loc;
            compile_expression(context, function_frame, code, *cond)?;
            push_instr!(loc, Bytecode::BrFalse(context.label_index(label.value)?));
        }
        Statement_::Return(exps) => {
            compile_expression(context, function_frame, code, *exps)?;
            push_instr!(statement.loc, Bytecode::Ret);
        }
        Statement_::Unpack(name, tys, bindings, e) => {
            let tokens = Signature(compile_types(
                context,
                function_frame.type_parameters(),
                &tys,
            )?);

            compile_expression(context, function_frame, code, *e)?;

            let def_idx = context.struct_definition_index(&name)?;
            if tys.is_empty() {
                push_instr!(statement.loc, Bytecode::Unpack(def_idx));
            } else {
                let type_parameters_id = context.signature_index(tokens)?;
                let si_idx = context.struct_instantiation_index(def_idx, type_parameters_id)?;
                push_instr!(statement.loc, Bytecode::UnpackGeneric(si_idx));
            }
            function_frame.pop()?;

            for (field_, lhs_variable) in bindings.iter().rev() {
                let loc_idx = function_frame.get_local(&lhs_variable.value)?;
                let st_loc = Bytecode::StLoc(loc_idx);
                push_instr!(field_.loc, st_loc);
            }
        }
    }
    Ok(())
}

fn compile_lvalues(
    context: &mut Context,
    function_frame: &mut FunctionFrame,
    code: &mut Vec<Bytecode>,
    lvalues: Vec<LValue>,
) -> Result<()> {
    make_push_instr!(context, code);
    for lvalue_ in lvalues.into_iter().rev() {
        match lvalue_.value {
            LValue_::Var(v) => {
                let loc_idx = function_frame.get_local(&v.value)?;
                push_instr!(lvalue_.loc, Bytecode::StLoc(loc_idx));
                function_frame.pop()?;
            }
            LValue_::Mutate(e) => {
                compile_expression(context, function_frame, code, e)?;
                push_instr!(lvalue_.loc, Bytecode::WriteRef);
                function_frame.pop()?;
                function_frame.pop()?;
            }
            LValue_::Pop => {
                push_instr!(lvalue_.loc, Bytecode::Pop);
                function_frame.pop()?;
            }
        }
    }
    Ok(())
}

fn compile_expression(
    context: &mut Context,
    function_frame: &mut FunctionFrame,
    code: &mut Vec<Bytecode>,
    exp: Exp,
) -> Result<()> {
    make_push_instr!(context, code);
    Ok(match exp.value {
        Exp_::Move(v) => {
            let loc_idx = function_frame.get_local(&v.value)?;
            let load_loc = Bytecode::MoveLoc(loc_idx);
            push_instr!(exp.loc, load_loc);
            function_frame.push()?;
        }
        Exp_::Copy(v) => {
            let loc_idx = function_frame.get_local(&v.value)?;
            let load_loc = Bytecode::CopyLoc(loc_idx);
            push_instr!(exp.loc, load_loc);
            function_frame.push()?;
        }
        Exp_::BorrowLocal(is_mutable, v) => {
            let loc_idx = function_frame.get_local(&v.value)?;
            if is_mutable {
                push_instr!(exp.loc, Bytecode::MutBorrowLoc(loc_idx));
            } else {
                push_instr!(exp.loc, Bytecode::ImmBorrowLoc(loc_idx));
            }
            function_frame.push()?;
        }
        Exp_::Value(cv) => match cv.value {
            CopyableVal_::Address(address) => {
                let address_value = MoveValue::Address(address);
                let constant = compile_constant(context, Type::Address, address_value)?;
                let idx = context.constant_index(constant)?;
                push_instr!(exp.loc, Bytecode::LdConst(idx));
                function_frame.push()?;
            }
            CopyableVal_::U8(i) => {
                push_instr!(exp.loc, Bytecode::LdU8(i));
                function_frame.push()?;
            }
            CopyableVal_::U16(i) => {
                push_instr!(exp.loc, Bytecode::LdU16(i));
                function_frame.push()?;
            }
            CopyableVal_::U32(i) => {
                push_instr!(exp.loc, Bytecode::LdU32(i));
                function_frame.push()?;
            }
            CopyableVal_::U64(i) => {
                push_instr!(exp.loc, Bytecode::LdU64(i));
                function_frame.push()?;
            }
            CopyableVal_::U128(i) => {
                push_instr!(exp.loc, Bytecode::LdU128(i));
                function_frame.push()?;
            }
            CopyableVal_::U256(i) => {
                push_instr!(exp.loc, Bytecode::LdU256(i));
                function_frame.push()?;
            }
            CopyableVal_::ByteArray(buf) => {
                let vec_value = MoveValue::vector_u8(buf);
                let ty = Type::Vector(Box::new(Type::U8));
                let constant = compile_constant(context, ty, vec_value)?;
                let idx = context.constant_index(constant)?;
                push_instr!(exp.loc, Bytecode::LdConst(idx));
                function_frame.push()?;
            }
            CopyableVal_::Bool(b) => {
                push_instr! {exp.loc,
                    if b {
                        Bytecode::LdTrue
                    } else {
                        Bytecode::LdFalse
                    }
                };
                function_frame.push()?;
            }
        },
        Exp_::Pack(name, ast_tys, fields) => {
            let sig_tys = compile_types(context, function_frame.type_parameters(), &ast_tys)?;
            let tokens = Signature(sig_tys);
            let type_actuals_id = context.signature_index(tokens)?;
            let def_idx = context.struct_definition_index(&name)?;

            let self_name = ModuleName::module_self();
            let ident = QualifiedStructIdent {
                module: self_name,
                name: name.clone(),
            };
            let sh_idx = context.struct_handle_index(ident)?;

            let num_fields = fields.len();
            for (field_order, (field, e)) in fields.into_iter().enumerate() {
                // Check that the fields are specified in order matching the definition.
                let (_, _, decl_order) = context.field(sh_idx, field.value.clone())?;
                if field_order != decl_order {
                    bail!("Field {} defined out of order for struct {}", field, name);
                }

                compile_expression(context, function_frame, code, e)?;
            }
            if ast_tys.is_empty() {
                push_instr!(exp.loc, Bytecode::Pack(def_idx));
            } else {
                let si_idx = context.struct_instantiation_index(def_idx, type_actuals_id)?;
                push_instr!(exp.loc, Bytecode::PackGeneric(si_idx));
            }
            for _ in 0..num_fields {
                function_frame.pop()?;
            }
            function_frame.push()?;
        }
        Exp_::UnaryExp(op, e) => {
            compile_expression(context, function_frame, code, *e)?;
            match op {
                UnaryOp::Not => {
                    push_instr!(exp.loc, Bytecode::Not);
                }
            }
        }
        Exp_::BinopExp(e1, op, e2) => {
            compile_expression(context, function_frame, code, *e1)?;
            compile_expression(context, function_frame, code, *e2)?;

            function_frame.pop()?;
            match op {
                BinOp::Add => {
                    push_instr!(exp.loc, Bytecode::Add);
                }
                BinOp::Sub => {
                    push_instr!(exp.loc, Bytecode::Sub);
                }
                BinOp::Mul => {
                    push_instr!(exp.loc, Bytecode::Mul);
                }
                BinOp::Mod => {
                    push_instr!(exp.loc, Bytecode::Mod);
                }
                BinOp::Div => {
                    push_instr!(exp.loc, Bytecode::Div);
                }
                BinOp::BitOr => {
                    push_instr!(exp.loc, Bytecode::BitOr);
                }
                BinOp::BitAnd => {
                    push_instr!(exp.loc, Bytecode::BitAnd);
                }
                BinOp::Xor => {
                    push_instr!(exp.loc, Bytecode::Xor);
                }
                BinOp::Shl => {
                    push_instr!(exp.loc, Bytecode::Shl);
                }
                BinOp::Shr => {
                    push_instr!(exp.loc, Bytecode::Shr);
                }
                BinOp::Or => {
                    push_instr!(exp.loc, Bytecode::Or);
                }
                BinOp::And => {
                    push_instr!(exp.loc, Bytecode::And);
                }
                BinOp::Eq => {
                    push_instr!(exp.loc, Bytecode::Eq);
                }
                BinOp::Neq => {
                    push_instr!(exp.loc, Bytecode::Neq);
                }
                BinOp::Lt => {
                    push_instr!(exp.loc, Bytecode::Lt);
                }
                BinOp::Gt => {
                    push_instr!(exp.loc, Bytecode::Gt);
                }
                BinOp::Le => {
                    push_instr!(exp.loc, Bytecode::Le);
                }
                BinOp::Ge => {
                    push_instr!(exp.loc, Bytecode::Ge);
                }
                BinOp::Subrange => {
                    unreachable!("Subrange operators should only appear in specification ASTs.");
                }
            }
        }
        Exp_::Dereference(e) => {
            compile_expression(context, function_frame, code, *e)?;
            push_instr!(exp.loc, Bytecode::ReadRef);
        }
        Exp_::Borrow {
            is_mutable,
            exp: inner_exp,
            field,
        } => {
            // Compile the "inner expression." In the case of a field borrow
            // such as `&mut move(s).S::f`, `move(s)` would be the inner
            // expression.
            compile_expression(context, function_frame, code, *inner_exp)?;

            // We're compiling a field borrow expression. To transform an
            // expression like this into bytecode, we need to create a borrow
            // field instruction that references the correct field handle index.
            // We can't know what the index of the field is without determining
            // the type of the underlying struct.
            let struct_ident = QualifiedStructIdent {
                module: ModuleName::module_self(),
                name: field.value.struct_name,
            };
            let sh_idx = context.struct_handle_index(struct_ident)?;
            let (def_idx, _, field_offset) = context.field(sh_idx, field.value.field.value)?;

            function_frame.pop()?;

            let fh_idx = context.field_handle_index(def_idx, field_offset as u16)?;

            if field.value.type_actuals.is_empty() {
                // The field handle index is sufficient if borrowing a field
                // from a struct with a concrete type.
                if is_mutable {
                    push_instr!(exp.loc, Bytecode::MutBorrowField(fh_idx));
                } else {
                    push_instr!(exp.loc, Bytecode::ImmBorrowField(fh_idx));
                }
            } else {
                // To borrow a field from a generic struct, the generic borrow
                // instruction needs the index of the field instantiation.
                let tokens = Signature(compile_types(
                    context,
                    function_frame.type_parameters(),
                    &field.value.type_actuals,
                )?);
                let type_parameters_id = context.signature_index(tokens)?;
                let fi_idx = context.field_instantiation_index(fh_idx, type_parameters_id)?;
                if is_mutable {
                    push_instr!(exp.loc, Bytecode::MutBorrowFieldGeneric(fi_idx));
                } else {
                    push_instr!(exp.loc, Bytecode::ImmBorrowFieldGeneric(fi_idx));
                }
            }
            function_frame.push()?;
        }
        Exp_::FunctionCall(f, exps) => {
            compile_expression(context, function_frame, code, *exps)?;
            compile_call(context, function_frame, code, f)?
        }
        Exp_::ExprList(exps) => {
            for e in exps {
                compile_expression(context, function_frame, code, e)?;
            }
        }
    })
}

fn compile_call(
    context: &mut Context,
    function_frame: &mut FunctionFrame,
    code: &mut Vec<Bytecode>,
    call: FunctionCall,
) -> Result<()> {
    make_push_instr!(context, code);
    Ok(match call.value {
        FunctionCall_::Builtin(function) => {
            match function {
                Builtin::Exists(name, tys) => {
                    let tokens = Signature(compile_types(
                        context,
                        function_frame.type_parameters(),
                        &tys,
                    )?);
                    let type_actuals_id = context.signature_index(tokens)?;
                    let def_idx = context.struct_definition_index(&name)?;
                    if tys.is_empty() {
                        push_instr!(call.loc, Bytecode::Exists(def_idx));
                    } else {
                        let si_idx =
                            context.struct_instantiation_index(def_idx, type_actuals_id)?;
                        push_instr!(call.loc, Bytecode::ExistsGeneric(si_idx));
                    }
                    function_frame.pop()?;
                    function_frame.push()?;
                }
                Builtin::BorrowGlobal(mut_, name, ast_tys) => {
                    let sig_tys =
                        compile_types(context, function_frame.type_parameters(), &ast_tys)?;
                    let tokens = Signature(sig_tys);
                    let type_actuals_id = context.signature_index(tokens)?;
                    let def_idx = context.struct_definition_index(&name)?;
                    if ast_tys.is_empty() {
                        push_instr! {call.loc,
                            if mut_ {
                                Bytecode::MutBorrowGlobal(def_idx)
                            } else {
                                Bytecode::ImmBorrowGlobal(def_idx)
                            }
                        };
                    } else {
                        let si_idx =
                            context.struct_instantiation_index(def_idx, type_actuals_id)?;
                        push_instr! {call.loc,
                            if mut_ {
                                Bytecode::MutBorrowGlobalGeneric(si_idx)
                            } else {
                                Bytecode::ImmBorrowGlobalGeneric(si_idx)
                            }
                        };
                    }
                    function_frame.pop()?;
                    function_frame.push()?;
                }
                Builtin::MoveFrom(name, ast_tys) => {
                    let sig_tys =
                        compile_types(context, function_frame.type_parameters(), &ast_tys)?;
                    let tokens = Signature(sig_tys);
                    let type_actuals_id = context.signature_index(tokens)?;
                    let def_idx = context.struct_definition_index(&name)?;
                    if ast_tys.is_empty() {
                        push_instr!(call.loc, Bytecode::MoveFrom(def_idx));
                    } else {
                        let si_idx =
                            context.struct_instantiation_index(def_idx, type_actuals_id)?;
                        push_instr!(call.loc, Bytecode::MoveFromGeneric(si_idx));
                    }
                    function_frame.pop()?; // pop the address
                    function_frame.push()?; // push the return value
                }
                Builtin::MoveTo(name, tys) => {
                    let tokens = Signature(compile_types(
                        context,
                        function_frame.type_parameters(),
                        &tys,
                    )?);
                    let type_actuals_id = context.signature_index(tokens)?;
                    let def_idx = context.struct_definition_index(&name)?;
                    if tys.is_empty() {
                        push_instr!(call.loc, Bytecode::MoveTo(def_idx));
                    } else {
                        let si_idx =
                            context.struct_instantiation_index(def_idx, type_actuals_id)?;
                        push_instr!(call.loc, Bytecode::MoveToGeneric(si_idx));
                    }
                    function_frame.pop()?; // pop the address
                    function_frame.pop()?; // pop the value to be moved
                }
                Builtin::VecPack(tys, num) => {
                    let tokens = compile_types(context, function_frame.type_parameters(), &tys)?;
                    let type_actuals_id = context.signature_index(Signature(tokens))?;
                    push_instr!(call.loc, Bytecode::VecPack(type_actuals_id, num));

                    for _ in 0..num {
                        function_frame.pop()?;
                    }
                    function_frame.push()?; // push the return value
                }
                Builtin::VecLen(tys) => {
                    let tokens = compile_types(context, function_frame.type_parameters(), &tys)?;
                    let type_actuals_id = context.signature_index(Signature(tokens))?;
                    push_instr!(call.loc, Bytecode::VecLen(type_actuals_id));

                    function_frame.pop()?; // pop the vector ref
                    function_frame.push()?; // push the return value
                }
                Builtin::VecImmBorrow(tys) => {
                    let tokens = compile_types(context, function_frame.type_parameters(), &tys)?;
                    let type_actuals_id = context.signature_index(Signature(tokens))?;
                    push_instr!(call.loc, Bytecode::VecImmBorrow(type_actuals_id));

                    function_frame.pop()?; // pop the vector ref
                    function_frame.pop()?; // pop the index
                    function_frame.push()?; // push the return value
                }
                Builtin::VecMutBorrow(tys) => {
                    let tokens = compile_types(context, function_frame.type_parameters(), &tys)?;
                    let type_actuals_id = context.signature_index(Signature(tokens))?;
                    push_instr!(call.loc, Bytecode::VecMutBorrow(type_actuals_id));

                    function_frame.pop()?; // pop the vector ref
                    function_frame.pop()?; // pop the index
                    function_frame.push()?; // push the return value
                }
                Builtin::VecPushBack(tys) => {
                    let tokens = compile_types(context, function_frame.type_parameters(), &tys)?;
                    let type_actuals_id = context.signature_index(Signature(tokens))?;
                    push_instr!(call.loc, Bytecode::VecPushBack(type_actuals_id));

                    function_frame.pop()?; // pop the vector ref
                    function_frame.pop()?; // pop the value
                }
                Builtin::VecPopBack(tys) => {
                    let tokens = compile_types(context, function_frame.type_parameters(), &tys)?;
                    let type_actuals_id = context.signature_index(Signature(tokens))?;
                    push_instr!(call.loc, Bytecode::VecPopBack(type_actuals_id));

                    function_frame.pop()?; // pop the vector ref
                    function_frame.push()?; // push the value
                }
                Builtin::VecUnpack(tys, num) => {
                    let tokens = compile_types(context, function_frame.type_parameters(), &tys)?;
                    let type_actuals_id = context.signature_index(Signature(tokens))?;
                    push_instr!(call.loc, Bytecode::VecUnpack(type_actuals_id, num));

                    function_frame.pop()?; // pop the vector ref
                    for _ in 0..num {
                        function_frame.push()?;
                    }
                }
                Builtin::VecSwap(tys) => {
                    let tokens = compile_types(context, function_frame.type_parameters(), &tys)?;
                    let type_actuals_id = context.signature_index(Signature(tokens))?;
                    push_instr!(call.loc, Bytecode::VecSwap(type_actuals_id));

                    function_frame.pop()?; // pop the vector ref
                    function_frame.pop()?; // pop the first index
                    function_frame.pop()?; // pop the second index
                }
                Builtin::Freeze => {
                    push_instr!(call.loc, Bytecode::FreezeRef);
                    function_frame.pop()?; // pop mut ref
                    function_frame.push()?; // push imm ref
                }
                Builtin::ToU8 => {
                    push_instr!(call.loc, Bytecode::CastU8);
                    function_frame.pop()?;
                    function_frame.push()?;
                }
                Builtin::ToU16 => {
                    push_instr!(call.loc, Bytecode::CastU16);
                    function_frame.pop()?;
                    function_frame.push()?;
                }
                Builtin::ToU32 => {
                    push_instr!(call.loc, Bytecode::CastU32);
                    function_frame.pop()?;
                    function_frame.push()?;
                }
                Builtin::ToU64 => {
                    push_instr!(call.loc, Bytecode::CastU64);
                    function_frame.pop()?;
                    function_frame.push()?;
                }
                Builtin::ToU128 => {
                    push_instr!(call.loc, Bytecode::CastU128);
                    function_frame.pop()?;
                    function_frame.push()?;
                }
                Builtin::ToU256 => {
                    push_instr!(call.loc, Bytecode::CastU256);
                    function_frame.pop()?;
                    function_frame.push()?;
                }
            }
        }
        FunctionCall_::ModuleFunctionCall {
            module,
            name,
            type_actuals,
        } => {
            let ty_arg_tokens =
                compile_types(context, function_frame.type_parameters(), &type_actuals)?;
            let tokens = Signature(ty_arg_tokens);
            let type_actuals_id = context.signature_index(tokens)?;
            let fh_idx = context.function_handle(module, name)?.1;
            let fcall = if type_actuals.is_empty() {
                Bytecode::Call(fh_idx)
            } else {
                let fi_idx = context.function_instantiation_index(fh_idx, type_actuals_id)?;
                Bytecode::CallGeneric(fi_idx)
            };
            push_instr!(call.loc, fcall);
            for _ in 0..type_actuals.len() {
                function_frame.pop()?;
            }
            // Return value of current function is pushed onto the stack.
            function_frame.push()?;
        }
    })
}

fn compile_constant(_context: &mut Context, ty: Type, value: MoveValue) -> Result<Constant> {
    fn type_layout(ty: Type) -> Result<MoveTypeLayout> {
        Ok(match ty {
            Type::Address => MoveTypeLayout::Address,
            Type::Signer => MoveTypeLayout::Signer,
            Type::U8 => MoveTypeLayout::U8,
            Type::U16 => MoveTypeLayout::U16,
            Type::U32 => MoveTypeLayout::U32,
            Type::U64 => MoveTypeLayout::U64,
            Type::U128 => MoveTypeLayout::U128,
            Type::U256 => MoveTypeLayout::U256,
            Type::Bool => MoveTypeLayout::Bool,
            Type::Vector(inner_type) => MoveTypeLayout::Vector(Box::new(type_layout(*inner_type)?)),
            Type::Reference(_, _) => bail!("References are not supported in constant type layouts"),
            Type::TypeParameter(_) => {
                bail!("Type parameters are not supported in constant type layouts")
            }
            Type::Struct(_ident, _tys) => {
                bail!("TODO Structs are not *yet* supported in constant type layouts")
            }
        })
    }

    Constant::serialize_constant(&type_layout(ty)?, &value)
        .ok_or_else(|| format_err!("Could not serialize constant"))
}

//**************************************************************************************************
// Bytecode
//**************************************************************************************************

fn compile_function_body_bytecode(
    context: &mut Context,
    type_parameters: HashMap<TypeVar_, TypeParameterIndex>,
    formals: Vec<(Var, Type)>,
    locals: Vec<(Var, Type)>,
    blocks: BytecodeBlocks,
) -> Result<CodeUnit> {
    let mut function_frame = FunctionFrame::new(type_parameters);
    let mut locals_signature = Signature(vec![]);
    for (var, t) in formals {
        let sig = compile_type(context, function_frame.type_parameters(), &t)?;
        function_frame.define_local(&var.value, sig.clone())?;
        record_src_loc!(parameter: context, var);
    }
    for (var_, t) in locals {
        let sig = compile_type(context, function_frame.type_parameters(), &t)?;
        function_frame.define_local(&var_.value, sig.clone())?;
        locals_signature.0.push(sig);
        record_src_loc!(local: context, var_);
    }
    let sig_idx = context.signature_index(locals_signature)?;

    let mut code = vec![];
    let mut label_to_index: HashMap<BlockLabel_, u16> = HashMap::new();
    for (label, block) in blocks {
        label_to_index.insert(label.clone(), code.len() as u16);
        context.label_index(label)?;
        compile_bytecode_block(context, &mut function_frame, &mut code, block)?;
    }
    let fake_to_actual = context.build_index_remapping(label_to_index);
    remap_branch_offsets(&mut code, &fake_to_actual);
    Ok(CodeUnit {
        locals: sig_idx,
        code,
    })
}

fn compile_bytecode_block(
    context: &mut Context,
    function_frame: &mut FunctionFrame,
    code: &mut Vec<Bytecode>,
    block: BytecodeBlock,
) -> Result<()> {
    for instr in block {
        compile_bytecode(context, function_frame, code, instr)?
    }
    Ok(())
}

fn compile_bytecode(
    context: &mut Context,
    function_frame: &mut FunctionFrame,
    code: &mut Vec<Bytecode>,
    sp!(loc, instr_): IRBytecode,
) -> Result<()> {
    make_push_instr!(context, code);
    make_record_nop_label!(context, code);
    let ff_instr = match instr_ {
        IRBytecode_::Pop => Bytecode::Pop,
        IRBytecode_::Ret => Bytecode::Ret,
        IRBytecode_::Nop(None) => Bytecode::Nop,
        IRBytecode_::Nop(Some(lbl)) => {
            record_nop_label!(lbl);
            Bytecode::Nop
        }
        IRBytecode_::BrTrue(lbl) => Bytecode::BrTrue(context.label_index(lbl)?),
        IRBytecode_::BrFalse(lbl) => Bytecode::BrFalse(context.label_index(lbl)?),
        IRBytecode_::Branch(lbl) => Bytecode::Branch(context.label_index(lbl)?),
        IRBytecode_::LdU8(u) => Bytecode::LdU8(u),
        IRBytecode_::LdU16(u) => Bytecode::LdU16(u),
        IRBytecode_::LdU32(u) => Bytecode::LdU32(u),
        IRBytecode_::LdU64(u) => Bytecode::LdU64(u),
        IRBytecode_::LdU128(u) => Bytecode::LdU128(u),
        IRBytecode_::LdU256(u) => Bytecode::LdU256(u),
        IRBytecode_::CastU8 => Bytecode::CastU8,
        IRBytecode_::CastU16 => Bytecode::CastU16,
        IRBytecode_::CastU32 => Bytecode::CastU32,
        IRBytecode_::CastU64 => Bytecode::CastU64,
        IRBytecode_::CastU128 => Bytecode::CastU128,
        IRBytecode_::CastU256 => Bytecode::CastU256,
        IRBytecode_::LdTrue => Bytecode::LdTrue,
        IRBytecode_::LdFalse => Bytecode::LdFalse,
        IRBytecode_::LdConst(ty, v) => {
            let constant = compile_constant(context, ty, v)?;
            Bytecode::LdConst(context.constant_index(constant)?)
        }
        IRBytecode_::LdNamedConst(c) => Bytecode::LdConst(context.named_constant_index(&c)?),
        IRBytecode_::CopyLoc(sp!(_, v_)) => Bytecode::CopyLoc(function_frame.get_local(&v_)?),
        IRBytecode_::MoveLoc(sp!(_, v_)) => Bytecode::MoveLoc(function_frame.get_local(&v_)?),
        IRBytecode_::StLoc(sp!(_, v_)) => Bytecode::StLoc(function_frame.get_local(&v_)?),
        IRBytecode_::Call(m, n, tys) => {
            let tokens = Signature(compile_types(
                context,
                function_frame.type_parameters(),
                &tys,
            )?);
            let type_actuals_id = context.signature_index(tokens)?;
            let fh_idx = context.function_handle(m, n)?.1;
            if tys.is_empty() {
                Bytecode::Call(fh_idx)
            } else {
                let fi_idx = context.function_instantiation_index(fh_idx, type_actuals_id)?;
                Bytecode::CallGeneric(fi_idx)
            }
        }
        IRBytecode_::Pack(n, tys) => {
            let tokens = Signature(compile_types(
                context,
                function_frame.type_parameters(),
                &tys,
            )?);
            let type_actuals_id = context.signature_index(tokens)?;
            let def_idx = context.struct_definition_index(&n)?;
            if tys.is_empty() {
                Bytecode::Pack(def_idx)
            } else {
                let si_idx = context.struct_instantiation_index(def_idx, type_actuals_id)?;
                Bytecode::PackGeneric(si_idx)
            }
        }
        IRBytecode_::Unpack(n, tys) => {
            let tokens = Signature(compile_types(
                context,
                function_frame.type_parameters(),
                &tys,
            )?);
            let type_actuals_id = context.signature_index(tokens)?;
            let def_idx = context.struct_definition_index(&n)?;
            if tys.is_empty() {
                Bytecode::Unpack(def_idx)
            } else {
                let si_idx = context.struct_instantiation_index(def_idx, type_actuals_id)?;
                Bytecode::UnpackGeneric(si_idx)
            }
        }
        IRBytecode_::ReadRef => Bytecode::ReadRef,
        IRBytecode_::WriteRef => Bytecode::WriteRef,
        IRBytecode_::FreezeRef => Bytecode::FreezeRef,
        IRBytecode_::MutBorrowLoc(sp!(_, v_)) => {
            Bytecode::MutBorrowLoc(function_frame.get_local(&v_)?)
        }
        IRBytecode_::ImmBorrowLoc(sp!(_, v_)) => {
            Bytecode::ImmBorrowLoc(function_frame.get_local(&v_)?)
        }
        IRBytecode_::MutBorrowField(name, tys, sp!(_, field_)) => {
            let qualified_struct_name = QualifiedStructIdent {
                module: ModuleName::module_self(),
                name,
            };
            let sh_idx = context.struct_handle_index(qualified_struct_name)?;
            let (def_idx, _, field_offset) = context.field(sh_idx, field_)?;

            let fh_idx = context.field_handle_index(def_idx, field_offset as u16)?;
            if tys.is_empty() {
                Bytecode::MutBorrowField(fh_idx)
            } else {
                let tokens = Signature(compile_types(
                    context,
                    function_frame.type_parameters(),
                    &tys,
                )?);
                let type_actuals_id = context.signature_index(tokens)?;
                let fi_idx = context.field_instantiation_index(fh_idx, type_actuals_id)?;
                Bytecode::MutBorrowFieldGeneric(fi_idx)
            }
        }
        IRBytecode_::ImmBorrowField(name, tys, sp!(_, field_)) => {
            let qualified_struct_name = QualifiedStructIdent {
                module: ModuleName::module_self(),
                name,
            };
            let sh_idx = context.struct_handle_index(qualified_struct_name)?;
            let (def_idx, _, field_offset) = context.field(sh_idx, field_)?;

            let fh_idx = context.field_handle_index(def_idx, field_offset as u16)?;
            if tys.is_empty() {
                Bytecode::ImmBorrowField(fh_idx)
            } else {
                let tokens = Signature(compile_types(
                    context,
                    function_frame.type_parameters(),
                    &tys,
                )?);
                let type_actuals_id = context.signature_index(tokens)?;
                let fi_idx = context.field_instantiation_index(fh_idx, type_actuals_id)?;
                Bytecode::ImmBorrowFieldGeneric(fi_idx)
            }
        }
        IRBytecode_::MutBorrowGlobal(n, tys) => {
            let tokens = Signature(compile_types(
                context,
                function_frame.type_parameters(),
                &tys,
            )?);
            let type_actuals_id = context.signature_index(tokens)?;
            let def_idx = context.struct_definition_index(&n)?;
            if tys.is_empty() {
                Bytecode::MutBorrowGlobal(def_idx)
            } else {
                let si_idx = context.struct_instantiation_index(def_idx, type_actuals_id)?;
                Bytecode::MutBorrowGlobalGeneric(si_idx)
            }
        }
        IRBytecode_::ImmBorrowGlobal(n, tys) => {
            let tokens = Signature(compile_types(
                context,
                function_frame.type_parameters(),
                &tys,
            )?);
            let type_actuals_id = context.signature_index(tokens)?;
            let def_idx = context.struct_definition_index(&n)?;
            if tys.is_empty() {
                Bytecode::ImmBorrowGlobal(def_idx)
            } else {
                let si_idx = context.struct_instantiation_index(def_idx, type_actuals_id)?;
                Bytecode::ImmBorrowGlobalGeneric(si_idx)
            }
        }
        IRBytecode_::Add => Bytecode::Add,
        IRBytecode_::Sub => Bytecode::Sub,
        IRBytecode_::Mul => Bytecode::Mul,
        IRBytecode_::Mod => Bytecode::Mod,
        IRBytecode_::Div => Bytecode::Div,
        IRBytecode_::BitOr => Bytecode::BitOr,
        IRBytecode_::BitAnd => Bytecode::BitAnd,
        IRBytecode_::Xor => Bytecode::Xor,
        IRBytecode_::Or => Bytecode::Or,
        IRBytecode_::And => Bytecode::And,
        IRBytecode_::Not => Bytecode::Not,
        IRBytecode_::Eq => Bytecode::Eq,
        IRBytecode_::Neq => Bytecode::Neq,
        IRBytecode_::Lt => Bytecode::Lt,
        IRBytecode_::Gt => Bytecode::Gt,
        IRBytecode_::Le => Bytecode::Le,
        IRBytecode_::Ge => Bytecode::Ge,
        IRBytecode_::Abort => Bytecode::Abort,
        IRBytecode_::Exists(n, tys) => {
            let tokens = Signature(compile_types(
                context,
                function_frame.type_parameters(),
                &tys,
            )?);
            let type_actuals_id = context.signature_index(tokens)?;
            let def_idx = context.struct_definition_index(&n)?;
            if tys.is_empty() {
                Bytecode::Exists(def_idx)
            } else {
                let si_idx = context.struct_instantiation_index(def_idx, type_actuals_id)?;
                Bytecode::ExistsGeneric(si_idx)
            }
        }
        IRBytecode_::MoveFrom(n, tys) => {
            let tokens = Signature(compile_types(
                context,
                function_frame.type_parameters(),
                &tys,
            )?);
            let type_actuals_id = context.signature_index(tokens)?;
            let def_idx = context.struct_definition_index(&n)?;
            if tys.is_empty() {
                Bytecode::MoveFrom(def_idx)
            } else {
                let si_idx = context.struct_instantiation_index(def_idx, type_actuals_id)?;
                Bytecode::MoveFromGeneric(si_idx)
            }
        }
        IRBytecode_::MoveTo(n, tys) => {
            let tokens = Signature(compile_types(
                context,
                function_frame.type_parameters(),
                &tys,
            )?);
            let type_actuals_id = context.signature_index(tokens)?;
            let def_idx = context.struct_definition_index(&n)?;
            if tys.is_empty() {
                Bytecode::MoveTo(def_idx)
            } else {
                let si_idx = context.struct_instantiation_index(def_idx, type_actuals_id)?;
                Bytecode::MoveToGeneric(si_idx)
            }
        }
        IRBytecode_::Shl => Bytecode::Shl,
        IRBytecode_::Shr => Bytecode::Shr,
        IRBytecode_::VecPack(ty, n) => {
            let tokens = compile_type(context, function_frame.type_parameters(), &ty)?;
            let sig = Signature(vec![tokens]);
            Bytecode::VecPack(context.signature_index(sig)?, n)
        }
        IRBytecode_::VecLen(ty) => {
            let tokens = compile_type(context, function_frame.type_parameters(), &ty)?;
            let sig = Signature(vec![tokens]);
            Bytecode::VecLen(context.signature_index(sig)?)
        }
        IRBytecode_::VecImmBorrow(ty) => {
            let tokens = compile_type(context, function_frame.type_parameters(), &ty)?;
            let sig = Signature(vec![tokens]);
            Bytecode::VecImmBorrow(context.signature_index(sig)?)
        }
        IRBytecode_::VecMutBorrow(ty) => {
            let tokens = compile_type(context, function_frame.type_parameters(), &ty)?;
            let sig = Signature(vec![tokens]);
            Bytecode::VecMutBorrow(context.signature_index(sig)?)
        }
        IRBytecode_::VecPushBack(ty) => {
            let tokens = compile_type(context, function_frame.type_parameters(), &ty)?;
            let sig = Signature(vec![tokens]);
            Bytecode::VecPushBack(context.signature_index(sig)?)
        }
        IRBytecode_::VecPopBack(ty) => {
            let tokens = compile_type(context, function_frame.type_parameters(), &ty)?;
            let sig = Signature(vec![tokens]);
            Bytecode::VecPopBack(context.signature_index(sig)?)
        }
        IRBytecode_::VecUnpack(ty, n) => {
            let tokens = compile_type(context, function_frame.type_parameters(), &ty)?;
            let sig = Signature(vec![tokens]);
            Bytecode::VecUnpack(context.signature_index(sig)?, n)
        }
        IRBytecode_::VecSwap(ty) => {
            let tokens = compile_type(context, function_frame.type_parameters(), &ty)?;
            let sig = Signature(vec![tokens]);
            Bytecode::VecSwap(context.signature_index(sig)?)
        }
    };
    push_instr!(loc, ff_instr);
    Ok(())
}

fn remap_branch_offsets(code: &mut Vec<Bytecode>, fake_to_actual: &HashMap<u16, u16>) {
    for instr in code {
        match instr {
            Bytecode::BrTrue(offset) | Bytecode::BrFalse(offset) | Bytecode::Branch(offset) => {
                *offset = fake_to_actual[offset]
            }
            _ => (),
        }
    }
}
