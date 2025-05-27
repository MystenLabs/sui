// Copyright (c) Verichains, 2023

use std::collections::{HashMap, HashSet, VecDeque};

use move_binary_format::{
    file_format::{DatatypeHandleIndex, SignatureToken},
    CompiledModule,
};
use move_command_line_common::files::FileHash;
use move_compiler::{
    expansion::ast::{
        AbilitySet, Address, Attributes, DatatypeTypeParameter, Function, FunctionBody_,
        FunctionSignature, ModuleAccess_, ModuleDefinition, ModuleIdent, ModuleIdent_, Program,
        StructDefinition, StructFields, Type, Type_,
    },
    parser::ast::{DatatypeName, Field, FunctionName, ModuleName},
    shared::unique_map::UniqueMap,
};
use move_ir_types::location::{Loc, Spanned};
use move_symbol_pool::Symbol;

use super::{aptos_compat::BinaryIndexedView, naming::Naming};

pub(crate) fn fake_loc() -> Loc {
    Loc::new(FileHash::empty(), 0, 0)
}

pub(crate) fn span_<T>(x: T) -> Spanned<T> {
    Spanned::unsafe_no_loc(x)
}

fn map_abilities(
    abilities: move_binary_format::file_format::AbilitySet,
) -> Result<AbilitySet, anyhow::Error> {
    let result: Vec<_> = abilities
        .into_iter()
        .map(|ability| {
            let ability_ast = match ability {
                move_binary_format::file_format::Ability::Copy => {
                    move_compiler::parser::ast::Ability_::Copy
                }

                move_binary_format::file_format::Ability::Drop => {
                    move_compiler::parser::ast::Ability_::Drop
                }

                move_binary_format::file_format::Ability::Store => {
                    move_compiler::parser::ast::Ability_::Store
                }

                move_binary_format::file_format::Ability::Key => {
                    move_compiler::parser::ast::Ability_::Key
                }
            };

            span_(ability_ast)
        })
        .collect();

    Ok(AbilitySet::from_abilities(result.iter().cloned()).unwrap())
}

fn map_type_parameter(
    name: Symbol,
    type_parameter_binary: move_binary_format::file_format::DatatypeTyParameter,
) -> Result<DatatypeTypeParameter, anyhow::Error> {
    Ok(DatatypeTypeParameter {
        is_phantom: type_parameter_binary.is_phantom,
        name: span_(name),
        constraints: map_abilities(type_parameter_binary.constraints)?,
    })
}

fn module_to_module_ident(
    compiled_module: &move_binary_format::CompiledModule,
    module: &move_binary_format::file_format::ModuleHandle,
) -> Result<ModuleIdent, anyhow::Error> {
    let module_name = compiled_module.identifier_at(module.name);
    let module_name = Symbol::from(module_name.as_str());
    let module_name = span_(module_name);
    let address = Address::Numerical {
        name: Some(module_name),
        value: span_(
            move_command_line_common::address::NumericalAddress::parse_str(
                compiled_module
                    .address_identifier_at(module.address)
                    .to_hex_literal()
                    .as_str(),
            )
            .unwrap(),
        ),
        name_conflict: false,
    };
    Ok(span_(ModuleIdent_::new(address, ModuleName(module_name))))
}

fn module_access_for_struct(
    compiled_module: &move_binary_format::CompiledModule,
    struct_handle_idx: DatatypeHandleIndex,
    _type_arguments: &Vec<SignatureToken>,
) -> Result<Spanned<ModuleAccess_>, anyhow::Error> {
    let struct_ = compiled_module.datatype_handle_at(struct_handle_idx);
    let module_ = compiled_module.module_handle_at(struct_.module);
    let access = ModuleAccess_::ModuleAccess(
        module_to_module_ident(compiled_module, module_)?,
        span_(Symbol::from(
            compiled_module.identifier_at(struct_.name).as_str(),
        )),
    );
    Ok(span_(access))
}

fn struct_access(
    compiled_module: &move_binary_format::CompiledModule,
    struct_handle_idx: DatatypeHandleIndex,
    type_arguments: &Vec<SignatureToken>,
    naming: &Naming,
) -> Result<Type_, anyhow::Error> {
    Ok(Type_::Apply(
        module_access_for_struct(compiled_module, struct_handle_idx, type_arguments)?,
        type_arguments
            .iter()
            .map(|x| map_type(compiled_module, x, naming))
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
    ))
}

