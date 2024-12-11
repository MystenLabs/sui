// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::arena::Arena,
    execution::{
        dispatch_tables::{IntraPackageKey, PackageVirtualTable, VirtualTableKey},
        values::ConstantValue,
    },
    natives::functions::{NativeFunction, UnboxedNativeFunction},
    shared::{
        binary_cache::BinaryCache,
        constants::TYPE_DEPTH_MAX,
        types::{PackageStorageId, RuntimePackageId},
        vm_pointer::{self, VMPointer},
    },
    string_interner,
};

use move_binary_format::{
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        AbilitySet, CodeOffset, ConstantPoolIndex, EnumDefInstantiationIndex, EnumDefinitionIndex,
        FieldHandleIndex, FieldInstantiationIndex, FunctionDefinitionIndex,
        FunctionInstantiationIndex, LocalIndex, SignatureIndex, SignatureToken,
        StructDefInstantiationIndex, StructDefinitionIndex, VariantHandle, VariantHandleIndex,
        VariantInstantiationHandle, VariantInstantiationHandleIndex, VariantJumpTable,
        VariantJumpTableIndex, VariantTag,
    },
};
use move_core_types::{
    gas_algebra::AbstractMemorySize, identifier::Identifier, language_storage::ModuleId,
    vm_status::StatusCode,
};
use std::collections::BTreeMap;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------
/// Representation of a loaded package.
pub struct Package {
    pub storage_id: PackageStorageId,
    pub runtime_id: RuntimePackageId,

    // NB: this is under the package's context so we don't need to further resolve by
    // address in this table.
    pub loaded_modules: BinaryCache<Identifier, Module>,

    // NB: Package functions and code are allocated into this arena.
    pub package_arena: Arena,
    pub vtable: PackageVirtualTable,
}

// A LoadedModule is very similar to a CompiledModule but data is "transformed" to a representation
// more appropriate to execution.
// When code executes indexes in instructions are resolved against those runtime structure
// so that any data needed for execution is immediately available
#[derive(Debug)]
pub struct Module {
    #[allow(dead_code)]
    pub id: ModuleId,

    ///
    /// types as indexes into the package's vtable
    ///
    #[allow(dead_code)]
    pub type_refs: Vec<IntraPackageKey>,

    /// struct references carry the index into the global vector of types.
    /// That is effectively an indirection over the ref table:
    /// the instruction carries an index into this table which contains the index into the
    /// glabal table of types. No instantiation of generic types is saved into the global table.
    pub structs: Vec<StructDef>,
    /// materialized instantiations, whether partial or not
    pub struct_instantiations: Vec<StructInstantiation>,

    /// enum references carry the index into the global vector of types.
    /// That is effectively an indirection over the ref table:
    /// the instruction carries an index into this table which contains the index into the
    /// glabal table of types. No instantiation of generic types is saved into the global table.
    /// Note that variants are not carried in the global table as these should stay in sync with the
    /// enum type.
    pub enums: Vec<EnumDef>,
    /// materialized instantiations
    pub enum_instantiations: Vec<EnumInstantiation>,

    pub variant_handles: Vec<VariantHandle>,
    pub variant_instantiation_handles: Vec<VariantInstantiationHandle>,

    /// materialized instantiations, whether partial or not
    pub function_instantiations: Vec<FunctionInstantiation>,

    /// fields as a pair of index, first to the type, second to the field position in that type
    pub field_handles: Vec<FieldHandle>,
    /// materialized instantiations, whether partial or not
    pub field_instantiations: Vec<FieldInstantiation>,

    /// a map of single-token signature indices to type.
    /// Single-token signatures are usually indexed by the `SignatureIndex` in bytecode. For example,
    /// `VecMutBorrow(SignatureIndex)`, the `SignatureIndex` maps to a single `SignatureToken`, and
    /// hence, a single type.
    pub single_signature_token_map: BTreeMap<SignatureIndex, Type>,

    /// a map from signatures in instantiations to the `Vec<Type>` that reperesent it.
    pub instantiation_signatures: BTreeMap<SignatureIndex, Vec<Type>>,

    /// constant references carry an index into a global vector of values.
    pub constants: Vec<Constant>,
}

// A runtime constant
#[derive(Debug)]
pub struct Constant {
    pub value: ConstantValue,
    pub type_: Type,
    // Size of constant -- used for gas charging.
    pub size: u64,
}

// A runtime function
// #[derive(Debug)]
// https://github.com/rust-lang/rust/issues/70263
pub struct Function {
    #[allow(unused)]
    pub file_format_version: u32,
    pub is_entry: bool,
    pub index: FunctionDefinitionIndex,
    pub code: *const [Bytecode],
    pub parameters: Vec<Type>,
    pub return_: Vec<Type>,
    pub type_parameters: Vec<AbilitySet>,
    pub native: Option<NativeFunction>,
    pub def_is_native: bool,
    pub module: ModuleId,
    pub name: Identifier,
    pub locals_len: usize,
    pub jump_tables: Vec<VariantJumpTable>,
}

//
// Internal structures that are saved at the proper index in the proper tables to access
// execution information (interpreter).
// The following structs are internal to the loader and never exposed out.
// The `Loader` will create those struct and the proper table when loading a module.
// The `Resolver` uses those structs to return information to the `Interpreter`.
//

// The type of call -- there are two types:
// - Known: the function is known and the index is the index in the global table of functions
//   (e.g., intra-package calls, or possibly calls to framework/well-known external packages).
// - Virtual: the function is unknown and the index is the index in the global table of vtables
//   that will be filled in at a later time before execution.
pub enum CallType {
    Direct(VMPointer<Function>),
    Virtual(VirtualTableKey),
}

