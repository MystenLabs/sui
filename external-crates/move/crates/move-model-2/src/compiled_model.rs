// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::source_model::QualifiedMemberId;
use move_binary_format::file_format::{
    self, AbilitySet, CodeOffset, CodeUnit, CompiledModule, ConstantPoolIndex, DatatypeHandleIndex,
    DatatypeTyParameter, EnumDefinitionIndex, FieldHandleIndex, FunctionDefinition,
    FunctionDefinitionIndex, FunctionHandleIndex, IdentifierIndex, LocalIndex, MemberCount,
    SignatureIndex, SignatureToken, StructDefInstantiationIndex, StructDefinition,
    StructDefinitionIndex, StructFieldInformation, TypeParameterIndex, VariantHandleIndex,
    VariantInstantiationHandleIndex, VariantJumpTable, Visibility,
};
use move_core_types::{account_address::AccountAddress, u256::U256};
use move_symbol_pool::Symbol;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct BinaryModel {
    pub packages: BTreeMap<AccountAddress, Package>,
}

#[derive(Debug, Clone)]
pub struct Package {
    pub package_id: AccountAddress,
    pub modules: BTreeMap<Symbol, Module>,
}

#[derive(Debug, Clone)]
pub struct Module {
    pub name: Symbol,
    pub package: AccountAddress,
    pub structs: BTreeMap<Symbol, Struct>,
    pub functions: BTreeMap<Symbol, Function>,
    pub constants: BTreeMap<Symbol, Constant>,
    pub module: CompiledModule,
}

#[derive(Debug, Clone)]
pub struct Struct {
    pub name: Symbol,
    pub abilities: AbilitySet,
    pub type_parameters: Vec<DatatypeTyParameter>,
    pub fields: Vec<Field>,
    pub def_idx: StructDefinitionIndex,
}

#[derive(Debug, Clone)]
pub struct Enum {
    pub name: Symbol,
    pub abilities: AbilitySet,
    pub type_parameters: Vec<DatatypeTyParameter>,
    pub variants: Vec<Variant>,
    pub def_idx: EnumDefinitionIndex,
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub name: Symbol,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: Symbol,
    pub type_: Type,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: Symbol,
    pub type_parameters: Vec<AbilitySet>,
    pub parameters: Vec<Type>,
    pub returns: Vec<Type>,
    pub visibility: u8,
    pub code: Option<Code>,
    pub def_idx: FunctionDefinitionIndex,
}

#[repr(u8)]
#[derive(Debug, Clone)]
enum Modifiers {
    Private = 0x0,
    Public = 0x1,
    Package = 0x2,
    Entry = 0x80, // high bit reserved for entry `0x80` `0b1000 0000`
}

#[derive(Debug, Clone)]
pub struct Code {
    pub locals: Vec<Type>,
    pub code: Vec<Bytecode>,
}

#[derive(Debug, Clone)]
pub struct Constant {
    pub type_: Type,
    // refer to the value in the `CompiledModule`
    pub constant: ConstantPoolIndex,
}

#[derive(Debug, Clone)]
pub enum Type {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
    Vector(Box<Type>),
    Datatype(Box<QualifiedMemberId>),
    DatatypeInstantiation(Box<(QualifiedMemberId, Vec<Type>)>),
    Reference(Box<Type>),
    MutableReference(Box<Type>),
    TypeParameter(TypeParameterIndex),
}