fn map_type(
    compiled_module: &move_binary_format::CompiledModule,
    signature: &move_binary_format::file_format::SignatureToken,
    naming: &Naming,
) -> Result<Type, anyhow::Error> {
    fn prim_type(x: &str) -> Type_ {
        Type_::Apply(
            span_(ModuleAccess_::Name(span_(Symbol::from(x)))),
            Vec::new(),
        )
    }

    Ok(span_(match signature {
        move_binary_format::file_format::SignatureToken::Bool => prim_type("bool"),
        move_binary_format::file_format::SignatureToken::U8 => prim_type("u8"),
        move_binary_format::file_format::SignatureToken::U16 => prim_type("u16"),
        move_binary_format::file_format::SignatureToken::U32 => prim_type("u32"),
        move_binary_format::file_format::SignatureToken::U64 => prim_type("u64"),
        move_binary_format::file_format::SignatureToken::U128 => prim_type("u128"),
        move_binary_format::file_format::SignatureToken::U256 => prim_type("u256"),
        move_binary_format::file_format::SignatureToken::Address => prim_type("address"),
        move_binary_format::file_format::SignatureToken::Signer => prim_type("signer"),

        move_binary_format::file_format::SignatureToken::Vector(sig) => Type_::Apply(
            span_(ModuleAccess_::Name(span_(Symbol::from("vector")))),
            vec![map_type(compiled_module, sig, naming)?],
        ),

        move_binary_format::file_format::SignatureToken::Datatype(struct_handle_idx) => {
            struct_access(compiled_module, *struct_handle_idx, &Vec::new(), naming)?
        }

        move_binary_format::file_format::SignatureToken::DatatypeInstantiation(boxed) => {
            struct_access(compiled_module, boxed.0, &boxed.1, naming)?
        }

        move_binary_format::file_format::SignatureToken::Reference(signature_token) => Type_::Ref(
            false,
            Box::new(map_type(compiled_module, signature_token, naming)?),
        ),

        move_binary_format::file_format::SignatureToken::MutableReference(signature_token) => {
            Type_::Ref(
                true,
                Box::new(map_type(compiled_module, signature_token, naming)?),
            )
        }

        move_binary_format::file_format::SignatureToken::TypeParameter(type_parameter_index) => {
            prim_type(
                naming
                    .templated_type(*type_parameter_index as usize)
                    .as_str(),
            )
        }
    }))
}

fn map_struct(
    compiled_module: &move_binary_format::CompiledModule,
    struct_: &move_binary_format::file_format::StructDefinition,
    naming: &Naming,
) -> Result<StructDefinition, anyhow::Error> {
    let struct_handle = compiled_module.datatype_handle_at(struct_.struct_handle);

    let type_parameters = struct_handle
        .type_parameters
        .iter()
        .enumerate()
        .map(|(idx, x)| {
            Ok(map_type_parameter(
                Symbol::from(naming.templated_type(idx)),
                *x,
            )?)
        })
        .collect::<Result<Vec<DatatypeTypeParameter>, anyhow::Error>>()?;

    let fields: StructFields = match &struct_.field_information {
        move_binary_format::file_format::StructFieldInformation::Native => {
            StructFields::Native(fake_loc())
        }
        move_binary_format::file_format::StructFieldInformation::Declared(fields) => {
            let mut result: UniqueMap<Field, (usize, Type)> = UniqueMap::new();
            for (idx, field) in fields.iter().enumerate() {
                let name = Symbol::from(compiled_module.identifier_at(field.name).as_str());
                let name = Field(span_(name));

                let mapped_type: Type = map_type(compiled_module, &field.signature.0, naming)?;
                result.add(name, (idx, mapped_type)).map_err(|(name, _)| {
                    anyhow::Error::msg(format!("Error adding field {}", name))
                })?;
            }
            StructFields::Named(result)
        }
    };

    Ok(StructDefinition {
        attributes: UniqueMap::new(),
        loc: fake_loc(),
        abilities: map_abilities(struct_handle.abilities)?,
        type_parameters,
        fields,
        warning_filter: move_compiler::diagnostics::WarningFilters::new_for_source(),
        index: struct_.struct_handle.0 as usize,
    })
}