// A function instantiation.
#[derive(Debug)]
pub struct FunctionInstantiation {
    // index to `ModuleCache::functions` global table if a in-package call otherwise a virtual call
    pub handle: CallType,
    pub instantiation_idx: SignatureIndex,
}

#[derive(Debug)]
pub struct StructDef {
    // struct field count
    pub field_count: u16,
    // `ModuelCache::structs` global table index
    pub idx: VirtualTableKey,
}

#[derive(Debug)]
pub struct StructInstantiation {
    // struct field count
    pub field_count: u16,
    // `ModuleCache::structs` global table index. It is the generic type.
    pub def: VirtualTableKey,
    pub instantiation_idx: SignatureIndex,
}

// A field handle. The offset is the only used information when operating on a field
#[derive(Debug)]
pub struct FieldHandle {
    pub offset: usize,
    // `ModuelCache::structs` global table index. It is the generic type.
    pub owner: VirtualTableKey,
}

// A field instantiation. The offset is the only used information when operating on a field
#[derive(Debug)]
pub struct FieldInstantiation {
    pub offset: usize,
    // `ModuleCache::structs` global table index. It is the generic type.
    #[allow(unused)]
    pub owner: VirtualTableKey,
}

#[derive(Debug)]
pub struct EnumDef {
    // enum variant count
    #[allow(unused)]
    pub variant_count: u16,
    pub variants: Vec<VariantDef>,
    // `ModuelCache::types` global table index
    pub idx: VirtualTableKey,
}

#[derive(Debug)]
pub struct EnumInstantiation {
    // enum variant count
    pub variant_count_map: Vec<u16>,
    // `ModuelCache::types` global table index
    pub def: VirtualTableKey,
    pub instantiation_idx: SignatureIndex,
}

#[derive(Debug)]
pub struct VariantDef {
    #[allow(unused)]
    pub tag: u16,
    pub field_count: u16,
    #[allow(unused)]
    pub field_types: Vec<Type>,
}

// -------------------------------------------------------------------------------------------------
// Runtime Type representation
// -------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Type {
    Bool,
    U8,
    U64,
    U128,
    Address,
    Signer,
    Vector(Box<Type>),
    Datatype(VirtualTableKey),
    DatatypeInstantiation(Box<(VirtualTableKey, Vec<Type>)>),
    Reference(Box<Type>),
    MutableReference(Box<Type>),
    TyParam(u16),
    U16,
    U32,
    U256,
}

#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Datatype {
    Enum(EnumType),
    Struct(StructType),
}

#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EnumType {
    pub variants: Vec<VariantType>,
    pub enum_def: EnumDefinitionIndex,
}

#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VariantType {
    pub variant_name: Identifier,
    pub fields: Vec<Type>,
    pub field_names: Vec<Identifier>,
    pub enum_def: EnumDefinitionIndex,
    pub variant_tag: VariantTag,
}

#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StructType {
    pub fields: Vec<Type>,
    pub field_names: Vec<Identifier>,
    pub struct_def: StructDefinitionIndex,
}

// -------------------------------------------------------------------------------------------------
// Bytecode
// -------------------------------------------------------------------------------------------------

