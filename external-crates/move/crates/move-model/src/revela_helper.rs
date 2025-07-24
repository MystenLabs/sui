use std::{
    collections::{BTreeMap, HashMap},
    rc::Rc,
    vec,
};

use codespan::Files;
use move_binary_format::{file_format::DatatypeHandleIndex, CompiledModule};
use move_bytecode_source_map::source_map::SourceMap;
use move_command_line_common::files::FileHash;
use move_compiler::expansion::ast::Program;

use crate::{
    ast::*,
    builder::{model_builder::ModelBuilder, module_builder::ModuleBuilder},
    model::*,
    symbol::*,
};

pub fn dummy_loc() -> move_ir_types::location::Loc {
    move_ir_types::location::Loc::invalid()
}

pub fn dummy_source_map() -> SourceMap {
    SourceMap::new(
        dummy_loc(),
        move_ir_types::ast::ModuleIdent::new(
            move_ir_types::ast::ModuleName::module_self(),
            move_core_types::account_address::AccountAddress::random(),
        ),
    )
}

impl ModuleData {
    pub fn dummy(name: ModuleName, id: usize) -> ModuleData {
        ModuleData {
            name,
            id: ModuleId::new(id),
            attributes: Default::default(),
            source_map: dummy_source_map(),
            named_constants: Default::default(),
            struct_data: Default::default(),
            struct_idx_to_id: Default::default(),
            function_data: Default::default(),
            function_idx_to_id: Default::default(),
            loc: Default::default(),
            used_modules: Default::default(),
            friend_modules: Default::default(),
            module: Default::default(),
            enum_data: Default::default(),
            enum_idx_to_id: Default::default(),
        }
    }
}

impl StructData {
    /// Creates a new struct data.
    pub fn dummy(name: Symbol) -> StructData {
        StructData {
            name,
            loc: Default::default(),
            attributes: Default::default(),
            field_data: Default::default(),
            info: StructInfo::Declared {
                def_idx: move_binary_format::file_format::StructDefinitionIndex::new(0),
                handle_idx: DatatypeHandleIndex::new(0),
            },
        }
    }
}

impl FunctionData {
    /// Creates a new function data.
    pub fn dummy(name: Symbol) -> FunctionData {
        FunctionData {
            name,
            loc: Default::default(),
            def_idx: Default::default(),
            handle_idx: Default::default(),
            attributes: Default::default(),
            called_funs: Default::default(),
            calling_funs: Default::default(),
            transitive_closure_of_called_funs: Default::default(),
            arg_names: Default::default(),
            type_arg_names: Default::default(),
        }
    }
}

pub fn run_stackless_compiler(
    env: &mut GlobalEnv,
    program: Program,
    module_map: HashMap<usize, CompiledModule>,
) {
    env.add_source(FileHash::empty(), Rc::new(BTreeMap::new()), "", "", false);
    (env.file_hash_map).insert(
        FileHash::empty(),
        (
            "".to_string(),
            Files::<String>::default().add("".to_string(), "".to_string()),
        ),
    );

    let mut builder: ModelBuilder<'_> = ModelBuilder::new(env);

    declare_builtins(&mut builder);

    for (module_count, (module_id, module_def)) in program.modules.into_iter().enumerate() {
        let loc = builder.to_loc(&module_def.loc);
        let addr_bytes = builder.resolve_address(&loc, &module_id.value.address);
        let module_name = ModuleName::from_address_bytes_and_name(
            addr_bytes,
            builder
                .env
                .symbol_pool()
                .make(&module_id.value.module.0.value),
        );
        let dummy_compiled_module = create_dummy_compiled_module(
            &module_id,
            &module_def,
            move_core_types::account_address::AccountAddress::from_bytes(addr_bytes.into_bytes()).expect("invalid address?")
        );
        let compiled_module = module_map
            .get(&(module_def.loc.start() as usize))
            .unwrap_or(&dummy_compiled_module);
        let module_id = ModuleId::new(module_count);
        let mut module_translator = ModuleBuilder::new(&mut builder, module_id, module_name);
        module_translator.translate(loc, module_def, compiled_module.clone(), dummy_source_map());
    }
}

fn create_dummy_compiled_module(
    module_id: &move_ir_types::location::Spanned<move_compiler::expansion::ast::ModuleIdent_>,
    module_def: &move_compiler::expansion::ast::ModuleDefinition,
    address: move_core_types::account_address::AccountAddress,
) -> CompiledModule {
    use move_binary_format::file_format::*;
    let mut identifiers = Vec::new();
    identifiers.push(
        move_core_types::identifier::IdentStr::new(module_id.value.module.0.value.as_str())
            .unwrap()
            .to_owned(),
    );
    let mut function_handles = Vec::new();
    let mut function_defs = Vec::new();
    for (_loc, name, _func) in &module_def.functions {
        let function_handle = FunctionHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(identifiers.len() as u16),
            parameters: SignatureIndex(0),
            return_: SignatureIndex(0),
            type_parameters: Default::default(),
        };
        identifiers.push(
            move_core_types::identifier::IdentStr::new(name.as_str())
                .unwrap()
                .to_owned(),
        );
        let function_def = FunctionDefinition {
            function: FunctionHandleIndex(function_handles.len() as u16),
            visibility: Visibility::Public,
            acquires_global_resources: vec![],
            code: None,
            is_entry: false,
        };
        function_handles.push(function_handle);
        function_defs.push(function_def);
    }

    let mut datatype_handles = Vec::new();
    let mut struct_defs = Vec::new();
    for (_loc, name, _sdef) in &module_def.structs {
        let struct_handle = DatatypeHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(identifiers.len() as u16),
            abilities: AbilitySet::EMPTY,
            type_parameters: vec![],
        };
        identifiers.push(
            move_core_types::identifier::IdentStr::new(name.as_str())
                .unwrap()
                .to_owned(),
        );
        let struct_def = StructDefinition {
            struct_handle: DatatypeHandleIndex(datatype_handles.len() as u16),
            field_information: StructFieldInformation::Declared(vec![]), //TODO
        };
        datatype_handles.push(struct_handle);
        struct_defs.push(struct_def);
    }

    CompiledModule {
        version: move_binary_format::file_format_common::VERSION_MAX,
        module_handles: vec![ModuleHandle {
            address: AddressIdentifierIndex(0),
            name: IdentifierIndex(0),
        }],
        self_module_handle_idx: ModuleHandleIndex(0),
        identifiers,
        address_identifiers: vec![address],
        constant_pool: vec![],
        metadata: vec![],
        function_defs,
        struct_defs,
        datatype_handles,
        function_handles,
        field_handles: vec![],
        friend_decls: vec![],
        struct_def_instantiations: vec![],
        function_instantiations: vec![],
        field_instantiations: vec![],
        signatures: vec![Signature(vec![])],
        enum_defs: vec![], //TODO
        enum_def_instantiations: vec![],
        variant_handles: vec![],
        variant_instantiation_handles: vec![],
    }
}