fn map_function(
    compiled_module: &move_binary_format::CompiledModule,
    function_: &move_binary_format::file_format::FunctionDefinition,
    naming: &Naming,
) -> Result<Function, anyhow::Error> {
    let function_handle = compiled_module.function_handle_at(function_.function);

    let visibility = match function_.visibility {
        move_binary_format::file_format::Visibility::Private => {
            move_compiler::expansion::ast::Visibility::Internal
        }
        move_binary_format::file_format::Visibility::Public => {
            move_compiler::expansion::ast::Visibility::Public(fake_loc())
        }
        move_binary_format::file_format::Visibility::Friend => {
            move_compiler::expansion::ast::Visibility::Friend(fake_loc())
        }
    };

    let type_parameters = function_handle
        .type_parameters
        .iter()
        .enumerate()
        .map(|(idx, x)| Ok((span_(Symbol::from(format!("T{}", idx))), map_abilities(*x)?)))
        .collect::<Result<Vec<(Spanned<Symbol>, AbilitySet)>, anyhow::Error>>()?;

    let parameters = compiled_module
        .signature_at(function_handle.parameters)
        .0
        .iter()
        .enumerate()
        .map(|(idx, x)| {
            Ok((
                move_compiler::expansion::ast::Mutability::Either, //FIXME
                move_compiler::parser::ast::Var(span_(Symbol::from(naming.argument(idx)))),
                map_type(compiled_module, x, naming)?,
            ))
        })
        .collect::<Result<
            Vec<(
                move_compiler::expansion::ast::Mutability,
                move_compiler::parser::ast::Var,
                Type,
            )>,
            anyhow::Error,
        >>()?;

    let return_type = compiled_module
        .signature_at(function_handle.return_)
        .0
        .iter()
        .map(|x| Ok(map_type(compiled_module, x, naming)?))
        .collect::<Result<Vec<Type>, anyhow::Error>>()?;

    let signature = FunctionSignature {
        type_parameters,
        parameters,
        return_type: if return_type.len() == 1 {
            return_type[0].clone()
        } else {
            span_(Type_::Multiple(return_type))
        },
    };

    let body = if function_.is_native() {
        FunctionBody_::Native
    } else {
        // dummy body as we do not have source code
        FunctionBody_::Defined((Default::default(), VecDeque::new()))
    };

    Ok(Function {
        attributes: UniqueMap::new(),
        loc: fake_loc(),
        visibility,
        entry: if function_.is_entry {
            Some(fake_loc())
        } else {
            None
        },
        signature,
        body: span_(body),
        warning_filter: move_compiler::diagnostics::WarningFilters::new_for_source(),
        index: function_.function.0 as usize,
        macro_: None,
    })
}

fn create_module_ident(address: &str, module_name: &str) -> ModuleIdent {
    let address = Address::Numerical {
        name: Some(span_(Symbol::from(module_name))),
        value: span_(
            move_command_line_common::address::NumericalAddress::parse_str(address).unwrap(),
        ),
        name_conflict: false,
    };

    let module_name = span_(Symbol::from(module_name));

    span_(ModuleIdent_::new(address, ModuleName(module_name)))
}

fn builtin_type(name: &str) -> Type {
    span_(Type_::Apply(
        span_(ModuleAccess_::Name(span_(Symbol::from(name)))),
        Vec::new(),
    ))
}

#[allow(dead_code)]
fn ref_type(t: Type) -> Type {
    span_(Type_::Ref(false, Box::new(t)))
}