/// `Bytecode` is a VM instruction of variable size. The type of the bytecode (opcode) defines
/// the size of the bytecode.
///
/// Bytecodes operate on a stack machine and each bytecode has side effect on the stack and the
/// instruction stream.
#[derive(Clone, Eq, PartialEq)]
pub enum Bytecode {
    /// Pop and discard the value at the top of the stack.
    /// The value on the stack must be an copyable type.
    ///
    /// Stack transition:
    ///
    /// ```..., value -> ...```
    Pop,
    /// Return from function, possibly with values according to the return types in the
    /// function signature. The returned values are pushed on the stack.
    /// The function signature of the function being executed defines the semantic of
    /// the Ret opcode.
    ///
    /// Stack transition:
    ///
    /// ```..., arg_val(1), ..., arg_val(n) -> ..., return_val(1), ..., return_val(n)```
    Ret,
    /// Branch to the instruction at position `CodeOffset` if the value at the top of the stack
    /// is true. Code offsets are relative to the start of the instruction stream.
    ///
    /// Stack transition:
    ///
    /// ```..., bool_value -> ...```
    BrTrue(CodeOffset),
    /// Branch to the instruction at position `CodeOffset` if the value at the top of the stack
    /// is false. Code offsets are relative to the start of the instruction stream.
    ///
    /// Stack transition:
    ///
    /// ```..., bool_value -> ...```
    BrFalse(CodeOffset),
    /// Branch unconditionally to the instruction at position `CodeOffset`. Code offsets are
    /// relative to the start of the instruction stream.
    ///
    /// Stack transition: none
    Branch(CodeOffset),
    /// Push a U8 constant onto the stack.
    ///
    /// Stack transition:
    ///
    /// ```... -> ..., u8_value```
    LdU8(u8),
    /// Push a U64 constant onto the stack.
    ///
    /// Stack transition:
    ///
    /// ```... -> ..., u64_value```
    LdU64(u64),
    /// Push a U128 constant onto the stack.
    ///
    /// Stack transition:
    ///
    /// ```... -> ..., u128_value```
    LdU128(Box<u128>),
    /// Convert the value at the top of the stack into u8.
    ///
    /// Stack transition:
    ///
    /// ```..., integer_value -> ..., u8_value```
    CastU8,
    /// Convert the value at the top of the stack into u64.
    ///
    /// Stack transition:
    ///
    /// ```..., integer_value -> ..., u8_value```
    CastU64,
    /// Convert the value at the top of the stack into u128.
    ///
    /// Stack transition:
    ///
    /// ```..., integer_value -> ..., u128_value```
    CastU128,
    /// Push a `Constant` onto the stack. The value is loaded and deserialized (according to its
    /// type) from the `ConstantPool` via `ConstantPoolIndex`
    ///
    /// Stack transition:
    ///
    /// ```... -> ..., value```
    LdConst(ConstantPoolIndex),
    /// Push `true` onto the stack.
    ///
    /// Stack transition:
    ///
    /// ```... -> ..., true```
    LdTrue,
    /// Push `false` onto the stack.
    ///
    /// Stack transition:
    ///
    /// ```... -> ..., false```
    LdFalse,
    /// Push the local identified by `LocalIndex` onto the stack. The value is copied and the
    /// local is still safe to use.
    ///
    /// Stack transition:
    ///
    /// ```... -> ..., value```
    CopyLoc(LocalIndex),
    /// Push the local identified by `LocalIndex` onto the stack. The local is moved and it is
    /// invalid to use from that point on, unless a store operation writes to the local before
    /// any read to that local.
    ///
    /// Stack transition:
    ///
    /// ```... -> ..., value```
    MoveLoc(LocalIndex),
    /// Pop value from the top of the stack and store it into the function locals at
    /// position `LocalIndex`.
    ///
    /// Stack transition:
    ///
    /// ```..., value -> ...```
    StLoc(LocalIndex),
    /// Call a well-known (usually intra-package) function. The stack has the arguments pushed
    /// first to last. The arguments are consumed and pushed to the locals of the function. Return
    /// values are pushed on the stack and available to the caller.
    ///
    /// Stack transition:
    ///
    /// ```..., arg(1), arg(2), ...,  arg(n) -> ..., return_value(1), return_value(2), ..., return_value(k)```
    DirectCall(VMPointer<Function>),
    /// Call an unknown (inter-package) function. The stack has the arguments pushed first to
    /// last. The arguments are consumed and pushed to the locals of the function.
    /// Return values are pushed on the stack and available to the caller.
    ///
    /// Stack transition:
    ///
    /// ```..., arg(1), arg(2), ...,  arg(n) -> ..., return_value(1), return_value(2), ..., return_value(k)```
    ///
    /// The VTableKey must be resolved in the current package context to resolve it to a function
    /// that can be executed.
    VirtualCall(VirtualTableKey),
    CallGeneric(FunctionInstantiationIndex),
    /// Create an instance of the type specified via `DatatypeHandleIndex` and push it on the stack.
    /// The values of the fields of the struct, in the order they appear in the struct declaration,
    /// must be pushed on the stack. All fields must be provided.
    ///
    /// A Pack instruction must fully initialize an instance.
    ///
    /// Stack transition:
    ///
    /// ```..., field(1)_value, field(2)_value, ..., field(n)_value -> ..., instance_value```
    Pack(StructDefinitionIndex),
    PackGeneric(StructDefInstantiationIndex),
    /// Destroy an instance of a type and push the values bound to each field on the
    /// stack.
    ///
    /// The values of the fields of the instance appear on the stack in the order defined
    /// in the struct definition.
    ///
    /// This order makes Unpack<T> the inverse of Pack<T>. So `Unpack<T>; Pack<T>` is the identity
    /// for struct T.
    ///
    /// Stack transition:
    ///
    /// ```..., instance_value -> ..., field(1)_value, field(2)_value, ..., field(n)_value```
    Unpack(StructDefinitionIndex),
    UnpackGeneric(StructDefInstantiationIndex),
    /// Read a reference. The reference is on the stack, it is consumed and the value read is
    /// pushed on the stack.
    ///
    /// Reading a reference performs a copy of the value referenced.
    /// As such, ReadRef requires that the type of the value has the `Copy` ability.
    ///
    /// Stack transition:
    ///
    /// ```..., reference_value -> ..., value```
    ReadRef,
    /// Write to a reference. The reference and the value are on the stack and are consumed.
    ///
    ///
    /// WriteRef requires that the type of the value has the `Drop` ability as the previous value
    /// is lost
    ///
    /// Stack transition:
    ///
    /// ```..., value, reference_value -> ...```
    WriteRef,
    /// Convert a mutable reference to an immutable reference.
    ///
    /// Stack transition:
    ///
    /// ```..., reference_value -> ..., reference_value```
    FreezeRef,
    /// Load a mutable reference to a local identified by LocalIndex.
    ///
    /// The local must not be a reference.
    ///
    /// Stack transition:
    ///
    /// ```... -> ..., reference```
    MutBorrowLoc(LocalIndex),
    /// Load an immutable reference to a local identified by LocalIndex.
    ///
    /// The local must not be a reference.
    ///
    /// Stack transition:
    ///
    /// ```... -> ..., reference```
    ImmBorrowLoc(LocalIndex),
    /// Load a mutable reference to a field identified by `FieldHandleIndex`.
    /// The top of the stack must be a mutable reference to a type that contains the field
    /// definition.
    ///
    /// Stack transition:
    ///
    /// ```..., reference -> ..., field_reference```
    MutBorrowField(FieldHandleIndex),
    /// Load a mutable reference to a field identified by `FieldInstantiationIndex`.
    /// The top of the stack must be a mutable reference to a type that contains the field
    /// definition.
    ///
    /// Stack transition:
    ///
    /// ```..., reference -> ..., field_reference```
    MutBorrowFieldGeneric(FieldInstantiationIndex),
    /// Load an immutable reference to a field identified by `FieldHandleIndex`.
    /// The top of the stack must be a reference to a type that contains the field definition.
    ///
    /// Stack transition:
    ///
    /// ```..., reference -> ..., field_reference```
    ImmBorrowField(FieldHandleIndex),
    /// Load an immutable reference to a field identified by `FieldInstantiationIndex`.
    /// The top of the stack must be a reference to a type that contains the field definition.
    ///
    /// Stack transition:
    ///
    /// ```..., reference -> ..., field_reference```
    ImmBorrowFieldGeneric(FieldInstantiationIndex),
    /// Add the 2 u64 at the top of the stack and pushes the result on the stack.
    /// The operation aborts the transaction in case of overflow.
    ///
    /// Stack transition:
    ///
    /// ```..., u64_value(1), u64_value(2) -> ..., u64_value```
    Add,
    /// Subtract the 2 u64 at the top of the stack and pushes the result on the stack.
    /// The operation aborts the transaction in case of underflow.
    ///
    /// Stack transition:
    ///
    /// ```..., u64_value(1), u64_value(2) -> ..., u64_value```
    Sub,
    /// Multiply the 2 u64 at the top of the stack and pushes the result on the stack.
    /// The operation aborts the transaction in case of overflow.
    ///
    /// Stack transition:
    ///
    /// ```..., u64_value(1), u64_value(2) -> ..., u64_value```
    Mul,
    /// Perform a modulo operation on the 2 u64 at the top of the stack and pushes the
    /// result on the stack.
    ///
    /// Stack transition:
    ///
    /// ```..., u64_value(1), u64_value(2) -> ..., u64_value```
    Mod,
    /// Divide the 2 u64 at the top of the stack and pushes the result on the stack.
    /// The operation aborts the transaction in case of "divide by 0".
    ///
    /// Stack transition:
    ///
    /// ```..., u64_value(1), u64_value(2) -> ..., u64_value```
    Div,
    /// Bitwise OR the 2 u64 at the top of the stack and pushes the result on the stack.
    ///
    /// Stack transition:
    ///
    /// ```..., u64_value(1), u64_value(2) -> ..., u64_value```
    BitOr,
    /// Bitwise AND the 2 u64 at the top of the stack and pushes the result on the stack.
    ///
    /// Stack transition:
    ///
    /// ```..., u64_value(1), u64_value(2) -> ..., u64_value```
    BitAnd,
    /// Bitwise XOR the 2 u64 at the top of the stack and pushes the result on the stack.
    ///
    /// Stack transition:
    ///
    /// ```..., u64_value(1), u64_value(2) -> ..., u64_value```
    Xor,
    /// Logical OR the 2 bool at the top of the stack and pushes the result on the stack.
    ///
    /// Stack transition:
    ///
    /// ```..., bool_value(1), bool_value(2) -> ..., bool_value```
    Or,
    /// Logical AND the 2 bool at the top of the stack and pushes the result on the stack.
    ///
    /// Stack transition:
    ///
    /// ```..., bool_value(1), bool_value(2) -> ..., bool_value```
    And,
    /// Logical NOT the bool at the top of the stack and pushes the result on the stack.
    ///
    /// Stack transition:
    ///
    /// ```..., bool_value -> ..., bool_value```
    Not,
    /// Compare for equality the 2 value at the top of the stack and pushes the
    /// result on the stack.
    /// The values on the stack must have `Drop` as they will be consumed and destroyed.
    ///
    /// Stack transition:
    ///
    /// ```..., value(1), value(2) -> ..., bool_value```
    Eq,
    /// Compare for inequality the 2 value at the top of the stack and pushes the
    /// result on the stack.
    /// The values on the stack must have `Drop` as they will be consumed and destroyed.
    ///
    /// Stack transition:
    ///
    /// ```..., value(1), value(2) -> ..., bool_value```
    Neq,
    /// Perform a "less than" operation of the 2 u64 at the top of the stack and pushes the
    /// result on the stack.
    ///
    /// Stack transition:
    ///
    /// ```..., u64_value(1), u64_value(2) -> ..., bool_value```
    Lt,
    /// Perform a "greater than" operation of the 2 u64 at the top of the stack and pushes the
    /// result on the stack.
    ///
    /// Stack transition:
    ///
    /// ```..., u64_value(1), u64_value(2) -> ..., bool_value```
    Gt,
    /// Perform a "less than or equal" operation of the 2 u64 at the top of the stack and pushes
    /// the result on the stack.
    ///
    /// Stack transition:
    ///
    /// ```..., u64_value(1), u64_value(2) -> ..., bool_value```
    Le,
    /// Perform a "greater than or equal" than operation of the 2 u64 at the top of the stack
    /// and pushes the result on the stack.
    ///
    /// Stack transition:
    ///
    /// ```..., u64_value(1), u64_value(2) -> ..., bool_value```
    Ge,
    /// Abort execution with errorcode
    ///
    ///
    /// Stack transition:
    ///
    /// ```..., errorcode -> ...```
    Abort,
    /// No operation.
    ///
    /// Stack transition: none
    Nop,
    /// Shift the (second top value) left (top value) bits and pushes the result on the stack.
    ///
    /// Stack transition:
    ///
    /// ```..., u64_value(1), u64_value(2) -> ..., u64_value```
    Shl,
    /// Shift the (second top value) right (top value) bits and pushes the result on the stack.
    ///
    /// Stack transition:
    ///
    /// ```..., u64_value(1), u64_value(2) -> ..., u64_value```
    Shr,
    /// Create a vector by packing a statically known number of elements from the stack. Abort the
    /// execution if there are not enough number of elements on the stack to pack from or they don't
    /// have the same type identified by the SignatureIndex.
    ///
    /// Stack transition:
    ///
    /// ```..., e1, e2, ..., eN -> ..., vec[e1, e2, ..., eN]```
    VecPack(SignatureIndex, u64),
    /// Return the length of the vector,
    ///
    /// Stack transition:
    ///
    /// ```..., vector_reference -> ..., u64_value```
    VecLen(SignatureIndex),
    /// Acquire an immutable reference to the element at a given index of the vector. Abort the
    /// execution if the index is out of bounds.
    ///
    /// Stack transition:
    ///
    /// ```..., vector_reference, u64_value -> .., element_reference```
    VecImmBorrow(SignatureIndex),
    /// Acquire a mutable reference to the element at a given index of the vector. Abort the
    /// execution if the index is out of bounds.
    ///
    /// Stack transition:
    ///
    /// ```..., vector_reference, u64_value -> .., element_reference```
    VecMutBorrow(SignatureIndex),
    /// Add an element to the end of the vector.
    ///
    /// Stack transition:
    ///
    /// ```..., vector_reference, element -> ...```
    VecPushBack(SignatureIndex),
    /// Pop an element from the end of vector. Aborts if the vector is empty.
    ///
    /// Stack transition:
    ///
    /// ```..., vector_reference -> ..., element```
    VecPopBack(SignatureIndex),
    /// Destroy the vector and unpack a statically known number of elements onto the stack. Aborts
    /// if the vector does not have a length N.
    ///
    /// Stack transition:
    ///
    /// ```..., vec[e1, e2, ..., eN] -> ..., e1, e2, ..., eN```
    VecUnpack(SignatureIndex, u64),
    /// Swaps the elements at two indices in the vector. Abort the execution if any of the indice
    /// is out of bounds.
    ///
    /// ```..., vector_reference, u64_value(1), u64_value(2) -> ...```
    VecSwap(SignatureIndex),
    /// Push a U16 constant onto the stack.
    ///
    /// Stack transition:
    ///
    /// ```... -> ..., u16_value```
    LdU16(u16),
    /// Push a U32 constant onto the stack.
    ///
    /// Stack transition:
    ///
    /// ```... -> ..., u32_value```
    LdU32(u32),
    /// Push a U256 constant onto the stack.
    ///
    /// Stack transition:
    ///
    /// ```... -> ..., u256_value```
    LdU256(Box<move_core_types::u256::U256>),
    /// Convert the value at the top of the stack into u16.
    ///
    /// Stack transition:
    ///
    /// ```..., integer_value -> ..., u16_value```
    CastU16,
    /// Convert the value at the top of the stack into u32.
    ///
    /// Stack transition:
    ///
    /// ```..., integer_value -> ..., u32_value```
    CastU32,
    /// Convert the value at the top of the stack into u256.
    ///
    /// Stack transition:
    ///
    /// ```..., integer_value -> ..., u256_value```
    CastU256,
    /// Create a variant of the enum type specified via `VariantHandleIndex` and push it on the stack.
    /// The values of the fields of the variant, in the order they appear in the variant declaration,
    /// must be pushed on the stack. All fields for the variant must be provided.
    ///
    /// A PackVariant/PackVariantGeneric instruction must fully initialize an instance.
    ///
    /// Stack transition:
    ///
    /// ```..., field(1)_value, field(2)_value, ..., field(n)_value -> ..., variant_value```
    PackVariant(VariantHandleIndex),
    PackVariantGeneric(VariantInstantiationHandleIndex),
    /// Destroy a variant value specified by the `VariantHandleIndex` and push the values bound to
    /// each variant field on the stack.
    ///
    /// The values of the fields of the instance appear on the stack in the order defined
    /// in the enum variant's definition.
    ///
    /// This order makes UnpackVariant<T>(tag) the inverse of PackVariant<T>(tag). So
    /// `UnpackVariant<T>(tag); PackVariant<T>(tag)` is the identity for enum T and variant V with
    /// tag `t`.
    ///
    /// Stack transition:
    ///
    /// ```..., instance_value -> ..., field(1)_value, field(2)_value, ..., field(n)_value```
    UnpackVariant(VariantHandleIndex),
    UnpackVariantImmRef(VariantHandleIndex),
    UnpackVariantMutRef(VariantHandleIndex),
    UnpackVariantGeneric(VariantInstantiationHandleIndex),
    UnpackVariantGenericImmRef(VariantInstantiationHandleIndex),
    UnpackVariantGenericMutRef(VariantInstantiationHandleIndex),
    /// Branch on the tag value of the enum value reference that is on the top of the value stack,
    /// and jumps to the matching code offset for that tag within the `CodeUnit`. Code offsets are
    /// relative to the start of the instruction stream.
    ///
    /// Stack transition:
    /// ```..., enum_value_ref -> ...```
    VariantSwitch(VariantJumpTableIndex),
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl Module {
    pub fn struct_at(&self, idx: StructDefinitionIndex) -> VirtualTableKey {
        self.structs[idx.0 as usize].idx.clone()
    }

    pub fn struct_instantiation_at(&self, idx: u16) -> &StructInstantiation {
        &self.struct_instantiations[idx as usize]
    }

    pub fn function_instantiation_at(&self, idx: u16) -> &FunctionInstantiation {
        &self.function_instantiations[idx as usize]
    }

    pub fn field_count(&self, idx: u16) -> u16 {
        self.structs[idx as usize].field_count
    }

    pub fn field_instantiation_count(&self, idx: u16) -> u16 {
        self.struct_instantiations[idx as usize].field_count
    }

    pub fn field_offset(&self, idx: FieldHandleIndex) -> usize {
        self.field_handles[idx.0 as usize].offset
    }

    pub fn field_instantiation_offset(&self, idx: FieldInstantiationIndex) -> usize {
        self.field_instantiations[idx.0 as usize].offset
    }

    pub fn single_type_at(&self, idx: SignatureIndex) -> &Type {
        self.single_signature_token_map.get(&idx).unwrap()
    }

    pub fn instantiation_signature_at(
        &self,
        idx: SignatureIndex,
    ) -> Result<&Vec<Type>, PartialVMError> {
        self.instantiation_signatures.get(&idx).ok_or_else(|| {
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message("Instantiation signature not found".to_string())
        })
    }

    pub fn enum_at(&self, idx: EnumDefinitionIndex) -> VirtualTableKey {
        self.enums[idx.0 as usize].idx.clone()
    }

    pub fn enum_instantiation_at(&self, idx: EnumDefInstantiationIndex) -> &EnumInstantiation {
        &self.enum_instantiations[idx.0 as usize]
    }

    pub fn variant_at(&self, vidx: VariantHandleIndex) -> &VariantDef {
        let variant_handle = &self.variant_handles[vidx.0 as usize];
        let enum_def = &self.enums[variant_handle.enum_def.0 as usize];
        &enum_def.variants[variant_handle.variant as usize]
    }

    pub fn variant_handle_at(&self, vidx: VariantHandleIndex) -> &VariantHandle {
        &self.variant_handles[vidx.0 as usize]
    }

    pub fn variant_field_count(&self, vidx: VariantHandleIndex) -> (u16, VariantTag) {
        let variant = self.variant_at(vidx);
        (variant.field_count, variant.tag)
    }

    pub fn variant_instantiation_handle_at(
        &self,
        vidx: VariantInstantiationHandleIndex,
    ) -> &VariantInstantiationHandle {
        &self.variant_instantiation_handles[vidx.0 as usize]
    }

    pub fn variant_instantiantiation_field_count_and_tag(
        &self,
        vidx: VariantInstantiationHandleIndex,
    ) -> (u16, VariantTag) {
        let handle = self.variant_instantiation_handle_at(vidx);
        let enum_inst = &self.enum_instantiations[handle.enum_def.0 as usize];
        (
            enum_inst.variant_count_map[handle.variant as usize],
            handle.variant,
        )
    }

    pub fn constant_at(&self, idx: ConstantPoolIndex) -> &Constant {
        &self.constants[idx.0 as usize]
    }
}

impl Function {
    #[allow(unused)]
    pub fn file_format_version(&self) -> u32 {
        self.file_format_version
    }