#[derive(Debug, Clone)]
pub enum Bytecode {
    Nop,
    Pop,
    Ret,
    BrTrue(CodeOffset),
    BrFalse(CodeOffset),
    Branch(CodeOffset),
    LdConst(ConstantPoolIndex),
    LdTrue,
    LdFalse,
    LdU8(u8),
    LdU16(u16),
    LdU32(u32),
    LdU64(u64),
    LdU128(Box<u128>),
    LdU256(Box<U256>),
    CastU8,
    CastU16,
    CastU32,
    CastU64,
    CastU128,
    CastU256,
    Add,
    Sub,
    Mul,
    Mod,
    Div,
    BitOr,
    BitAnd,
    Xor,
    Or,
    And,
    Not,
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
    Shl,
    Shr,
    Abort,
    CopyLoc(LocalIndex),
    MoveLoc(LocalIndex),
    StLoc(LocalIndex),
    Call(Box<QualifiedMemberId>),
    CallGeneric(Box<(QualifiedMemberId, Vec<Type>)>),
    Pack(Box<QualifiedMemberId>),
    PackGeneric(Box<(QualifiedMemberId, Vec<Type>)>),
    Unpack(Box<QualifiedMemberId>),
    UnpackGeneric(Box<(QualifiedMemberId, Vec<Type>)>),
    MutBorrowLoc(LocalIndex),
    ImmBorrowLoc(LocalIndex),
    MutBorrowField(Box<FieldRef>),
    MutBorrowFieldGeneric(Box<(FieldRef, Vec<Type>)>),
    ImmBorrowField(Box<FieldRef>),
    ImmBorrowFieldGeneric(Box<(FieldRef, Vec<Type>)>),
    ReadRef,
    WriteRef,
    FreezeRef,
    VecPack(Box<(Type, u64)>),
    VecLen(Box<Type>),
    VecImmBorrow(Box<Type>),
    VecMutBorrow(Box<Type>),
    VecPushBack(Box<Type>),
    VecPopBack(Box<Type>),
    VecUnpack(Box<(Type, u64)>),
    VecSwap(Box<Type>),
    PackVariant(Box<(QualifiedMemberId, Symbol)>),
    PackVariantGeneric(Box<(QualifiedMemberId, Symbol, Vec<Type>)>),
    UnpackVariant(Box<(QualifiedMemberId, Symbol)>),
    UnpackVariantImmRef(Box<(QualifiedMemberId, Symbol)>),
    UnpackVariantMutRef(Box<(QualifiedMemberId, Symbol)>),
    UnpackVariantGeneric(Box<(QualifiedMemberId, Symbol, Vec<Type>)>),
    UnpackVariantGenericImmRef(Box<(QualifiedMemberId, Symbol, Vec<Type>)>),
    UnpackVariantGenericMutRef(Box<(QualifiedMemberId, Symbol, Vec<Type>)>),
    VariantSwitch(Box<(QualifiedMemberId, Vec<(Symbol, CodeOffset)>)>),
}

#[derive(Debug, Clone)]
pub struct FieldRef {
    pub struct_: QualifiedMemberId,
    pub field: MemberCount,
}

//**************************************************************************************************
// Construction
//**************************************************************************************************

impl BinaryModel {
    pub fn new(compiled_modules: &[CompiledModule]) -> Self {
        let mut packages = BTreeMap::new();

        for compiled_module in compiled_modules {
            let module = Module::new(compiled_module);
            let package = packages
                .entry(module.package)
                .or_insert_with(|| Package::new(module.package));
            package.insert(module);
        }

        Self { packages }
    }
}

impl Package {
    fn new(package_id: AccountAddress) -> Self {
        Self {
            package_id,
            modules: BTreeMap::new(),
        }
    }

    fn insert(&mut self, module: Module) {
        self.modules.insert(module.name, module);
    }
}

impl Module {
    fn new(compiled_module: &CompiledModule) -> Self {
        let module_id = compiled_module.self_id();
        let name = module_id.name().as_str().into();
        let package = *module_id.address();

        let structs = compiled_module
            .struct_defs()
            .iter()
            .enumerate()
            .map(|(idx, def)| {
                let struct_ = make_struct(compiled_module, def, StructDefinitionIndex(idx as u16));
                (struct_.name, struct_)
            })
            .collect::<BTreeMap<_, _>>();
        let functions = compiled_module
            .function_defs()
            .iter()
            .enumerate()
            .map(|(idx, def)| {
                let fun = make_fun(compiled_module, def, FunctionDefinitionIndex(idx as u16));
                (fun.name, fun)
            })
            .collect::<BTreeMap<_, _>>();
        let constants = BTreeMap::<Symbol, Constant>::new();

        Self {
            name,
            package,
            structs,
            functions,
            constants,
            module: compiled_module.clone(),
        }
    }
}