fn declare_builtins(trans: &mut ModelBuilder) {
    use crate::ty::{PrimitiveType, Type};
    use num::BigUint;

    let loc = trans.env.internal_loc();
    // let bool_t = &Type::new_prim(PrimitiveType::Bool);
    let num_t = &Type::new_prim(PrimitiveType::Num);
    // let range_t = &Type::new_prim(PrimitiveType::Range);
    // let address_t = &Type::new_prim(PrimitiveType::Address);

    let mk_type_param = |trans: &ModelBuilder<'_>, p: u16| {
        (
            trans.env.symbol_pool().make(&format!("T{}", p)),
            Type::TypeParameter(p),
        )
    };
    let mk_param = |trans: &ModelBuilder<'_>, p: usize, ty: Type| {
        (trans.env.symbol_pool().make(&format!("p{}", p)), ty)
    };

    let param_t_0 = &Type::TypeParameter(0);

    let builtin_qualified_symbol = |trans: &ModelBuilder, name: &str| QualifiedSymbol {
        module_name: ModuleName::new(
            BigUint::from(0u32),
            trans.env.symbol_pool().make("$built_in$"),
        ),
        symbol: trans.env.symbol_pool().make(name),
    };

    {
        // Builtin functions.
        let vector_t0 = &Type::Vector(Box::new(param_t_0.clone()));

        // https://vscode.dev/github/MystenLabs/sui/blob/main/external-crates/move/crates/move-compiler/src/naming/fake_natives.rs#L81-L88
        // https://docs.sui.io/references/framework/move-stdlib/vector#0x1_vector_destroy_empty
        // Vectors
        // public fun empty<Element>(): vector<Element>
        trans.define_fun(
            loc.clone(),
            vec![],
            builtin_qualified_symbol(trans, "empty"),
            vec![mk_type_param(trans, 0)],
            vec![],
        );
        // public fun length<Element>(v: &vector<Element>): u64
        trans.define_fun(
            loc.clone(),
            vec![],
            builtin_qualified_symbol(trans, "length"),
            vec![mk_type_param(trans, 0)],
            vec![mk_param(trans, 0, vector_t0.clone())],
        );
        // public fun borrow<Element>(v: &vector<Element>, i: u64): &Element
        trans.define_fun(
            loc.clone(),
            vec![],
            builtin_qualified_symbol(trans, "borrow"),
            vec![mk_type_param(trans, 0)],
            vec![
                mk_param(trans, 0, vector_t0.clone()),
                mk_param(trans, 1, num_t.clone()),
            ],
        );
        // public fun push_back<Element>(v: &mut vector<Element>, e: Element)
        trans.define_fun(
            loc.clone(),
            vec![],
            builtin_qualified_symbol(&trans, "push_back"),
            vec![mk_type_param(trans, 0)],
            vec![
                mk_param(trans, 0, vector_t0.clone()),
                mk_param(trans, 1, param_t_0.clone()),
            ],
        );
        // public fun borrow_mut<Element>(v: &mut vector<Element>, i: u64): &mut Element
        trans.define_fun(
            loc.clone(),
            vec![],
            builtin_qualified_symbol(&trans, "borrow_mut"),
            vec![mk_type_param(trans, 0)],
            vec![
                mk_param(trans, 0, vector_t0.clone()),
                mk_param(trans, 1, num_t.clone()),
            ],
        );
        // public fun pop_back<Element>(v: &mut vector<Element>): Element
        trans.define_fun(
            loc.clone(),
            vec![],
            builtin_qualified_symbol(&trans, "pop_back"),
            vec![mk_type_param(trans, 0)],
            vec![mk_param(trans, 0, vector_t0.clone())],
        );
        // public fun destroy_empty<Element>(v: vector<Element>)
        trans.define_fun(
            loc.clone(),
            vec![],
            builtin_qualified_symbol(&trans, "destroy_empty"),
            vec![mk_type_param(trans, 0)],
            vec![mk_param(trans, 0, vector_t0.clone())],
        );
        // public fun swap<Element>(v: &mut vector<Element>, i: u64, j: u64)
        trans.define_fun(
            loc.clone(),
            vec![],
            builtin_qualified_symbol(&trans, "swap"),
            vec![mk_type_param(trans, 0)],
            vec![
                mk_param(trans, 0, vector_t0.clone()),
                mk_param(trans, 1, num_t.clone()),
                mk_param(trans, 2, num_t.clone()),
            ],
        );
    }
}