    pub fn module_id(&self) -> &ModuleId {
        &self.module
    }

    pub fn index(&self) -> FunctionDefinitionIndex {
        self.index
    }

    pub fn local_count(&self) -> usize {
        self.locals_len
    }

    pub fn arg_count(&self) -> usize {
        self.parameters.len()
    }

    pub fn return_type_count(&self) -> usize {
        self.return_.len()
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn code(&self) -> &[Bytecode] {
        vm_pointer::ref_slice(self.code)
    }

    pub fn jump_tables(&self) -> &[VariantJumpTable] {
        &self.jump_tables
    }

    pub fn type_parameters(&self) -> &[AbilitySet] {
        &self.type_parameters
    }

    pub fn pretty_string(&self) -> String {
        let id = &self.module;
        format!(
            "0x{}::{}::{}",
            id.address(),
            id.name().as_str(),
            self.name.as_str()
        )
    }

    #[cfg(any(debug_assertions, feature = "tracing"))]
    pub fn pretty_short_string(&self) -> String {
        let id = &self.module;
        format!(
            "0x{}::{}::{}",
            id.address().short_str_lossless(),
            id.name().as_str(),
            self.name.as_str()
        )
    }

    pub fn is_native(&self) -> bool {
        self.def_is_native
    }

    pub fn get_native(&self) -> PartialVMResult<&UnboxedNativeFunction> {
        if cfg!(feature = "lazy_natives") {
            // If lazy_natives is configured, this is a MISSING_DEPENDENCY error, as we skip
            // checking those at module loading time.
            self.native.as_deref().ok_or_else(|| {
                PartialVMError::new(StatusCode::MISSING_DEPENDENCY)
                    .with_message(format!("Missing Native Function `{}`", self.name))
            })
        } else {
            // Otherwise this error should not happen, hence UNREACHABLE
            self.native.as_deref().ok_or_else(|| {
                PartialVMError::new(StatusCode::UNREACHABLE)
                    .with_message("Missing Native Function".to_string())
            })
        }
    }
}

impl Type {
    fn clone_impl(&self, depth: usize) -> PartialVMResult<Type> {
        self.apply_subst(|idx, _| Ok(Type::TyParam(idx)), depth)
    }