fn make_struct(
    module: &CompiledModule,
    def: &StructDefinition,
    def_idx: StructDefinitionIndex,
) -> Struct {
    let handle = module.datatype_handle_at(def.struct_handle);
    let name = identifier_at(module, handle.name);
    let abilities = handle.abilities;
    let type_parameters = handle.type_parameters.clone();
    let fields = match &def.field_information {
        StructFieldInformation::Native => vec![],
        StructFieldInformation::Declared(fields) => fields
            .iter()
            .map(|field| Field {
                name: identifier_at(module, field.name),
                type_: make_type(module, &field.signature.0),
            })
            .collect(),
    };

    Struct {
        name,
        abilities,
        type_parameters,
        fields,
        def_idx,
    }
}

fn make_fun(
    module: &CompiledModule,
    def: &FunctionDefinition,
    def_idx: FunctionDefinitionIndex,
) -> Function {
    let handle = module.function_handle_at(def.function);
    let name = identifier_at(module, handle.name);
    let mut visibility: u8 = match def.visibility {
        Visibility::Private => Modifiers::Private as u8,
        Visibility::Public => Modifiers::Public as u8,
        Visibility::Friend => Modifiers::Package as u8,
    };
    if def.is_entry {
        visibility |= Modifiers::Entry as u8;
    }
    let type_parameters = handle.type_parameters.clone();
    let parameters = module
        .signature_at(handle.parameters)
        .0
        .iter()
        .map(|token| make_type(module, token))
        .collect();
    let returns = module
        .signature_at(handle.return_)
        .0
        .iter()
        .map(|token| make_type(module, token))
        .collect();
    let code = def.code.as_ref().map(|code| make_code(module, code));

    Function {
        name,
        type_parameters,
        parameters,
        returns,
        visibility,
        code,
        def_idx,
    }
}

fn make_type(module: &CompiledModule, token: &SignatureToken) -> Type {
    use file_format::SignatureToken::*;
    match token {
        Bool => Type::Bool,
        U8 => Type::U8,
        U64 => Type::U64,
        U128 => Type::U128,
        Address => Type::Address,
        Signer => panic!("Signer type is not supported"),
        Vector(token) => Type::Vector(Box::new(make_type(module, &*token))),
        Datatype(handle_idx) => {
            let member_id = qualified_member_from_datatype_handle(module, *handle_idx);
            Type::Datatype(Box::new(member_id))
        }
        DatatypeInstantiation(datatype_inst) => {
            let (handle_idx, tokens) = &**datatype_inst;
            let member_id = qualified_member_from_datatype_handle(module, *handle_idx);
            let types = tokens
                .iter()
                .map(|token| make_type(module, token))
                .collect();
            Type::DatatypeInstantiation(Box::new((member_id, types)))
        }
        Reference(token) => Type::Reference(Box::new(make_type(module, token))),
        MutableReference(token) => Type::MutableReference(Box::new(make_type(module, token))),
        TypeParameter(idx) => Type::TypeParameter(*idx),
        U16 => Type::U16,
        U32 => Type::U32,
        U256 => Type::U256,
    }
}

fn make_code(module: &CompiledModule, code: &CodeUnit) -> Code {
    let CodeUnit {
        locals,
        code,
        jump_tables,
    } = code;
    let locals = module
        .signature_at(*locals)
        .0
        .iter()
        .map(|token| make_type(module, token))
        .collect();
    let code = code
        .iter()
        .map(|bytecode| make_bytecode(module, jump_tables, bytecode))
        .collect();

    Code { locals, code }
}