fn create_dummy_for_non_existing_modules(
    modules: &mut UniqueMap<Spanned<ModuleIdent_>, ModuleDefinition>,
    adding_modules: &Vec<CompiledModule>,
    naming: &Naming,
) -> Result<(), anyhow::Error> {
    #[derive(Default)]
    struct DummyStruct {
        abilitites: Option<AbilitySet>,
        type_parameters: Option<Vec<DatatypeTypeParameter>>,
        fields: HashSet<String>,
    }

    #[derive(Default)]
    struct DummyModule {
        functions: HashSet<String>,
        structs: HashMap<String, DummyStruct>,
    }

    let mut dummy_modules = HashMap::<ModuleIdent, DummyModule>::new();

    for compiled_module in adding_modules {
        for func_handle in compiled_module.function_handles() {
            let module = compiled_module.module_handle_at(func_handle.module);
            let module_id = create_module_ident(
                compiled_module
                    .address_identifier_at(module.address)
                    .to_hex_literal()
                    .as_str(),
                compiled_module.identifier_at(module.name).as_str(),
            );

            if modules.contains_key(&module_id) {
                continue;
            }

            dummy_modules
                .entry(module_id)
                .or_insert(Default::default())
                .functions
                .insert(compiled_module.identifier_at(func_handle.name).to_string());
        }

        for struct_hanlde in compiled_module.datatype_handles() {
            let module = compiled_module.module_handle_at(struct_hanlde.module);
            let module_id = create_module_ident(
                compiled_module
                    .address_identifier_at(module.address)
                    .to_hex_literal()
                    .as_str(),
                compiled_module.identifier_at(module.name).as_str(),
            );

            if modules.contains_key(&module_id) {
                continue;
            }

            let struct_name = compiled_module
                .identifier_at(struct_hanlde.name)
                .to_string();

            let struct_ = dummy_modules
                .entry(module_id)
                .or_insert(Default::default())
                .structs
                .entry(struct_name.clone())
                .or_insert(Default::default());

            let abilities = map_abilities(struct_hanlde.abilities).unwrap();
            if let Some(a) = &struct_.abilitites {
                if a != &abilities {
                    return Err(anyhow::anyhow!(
                        "Different abilities for struct: {}::{}",
                        &module_id,
                        &struct_name
                    ));
                }
            } else {
                struct_.abilitites = Some(abilities);
            }

            let type_parameters = struct_hanlde
                .type_parameters
                .iter()
                .enumerate()
                .map(|(idx, x)| {
                    Ok(map_type_parameter(
                        Symbol::from(naming.templated_type(idx)),
                        *x,
                    )?)
                })
                .collect::<Result<Vec<DatatypeTypeParameter>, anyhow::Error>>()?;

            if let Some(a) = &struct_.type_parameters {
                if a != &type_parameters {
                    return Err(anyhow::anyhow!(
                        "Different type parameters for struct: {}::{}",
                        &module_id,
                        &struct_name
                    ));
                }
            } else {
                struct_.type_parameters = Some(type_parameters.clone());
            }
        }
    }

    {
        let vector_module_name = create_module_ident("0x1", "vector");

        let module = dummy_modules
            .entry(vector_module_name)
            .or_insert(Default::default());

        // only special functions which translated into bytecode
        let functions = &mut module.functions;
        functions.extend(
            [
                "empty",
                "length",
                "borrow",
                "borrow_mut",
                "push_back",
                "pop_back",
                "destroy_empty",
                "swap",
            ]
            .iter()
            .map(|&f| f.to_string()),
        );
    }

    for (&module_id, module) in &dummy_modules {
        // special case: we are decompiling the vector module from stdlib
        if module_id.value.module.0.value.as_str() == "vector" && modules.contains_key(&module_id) {
            continue;
        }

        let functions = UniqueMap::<FunctionName, Function>::maybe_from_iter(
            module.functions.iter().map(|fname| {
                (
                    FunctionName(span_(Symbol::from(fname.as_str()))),
                    Function {
                        attributes: UniqueMap::new(),
                        loc: fake_loc(),
                        visibility: move_compiler::expansion::ast::Visibility::Public(fake_loc()),
                        entry: None,
                        signature: FunctionSignature {
                            type_parameters: Vec::new(),
                            parameters: vec![],
                            return_type: builtin_type("u8"), //dummy
                        },
                        body: span_(FunctionBody_::Native),
                        warning_filter: move_compiler::diagnostics::WarningFilters::new_for_source(
                        ),
                        index: 0, //FIXME
                        macro_: None,
                    },
                )
            }),
        )
        .unwrap();

        let structs = UniqueMap::<DatatypeName, StructDefinition>::maybe_from_iter(
            module.structs.iter().map(|(sname, fields)| {
                (
                    DatatypeName(span_(Symbol::from(sname.as_str()))),
                    StructDefinition {
                        attributes: UniqueMap::new(),
                        loc: fake_loc(),
                        abilities: fields.abilitites.clone().unwrap_or(AbilitySet::empty()),
                        type_parameters: fields
                            .type_parameters
                            .clone()
                            .unwrap_or_else(|| Vec::new()),
                        fields: if fields.fields.is_empty() {
                            StructFields::Native(fake_loc())
                        } else {
                            StructFields::Named(
                                UniqueMap::<Field, (usize, Type)>::maybe_from_iter(
                                    fields.fields.iter().map(|fname| {
                                        (
                                            Field(span_(Symbol::from(fname.as_str()))),
                                            (0, builtin_type("u8")), //dummy
                                        )
                                    }),
                                )
                                .unwrap(),
                            )
                        },
                        warning_filter: move_compiler::diagnostics::WarningFilters::new_for_source(
                        ),
                        index: 0, //FIXME
                    },
                )
            }),
        )
        .unwrap();

        modules
            .add(
                module_id,
                ModuleDefinition {
                    package_name: None,
                    attributes: Attributes::new(),
                    loc: fake_loc(),
                    friends: UniqueMap::new(),
                    structs,
                    functions,
                    constants: UniqueMap::new(),
                    warning_filter: move_compiler::diagnostics::WarningFilters::new_for_source(),
                    target_kind: move_compiler::expansion::ast::TargetKind::External,
                    use_funs: Default::default(),
                    enums: UniqueMap::new(),
                },
            )
            .unwrap();
    }

    Ok(())
}