    fn apply_subst<F>(&self, subst: F, depth: usize) -> PartialVMResult<Type>
    where
        F: Fn(u16, usize) -> PartialVMResult<Type> + Copy,
    {
        if depth > TYPE_DEPTH_MAX {
            return Err(PartialVMError::new(StatusCode::VM_MAX_TYPE_DEPTH_REACHED));
        }
        let res = match self {
            Type::TyParam(idx) => subst(*idx, depth)?,
            Type::Bool => Type::Bool,
            Type::U8 => Type::U8,
            Type::U16 => Type::U16,
            Type::U32 => Type::U32,
            Type::U64 => Type::U64,
            Type::U128 => Type::U128,
            Type::U256 => Type::U256,
            Type::Address => Type::Address,
            Type::Signer => Type::Signer,
            Type::Vector(ty) => Type::Vector(Box::new(ty.apply_subst(subst, depth + 1)?)),
            Type::Reference(ty) => Type::Reference(Box::new(ty.apply_subst(subst, depth + 1)?)),
            Type::MutableReference(ty) => {
                Type::MutableReference(Box::new(ty.apply_subst(subst, depth + 1)?))
            }
            Type::Datatype(def_idx) => Type::Datatype(def_idx.clone()),
            Type::DatatypeInstantiation(def_inst) => {
                let (def_idx, instantiation) = &**def_inst;
                let mut inst = vec![];
                for ty in instantiation {
                    inst.push(ty.apply_subst(subst, depth + 1)?)
                }
                Type::DatatypeInstantiation(Box::new((def_idx.clone(), inst)))
            }
        };
        Ok(res)
    }