fn make_bytecode(
    module: &CompiledModule,
    jump_tables: &[VariantJumpTable],
    bytecode: &file_format::Bytecode,
) -> Bytecode {
    use file_format::Bytecode::*;
    match bytecode {
        Pop => Bytecode::Pop,
        Ret => Bytecode::Ret,
        BrTrue(offset) => Bytecode::BrTrue(*offset),
        BrFalse(offset) => Bytecode::BrFalse(*offset),
        Branch(offset) => Bytecode::Branch(*offset),
        LdU8(val) => Bytecode::LdU8(*val),
        LdU64(val) => Bytecode::LdU64(*val),
        LdU128(val) => Bytecode::LdU128(val.clone()),
        CastU8 => Bytecode::CastU8,
        CastU64 => Bytecode::CastU64,
        CastU128 => Bytecode::CastU128,
        LdConst(idx) => Bytecode::LdConst(*idx),
        LdTrue => Bytecode::LdTrue,
        LdFalse => Bytecode::LdFalse,
        CopyLoc(idx) => Bytecode::CopyLoc(*idx),
        MoveLoc(idx) => Bytecode::MoveLoc(*idx),
        StLoc(idx) => Bytecode::StLoc(*idx),
        Call(idx) => {
            let member_id = qualified_member_from_func_handle(module, *idx);
            Bytecode::Call(Box::new(member_id))
        }
        CallGeneric(idx) => {
            let func_inst = module.function_instantiation_at(*idx);
            let member_id = qualified_member_from_func_handle(module, func_inst.handle);
            let types = signature_to_types(module, func_inst.type_parameters);
            Bytecode::CallGeneric(Box::new((member_id, types)))
        }
        Pack(idx) => Bytecode::Pack(Box::new(resolve_struct(module, *idx))),
        PackGeneric(idx) => Bytecode::PackGeneric(Box::new(resolve_struct_generic(module, *idx))),
        Unpack(idx) => Bytecode::Unpack(Box::new(resolve_struct(module, *idx))),
        UnpackGeneric(idx) => {
            Bytecode::UnpackGeneric(Box::new(resolve_struct_generic(module, *idx)))
        }
        ReadRef => Bytecode::ReadRef,
        WriteRef => Bytecode::WriteRef,
        FreezeRef => Bytecode::FreezeRef,
        MutBorrowLoc(idx) => Bytecode::MutBorrowLoc(*idx),
        ImmBorrowLoc(idx) => Bytecode::ImmBorrowLoc(*idx),
        MutBorrowField(idx) => {
            let field_ref = field_ref_from_handle(module, *idx);
            Bytecode::MutBorrowField(Box::new(field_ref))
        }
        MutBorrowFieldGeneric(idx) => {
            let field_inst = module.field_instantiation_at(*idx);
            let field_ref = field_ref_from_handle(module, field_inst.handle);
            let types = signature_to_types(module, field_inst.type_parameters);
            Bytecode::MutBorrowFieldGeneric(Box::new((field_ref, types)))
        }
        ImmBorrowField(idx) => {
            let field_ref = field_ref_from_handle(module, *idx);
            Bytecode::ImmBorrowField(Box::new(field_ref))
        }
        ImmBorrowFieldGeneric(idx) => {
            let field_inst = module.field_instantiation_at(*idx);
            let field_ref = field_ref_from_handle(module, field_inst.handle);
            let types = signature_to_types(module, field_inst.type_parameters);
            Bytecode::ImmBorrowFieldGeneric(Box::new((field_ref, types)))
        }
        Add => Bytecode::Add,
        Sub => Bytecode::Sub,
        Mul => Bytecode::Mul,
        Mod => Bytecode::Mod,
        Div => Bytecode::Div,
        BitOr => Bytecode::BitOr,
        BitAnd => Bytecode::BitAnd,
        Xor => Bytecode::Xor,
        Or => Bytecode::Or,
        And => Bytecode::And,
        Not => Bytecode::Not,
        Eq => Bytecode::Eq,
        Neq => Bytecode::Neq,
        Lt => Bytecode::Lt,
        Gt => Bytecode::Gt,
        Le => Bytecode::Le,
        Ge => Bytecode::Ge,
        Abort => Bytecode::Abort,
        Nop => Bytecode::Nop,
        Shl => Bytecode::Shl,
        Shr => Bytecode::Shr,
        VecPack(idx, val) => {
            let vec_type = get_vector_signature_as_type(module, *idx);
            Bytecode::VecPack(Box::new((vec_type, *val)))
        }
        VecLen(idx) => {
            let vec_type = get_vector_signature_as_type(module, *idx);
            Bytecode::VecLen(Box::new(vec_type))
        }
        VecImmBorrow(idx) => {
            let vec_type = get_vector_signature_as_type(module, *idx);
            Bytecode::VecImmBorrow(Box::new(vec_type))
        }
        VecMutBorrow(idx) => {
            let vec_type = get_vector_signature_as_type(module, *idx);
            Bytecode::VecMutBorrow(Box::new(vec_type))
        }
        VecPushBack(idx) => {
            let vec_type = get_vector_signature_as_type(module, *idx);
            Bytecode::VecPushBack(Box::new(vec_type))
        }
        VecPopBack(idx) => {
            let vec_type = get_vector_signature_as_type(module, *idx);
            Bytecode::VecPopBack(Box::new(vec_type))
        }
        VecUnpack(idx, val) => {
            let vec_type = get_vector_signature_as_type(module, *idx);
            Bytecode::VecUnpack(Box::new((vec_type, *val)))
        }
        VecSwap(idx) => {
            let vec_type = get_vector_signature_as_type(module, *idx);
            Bytecode::VecSwap(Box::new(vec_type))
        }
        LdU16(val) => Bytecode::LdU16(*val),
        LdU32(val) => Bytecode::LdU32(*val),
        LdU256(val) => Bytecode::LdU256(val.clone()),
        CastU16 => Bytecode::CastU16,
        CastU32 => Bytecode::CastU32,
        CastU256 => Bytecode::CastU256,

        PackVariant(idx) => Bytecode::PackVariant(Box::new(resolve_variant(module, *idx))),
        PackVariantGeneric(idx) => {
            Bytecode::PackVariantGeneric(Box::new(resolve_variant_generic(module, *idx)))
        }
        UnpackVariant(idx) => Bytecode::UnpackVariant(Box::new(resolve_variant(module, *idx))),
        UnpackVariantImmRef(idx) => {
            Bytecode::UnpackVariantImmRef(Box::new(resolve_variant(module, *idx)))
        }
        UnpackVariantMutRef(idx) => {
            Bytecode::UnpackVariantMutRef(Box::new(resolve_variant(module, *idx)))
        }
        UnpackVariantGeneric(idx) => {
            Bytecode::UnpackVariantGeneric(Box::new(resolve_variant_generic(module, *idx)))
        }
        UnpackVariantGenericImmRef(idx) => {
            Bytecode::UnpackVariantGenericImmRef(Box::new(resolve_variant_generic(module, *idx)))
        }
        UnpackVariantGenericMutRef(idx) => {
            Bytecode::UnpackVariantGenericMutRef(Box::new(resolve_variant_generic(module, *idx)))
        }
        VariantSwitch(idx) => {
            let VariantJumpTable {
                head_enum,
                jump_table,
            } = &jump_tables[idx.0 as usize];
            let enum_def = module.enum_def_at(*head_enum);
            let offsets = match jump_table {
                file_format::JumpTableInner::Full(offsets) => enum_def
                    .variants
                    .iter()
                    .zip(offsets)
                    .map(|(variant, offset)| (identifier_at(module, variant.variant_name), *offset))
                    .collect(),
            };
            let member_id = qualified_member_from_datatype_handle(module, enum_def.enum_handle);
            Bytecode::VariantSwitch(Box::new((member_id, offsets)))
        }

        // deprecated
        ExistsDeprecated(_)
        | ExistsGenericDeprecated(_)
        | MoveFromDeprecated(_)
        | MoveFromGenericDeprecated(_)
        | MoveToDeprecated(_)
        | MoveToGenericDeprecated(_)
        | MutBorrowGlobalDeprecated(_)
        | MutBorrowGlobalGenericDeprecated(_)
        | ImmBorrowGlobalDeprecated(_)
        | ImmBorrowGlobalGenericDeprecated(_) => panic!("Unsupported bytecode"),
    }
}