pub(crate) fn create_program(
    binaries: &Vec<BinaryIndexedView>,
    naming: &Naming,
) -> Result<(Program, HashMap<usize, CompiledModule>), anyhow::Error> {
    const LOC_MODULE_ID_BASE: usize = 0x10000;

    let mut modules = UniqueMap::<ModuleIdent, ModuleDefinition>::new();

    let adding_modules: Vec<_> = binaries
        .into_iter()
        .map(|binary| match binary {
            BinaryIndexedView::Module(compiled_module) => (*compiled_module).clone(),
        })
        .collect();

    let mut module_from_module_idx = HashMap::new();

    for (module_idx, compiled_module) in adding_modules.iter().enumerate() {
        let mut structs: UniqueMap<DatatypeName, StructDefinition> = UniqueMap::new();
        let mut functions: UniqueMap<FunctionName, Function> = UniqueMap::new();

        for struct_ in compiled_module.struct_defs() {
            let struct_handle = struct_.struct_handle;
            let name_idx = compiled_module.datatype_handle_at(struct_handle).name;
            let name_str = compiled_module.identifier_at(name_idx).as_str();
            let name = span_(Symbol::from(name_str));

            structs
                .add(
                    DatatypeName(name),
                    map_struct(compiled_module, struct_, naming)?,
                )
                .map_err(|(name, _)| anyhow::Error::msg(format!("Error adding struct {}", name)))?;
        }

        for function_ in compiled_module.function_defs() {
            let function_handle = function_.function;
            let name_idx = compiled_module.function_handle_at(function_handle).name;
            let name_str = compiled_module.identifier_at(name_idx).as_str();
            let name = span_(Symbol::from(name_str));

            functions
                .add(
                    FunctionName(name),
                    map_function(&compiled_module, function_, naming)?,
                )
                .map_err(|(name, _)| {
                    anyhow::Error::msg(format!("Error adding function {}", name))
                })?;
        }

        let module_loc_idx = LOC_MODULE_ID_BASE + module_idx;

        module_from_module_idx.insert(module_loc_idx, compiled_module.clone());

        modules
            .add(
                module_to_module_ident(compiled_module, &compiled_module.self_handle())?,
                ModuleDefinition {
                    package_name: None,
                    attributes: Attributes::new(),
                    loc: Loc::new(
                        FileHash::empty(),
                        module_loc_idx as u32,
                        module_loc_idx as u32,
                    ),
                    friends: UniqueMap::new(),
                    structs,
                    functions,
                    constants: UniqueMap::new(),
                    warning_filter: move_compiler::diagnostics::WarningFilters::new_for_source(),
                    target_kind: move_compiler::expansion::ast::TargetKind::Source {
                        is_root_package: false,
                    },
                    use_funs: Default::default(),
                    enums: UniqueMap::new(),
                },
            )
            .unwrap();
        ()
    }

    create_dummy_for_non_existing_modules(&mut modules, &adding_modules, &naming)?;

    Ok((Program { modules }, module_from_module_idx))
}