    pub fn subst(&self, ty_args: &[Type]) -> PartialVMResult<Type> {
        self.apply_subst(
            |idx, depth| match ty_args.get(idx as usize) {
                Some(ty) => ty.clone_impl(depth),
                None => Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!(
                            "type substitution failed: index out of bounds -- len {} got {}",
                            ty_args.len(),
                            idx
                        )),
                ),
            },
            1,
        )
    }

    #[allow(deprecated)]
    const LEGACY_BASE_MEMORY_SIZE: AbstractMemorySize = AbstractMemorySize::new(1);

    /// Returns the abstract memory size the data structure occupies.
    ///
    /// This kept only for legacy reasons.
    /// New applications should not use this.
    pub fn size(&self) -> AbstractMemorySize {
        use Type::*;

        match self {
            TyParam(_) | Bool | U8 | U16 | U32 | U64 | U128 | U256 | Address | Signer => {
                Self::LEGACY_BASE_MEMORY_SIZE
            }
            Vector(ty) | Reference(ty) | MutableReference(ty) => {
                Self::LEGACY_BASE_MEMORY_SIZE + ty.size()
            }
            Datatype(_) => Self::LEGACY_BASE_MEMORY_SIZE,
            DatatypeInstantiation(inst) => {
                let (_, tys) = &**inst;
                tys.iter()
                    .fold(Self::LEGACY_BASE_MEMORY_SIZE, |acc, ty| acc + ty.size())
            }
        }
    }