//
// Utility functions
//

fn identifier_at(module: &CompiledModule, idx: IdentifierIndex) -> Symbol {
    module.identifier_at(idx).as_str().into()
}

fn resolve_struct(module: &CompiledModule, idx: StructDefinitionIndex) -> QualifiedMemberId {
    let struct_def = module.struct_def_at(idx);
    qualified_member_from_datatype_handle(module, struct_def.struct_handle)
}

fn resolve_struct_generic(
    module: &CompiledModule,
    idx: StructDefInstantiationIndex,
) -> (QualifiedMemberId, Vec<Type>) {
    let struct_inst = module.struct_instantiation_at(idx);
    let struct_def = module.struct_def_at(struct_inst.def);
    let member_id = qualified_member_from_datatype_handle(module, struct_def.struct_handle);
    let types = signature_to_types(module, struct_inst.type_parameters);
    (member_id, types)
}

fn resolve_variant(
    module: &CompiledModule,
    idx: VariantHandleIndex,
) -> (QualifiedMemberId, Symbol) {
    let variant_handle = module.variant_handle_at(idx);
    let enum_def = module.enum_def_at(variant_handle.enum_def);
    let variant_def = module.variant_def_at(variant_handle.enum_def, variant_handle.variant);
    let member_id = qualified_member_from_datatype_handle(module, enum_def.enum_handle);
    let variant_name = identifier_at(module, variant_def.variant_name);
    (member_id, variant_name)
}

fn resolve_variant_generic(
    module: &CompiledModule,
    idx: VariantInstantiationHandleIndex,
) -> (QualifiedMemberId, Symbol, Vec<Type>) {
    let variant_inst = module.variant_instantiation_handle_at(idx);
    let enum_inst = module.enum_instantiation_at(variant_inst.enum_def);
    let enum_def = module.enum_def_at(enum_inst.def);
    let variant_def = module.variant_def_at(enum_inst.def, variant_inst.variant);
    let member_id = qualified_member_from_datatype_handle(module, enum_def.enum_handle);
    let variant_name = identifier_at(module, variant_def.variant_name);
    let types = signature_to_types(module, enum_inst.type_parameters);
    (member_id, variant_name, types)
}

fn qualified_member_from_datatype_handle(
    module: &CompiledModule,
    handle_idx: DatatypeHandleIndex,
) -> QualifiedMemberId {
    let handle = module.datatype_handle_at(handle_idx);
    let module_handle = module.module_handle_at(handle.module);
    let address = *module.address_identifier_at(module_handle.address);
    let module_name = identifier_at(module, module_handle.name);
    let module_id = (address, module_name);
    let name = identifier_at(module, handle.name);
    (module_id, name)
}

fn qualified_member_from_func_handle(
    module: &CompiledModule,
    handle_idx: FunctionHandleIndex,
) -> QualifiedMemberId {
    let handle = module.function_handle_at(handle_idx);
    let module_handle = module.module_handle_at(handle.module);
    let address = *module.address_identifier_at(module_handle.address);
    let module_name = identifier_at(module, module_handle.name);
    let module_id = (address, module_name);
    let name = identifier_at(module, handle.name);
    (module_id, name)
}

fn field_ref_from_handle(module: &CompiledModule, handle: FieldHandleIndex) -> FieldRef {
    let field_handle = module.field_handle_at(handle);
    let struct_def = module.struct_def_at(field_handle.owner);
    let struct_ = qualified_member_from_datatype_handle(module, struct_def.struct_handle);
    let field = field_handle.field;
    FieldRef { struct_, field }
}

fn signature_to_types(module: &CompiledModule, sig_idx: SignatureIndex) -> Vec<Type> {
    module
        .signature_at(sig_idx)
        .0
        .iter()
        .map(|token| make_type(module, token))
        .collect()
}

fn get_vector_signature_as_type(module: &CompiledModule, sig_idx: SignatureIndex) -> Type {
    let mut vec_type = signature_to_types(module, sig_idx);
    if vec_type.len() != 1 {
        panic!("Bad vector signature")
    }
    vec_type.pop().unwrap()
}