    pub fn from_const_signature(constant_signature: &SignatureToken) -> PartialVMResult<Self> {
        use SignatureToken as S;
        use Type as L;

        Ok(match constant_signature {
            S::Bool => L::Bool,
            S::U8 => L::U8,
            S::U16 => L::U16,
            S::U32 => L::U32,
            S::U64 => L::U64,
            S::U128 => L::U128,
            S::U256 => L::U256,
            S::Address => L::Address,
            S::Vector(inner) => L::Vector(Box::new(Self::from_const_signature(inner)?)),
            // Not yet supported
            S::Datatype(_) | S::DatatypeInstantiation(_) => {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message("Unable to load const type signature".to_string()),
                )
            }
            // Not allowed/Not meaningful
            S::TypeParameter(_) | S::Reference(_) | S::MutableReference(_) | S::Signer => {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message("Unable to load const type signature".to_string()),
                )
            }
        })
    }

    pub fn check_vec_ref(&self, inner_ty: &Type, is_mut: bool) -> PartialVMResult<Type> {
        match self {
            Type::MutableReference(inner) => match &**inner {
                Type::Vector(inner) => {
                    inner.check_eq(inner_ty)?;
                    Ok(inner.as_ref().clone())
                }
                _ => Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message("VecMutBorrow expects a vector reference".to_string()),
                ),
            },
            Type::Reference(inner) if !is_mut => match &**inner {
                Type::Vector(inner) => {
                    inner.check_eq(inner_ty)?;
                    Ok(inner.as_ref().clone())
                }
                _ => Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message("VecMutBorrow expects a vector reference".to_string()),
                ),
            },
            _ => Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("VecMutBorrow expects a vector reference".to_string()),
            ),
        }
    }

    pub fn check_eq(&self, other: &Self) -> PartialVMResult<()> {
        if self != other {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                    format!("Type mismatch: expected {:?}, got {:?}", self, other),
                ),
            );
        }
        Ok(())
    }

    pub fn check_ref_eq(&self, expected_inner: &Self) -> PartialVMResult<()> {
        match self {
            Type::MutableReference(inner) | Type::Reference(inner) => {
                inner.check_eq(expected_inner)
            }
            _ => Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("VecMutBorrow expects a vector reference".to_string()),
            ),
        }
    }
}

//**************************************************************************************************
// Debug
//**************************************************************************************************

impl ::std::fmt::Debug for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}#{}", self.name, self.index)
    }
}

impl ::std::fmt::Debug for Bytecode {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match self {
            Bytecode::Pop => write!(f, "Pop"),
            Bytecode::Ret => write!(f, "Ret"),
            Bytecode::BrTrue(a) => write!(f, "BrTrue({})", a),
            Bytecode::BrFalse(a) => write!(f, "BrFalse({})", a),
            Bytecode::Branch(a) => write!(f, "Branch({})", a),
            Bytecode::LdU8(a) => write!(f, "LdU8({})", a),
            Bytecode::LdU16(a) => write!(f, "LdU16({})", a),
            Bytecode::LdU32(a) => write!(f, "LdU32({})", a),
            Bytecode::LdU64(a) => write!(f, "LdU64({})", a),
            Bytecode::LdU128(a) => write!(f, "LdU128({})", a),
            Bytecode::LdU256(a) => write!(f, "LdU256({})", a),
            Bytecode::CastU8 => write!(f, "CastU8"),
            Bytecode::CastU16 => write!(f, "CastU16"),
            Bytecode::CastU32 => write!(f, "CastU32"),
            Bytecode::CastU64 => write!(f, "CastU64"),
            Bytecode::CastU128 => write!(f, "CastU128"),
            Bytecode::CastU256 => write!(f, "CastU256"),
            Bytecode::LdConst(a) => write!(f, "LdConst({})", a),
            Bytecode::LdTrue => write!(f, "LdTrue"),
            Bytecode::LdFalse => write!(f, "LdFalse"),
            Bytecode::CopyLoc(a) => write!(f, "CopyLoc({})", a),
            Bytecode::MoveLoc(a) => write!(f, "MoveLoc({})", a),
            Bytecode::StLoc(a) => write!(f, "StLoc({})", a),
            Bytecode::DirectCall(fun) => write!(f, "Call({})", fun.to_ref().name),
            Bytecode::VirtualCall(vtable_key) => {
                let string_interner = string_interner();
                let module_name = string_interner
                    .resolve_string(&vtable_key.inner_pkg_key.module_name, "module name")
                    .expect("Failed to find interned string");
                let member_name = string_interner
                    .resolve_string(&vtable_key.inner_pkg_key.member_name, "member name")
                    .expect("Failed to find interned string");
                write!(
                    f,
                    "Call(~{}::{}::{})",
                    vtable_key.package_key, module_name, member_name
                )
            }
            Bytecode::CallGeneric(ndx) => write!(f, "CallGeneric({})", ndx),
            Bytecode::Pack(a) => write!(f, "Pack({})", a),
            Bytecode::PackGeneric(a) => write!(f, "PackGeneric({})", a),
            Bytecode::Unpack(a) => write!(f, "Unpack({})", a),
            Bytecode::UnpackGeneric(a) => write!(f, "UnpackGeneric({})", a),
            Bytecode::ReadRef => write!(f, "ReadRef"),
            Bytecode::WriteRef => write!(f, "WriteRef"),
            Bytecode::FreezeRef => write!(f, "FreezeRef"),
            Bytecode::MutBorrowLoc(a) => write!(f, "MutBorrowLoc({})", a),
            Bytecode::ImmBorrowLoc(a) => write!(f, "ImmBorrowLoc({})", a),
            Bytecode::MutBorrowField(a) => write!(f, "MutBorrowField({:?})", a),
            Bytecode::MutBorrowFieldGeneric(a) => write!(f, "MutBorrowFieldGeneric({:?})", a),
            Bytecode::ImmBorrowField(a) => write!(f, "ImmBorrowField({:?})", a),
            Bytecode::ImmBorrowFieldGeneric(a) => write!(f, "ImmBorrowFieldGeneric({:?})", a),
            Bytecode::Add => write!(f, "Add"),
            Bytecode::Sub => write!(f, "Sub"),
            Bytecode::Mul => write!(f, "Mul"),
            Bytecode::Mod => write!(f, "Mod"),
            Bytecode::Div => write!(f, "Div"),
            Bytecode::BitOr => write!(f, "BitOr"),
            Bytecode::BitAnd => write!(f, "BitAnd"),
            Bytecode::Xor => write!(f, "Xor"),
            Bytecode::Shl => write!(f, "Shl"),
            Bytecode::Shr => write!(f, "Shr"),
            Bytecode::Or => write!(f, "Or"),
            Bytecode::And => write!(f, "And"),
            Bytecode::Not => write!(f, "Not"),
            Bytecode::Eq => write!(f, "Eq"),
            Bytecode::Neq => write!(f, "Neq"),
            Bytecode::Lt => write!(f, "Lt"),
            Bytecode::Gt => write!(f, "Gt"),
            Bytecode::Le => write!(f, "Le"),
            Bytecode::Ge => write!(f, "Ge"),
            Bytecode::Abort => write!(f, "Abort"),
            Bytecode::Nop => write!(f, "Nop"),
            Bytecode::VecPack(a, n) => write!(f, "VecPack({}, {})", a, n),
            Bytecode::VecLen(a) => write!(f, "VecLen({})", a),
            Bytecode::VecImmBorrow(a) => write!(f, "VecImmBorrow({})", a),
            Bytecode::VecMutBorrow(a) => write!(f, "VecMutBorrow({})", a),
            Bytecode::VecPushBack(a) => write!(f, "VecPushBack({})", a),
            Bytecode::VecPopBack(a) => write!(f, "VecPopBack({})", a),
            Bytecode::VecUnpack(a, n) => write!(f, "VecUnpack({}, {})", a, n),
            Bytecode::VecSwap(a) => write!(f, "VecSwap({})", a),
            Bytecode::PackVariant(handle) => {
                write!(f, "PackVariant({:?})", handle)
            }
            Bytecode::PackVariantGeneric(handle) => write!(f, "PackVariantGeneric({:?})", handle),
            Bytecode::UnpackVariant(handle) => write!(f, "UnpackVariant({:?})", handle),
            Bytecode::UnpackVariantGeneric(handle) => {
                write!(f, "UnpackVariantGeneric({:?})", handle)
            }
            Bytecode::UnpackVariantImmRef(handle) => {
                write!(f, "UnpackVariantImmRef({:?})", handle)
            }
            Bytecode::UnpackVariantGenericImmRef(handle) => {
                write!(f, "UnpackVariantGenericImmRef({:?})", handle)
            }
            Bytecode::UnpackVariantMutRef(handle) => {
                write!(f, "UnpackVariantMutRef({:?})", handle)
            }
            Bytecode::UnpackVariantGenericMutRef(handle) => {
                write!(f, "UnpackVariantGenericMutRef({:?})", handle)
            }
            Bytecode::VariantSwitch(jt) => write!(f, "VariantSwitch({:?})", jt),
        }
    }
}

impl std::fmt::Debug for CallType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallType::Direct(fun) => write!(f, "Known({})", fun.to_ref().name),
            CallType::Virtual(vtable_key) => {
                let string_interner = string_interner();
                let module_name = string_interner
                    .resolve_string(&vtable_key.inner_pkg_key.module_name, "module name")
                    .expect("Failed to find interned string");
                let member_name = string_interner
                    .resolve_string(&vtable_key.inner_pkg_key.member_name, "member name")
                    .expect("Failed to find interned string");
                write!(
                    f,
                    "Virtual({}::{}::{})",
                    vtable_key.package_key, module_name, member_name
                )
            }
        }
    }
}

// Manually implementing Debug for Package
impl std::fmt::Debug for Package {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Package")
            .field("storage_id", &self.storage_id)
            .field("runtime_id", &self.runtime_id)
            .field("loaded_modules", &self.loaded_modules)
            .field("vtable", &self.vtable)
            .finish()
    }
}
