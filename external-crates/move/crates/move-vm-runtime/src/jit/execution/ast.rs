// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::{
        arena::{Arena, ArenaBox, ArenaBuilder, ArenaVec},
        identifier_interner::{IdentifierInterner, IdentifierKey},
    },
    execution::{
        dispatch_tables::{IntraPackageKey, PackageVirtualTable, VirtualTableKey},
        values::ConstantValue,
    },
    natives::functions::{NativeFunction, UnboxedNativeFunction},
    shared::{
        constants::{MAX_TYPE_INSTANTIATION_NODES, TYPE_DEPTH_MAX},
        types::{OriginalId, VersionId},
        vm_pointer::VMPointer,
    },
};

use indexmap::IndexMap;
use move_binary_format::{
    errors::PartialVMResult,
    file_format::{
        AbilitySet, CodeOffset, DatatypeTyParameter, FunctionDefinitionIndex, LocalIndex,
        VariantTag, Visibility,
    },
    file_format_common::Opcodes,
    partial_vm_error,
};
use move_core_types::{
    account_address::AccountAddress, gas_algebra::AbstractMemorySize, identifier::Identifier,
    language_storage::ModuleId,
};
use std::collections::BTreeMap;

// -------------------------------------------------------------------------------------------------
// Package / Module Type Definitions
// -------------------------------------------------------------------------------------------------

// NB: Whenever possible, we want to keep the data defined here crate-private. Converting a type
// to `pub` is a last-resort solution to a visibility problem and should be accompanied by a
// comment explaining why the type needs to be public and why the fields of the type cannot be made
// private.

/// Representation of a loaded package.
pub struct Package {
    pub version_id: VersionId,
    pub original_id: OriginalId,

    // NB: this is under the package's context so we don't need to further resolve by
    // address in this table.
    pub(crate) loaded_modules: IndexMap<IdentifierKey, Module>,

    // NB: Package functions and code are allocated into this arena.
    pub(crate) package_arena: Arena,
    pub vtable: PackageVirtualTable,
}

// A LoadedModule is very similar to a CompiledModule but data is "transformed" to a representation
// more appropriate to execution.
// When code executes indexes in instructions are resolved against those runtime structure
// so that any data needed for execution is immediately available
#[derive(Debug)]
// The dead code warning is silenced on this struct as it these fields retain our only pointers to
// the arena-allocated data. It seems prudent to track them here.
#[allow(dead_code)]
pub(crate) struct Module {
    pub id: ModuleId,

    /// Types as indexes into the package's vtable
    pub type_refs: ArenaVec<IntraPackageKey>,

    /// [ALLOC] This vector (and sub-definitions) are allocated in the package arena
    pub functions: ArenaVec<Function>,

    /// Descriptors for all of the datatypes defined in the module -- these are the enums and
    /// structs defined in the module.
    pub datatype_descriptors: ArenaVec<DatatypeDescriptor>,

    /// struct references carry the index into the global vector of types.
    /// That is effectively an indirection over the ref table:
    /// the instruction carries an index into this table which contains the index into the
    /// global table of types. No instantiation of generic types is saved into the global table.
    /// [ALLOC] This vector (and sub-definitions) are allocated in the package arena
    pub structs: ArenaVec<StructDef>,
    /// materialized instantiations, whether partial or not
    /// [ALLOC] This vector (and sub-definitions) are allocated in the package arena
    pub struct_instantiations: ArenaVec<StructInstantiation>,

    /// enum references carry the index into the global vector of types.
    /// That is effectively an indirection over the ref table:
    /// the instruction carries an index into this table which contains the index into the
    /// global table of types. No instantiation of generic types is saved into the global table.
    /// Note that variants are not carried in the global table as these should stay in sync with the
    /// enum type.
    /// [ALLOC] This vector (and sub-definitions) are allocated in the package arena
    pub enums: ArenaVec<EnumDef>,
    /// materialized instantiations
    /// [ALLOC] This vector (and sub-definitions) are allocated in the package arena
    pub enum_instantiations: ArenaVec<EnumInstantiation>,

    /// [ALLOC] This vector (and sub-definitions) are allocated in the package arena
    pub variant_instantiations: ArenaVec<VariantInstantiation>,

    /// materialized instantiations, whether partial or not
    /// [ALLOC] This vector (and sub-definitions) are allocated in the package arena
    pub function_instantiations: ArenaVec<FunctionInstantiation>,

    /// fields as a pair of index, first to the type, second to the field position in that type
    /// [ALLOC] This vector (and sub-definitions) are allocated in the package arena
    pub field_handles: ArenaVec<FieldHandle>,
    /// materialized instantiations, whether partial or not
    /// [ALLOC] This vector (and sub-definitions) are allocated in the package arena
    pub field_instantiations: ArenaVec<FieldInstantiation>,

    /// a map from signatures in instantiations to the `ArenaVec<FormulatedType>` that represents
    /// it, with each type's substitution formula precomputed at translation time.
    /// [ALLOC] This vector (and sub-definitions) are allocated in the package arena
    pub instantiation_signatures: ArenaVec<ArenaVec<FormulatedType>>,

    /// constant references carry an index into a global vector of values.
    /// [ALLOC] This vector (and sub-definitions) are allocated in the package arena
    pub constants: ArenaVec<Constant>,
}

impl Drop for Module {
    fn drop(&mut self) {
        // We need to manually drop the arena-allocated functions to ensure their native
        // Arc fields are correctly dropped.
        // Note the provided drain iterator calls the destructor on its elements when it is
        // dropped, so this is sufficient.
        self.functions.drain();
    }
}

// A runtime constant
#[derive(Debug)]
pub(crate) struct Constant {
    pub(crate) value: ConstantValue,
    pub(crate) type_: ArenaType,
    // Size of constant -- used for gas charging.
    pub size: u64,
}

// A runtime function
// #[derive(Debug)]
// https://github.com/rust-lang/rust/issues/70263
pub(crate) struct Function {
    #[allow(dead_code)]
    pub file_format_version: u32,
    pub is_entry: bool,
    pub visibility: Visibility,
    pub index: FunctionDefinitionIndex,
    pub code: ArenaVec<Bytecode>,
    pub parameters: ArenaVec<ArenaType>,
    pub locals: ArenaVec<ArenaType>,
    pub return_: ArenaVec<ArenaType>,
    pub type_parameters: ArenaVec<AbilitySet>,
    // NOTE: This field is manually dropped in Function::drop() to prevent Arc leaks
    // Any value holding a `Function` needs to ensure it is correctly dropped.
    pub native: Option<NativeFunction>,
    pub def_is_native: bool,
    pub name: VirtualTableKey,
    pub locals_len: usize,
    pub jump_tables: ArenaVec<VariantJumpTable>,
}

impl Drop for Function {
    fn drop(&mut self) {
        // Take ownership of the Arc and drop it
        if let Some(native_fn) = self.native.take() {
            drop(native_fn);
        }
    }
}

// A variant jump table -- note that these are only full at the moment.
pub(crate) type VariantJumpTable = ArenaVec<CodeOffset>;

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
pub(crate) enum CallType {
    Direct(VMPointer<Function>),
    Virtual(VirtualTableKey),
}

// -----------------------------------------------
// Datatypes
// -----------------------------------------------

#[derive(Debug)]
pub(crate) struct StructDef {
    pub def_vtable_key: VirtualTableKey,
    pub abilities: AbilitySet,
    pub type_parameters: ArenaVec<DatatypeTyParameter>,
    pub fields: ArenaVec<ArenaType>,
    pub field_names: ArenaVec<IdentifierKey>,
}

#[derive(Debug)]
pub(crate) struct EnumDef {
    pub def_vtable_key: VirtualTableKey,
    pub abilities: AbilitySet,
    pub type_parameters: ArenaVec<DatatypeTyParameter>,
    #[allow(dead_code)]
    pub variant_count: u16,
    pub variants: ArenaVec<VariantDef>,
}

#[derive(Debug)]
pub(crate) struct VariantDef {
    pub variant_tag: VariantTag,
    pub variant_name: IdentifierKey,
    pub fields: ArenaVec<ArenaType>,
    pub field_names: ArenaVec<IdentifierKey>,
    pub enum_def: VMPointer<EnumDef>,
}

// -----------------------------------------------
// Instantiations
// -----------------------------------------------

// A function instantiation.
#[derive(Debug)]
pub(crate) struct FunctionInstantiation {
    pub handle: CallType,
    pub(crate) instantiation: VMPointer<ArenaVec<FormulatedType>>,
}

#[derive(Debug)]
pub(crate) struct StructInstantiation {
    // struct field count
    pub field_count: u16,
    pub def_vtable_key: VirtualTableKey,
    pub(crate) type_params: VMPointer<ArenaVec<FormulatedType>>,
}

// A field handle. The offset is the only used information when operating on a field
#[derive(Debug)]
pub(crate) struct FieldHandle {
    pub offset: usize,
    pub owner: VirtualTableKey,
}

// A field instantiation. The offset is the only used information when operating on a field
#[derive(Debug)]
pub(crate) struct FieldInstantiation {
    pub offset: usize,
    pub owner: VirtualTableKey,
}

#[derive(Debug)]
pub(crate) struct EnumInstantiation {
    // enum variant count
    #[allow(dead_code)]
    pub variant_count_map: ArenaVec<u16>,
    pub enum_def: VMPointer<EnumDef>,
    pub def_vtable_key: VirtualTableKey,
    pub type_params: VMPointer<ArenaVec<FormulatedType>>,
}

// A variant instantiation.
#[derive(Debug)]
pub(crate) struct VariantInstantiation {
    pub enum_inst: VMPointer<EnumInstantiation>,
    pub variant: VMPointer<VariantDef>,
}

// -------------------------------------------------------------------------------------------------
// Runtime Type representation
// -------------------------------------------------------------------------------------------------

pub(crate) enum ArenaType {
    Bool,
    U8,
    U64,
    U128,
    Address,
    Signer,
    Vector(ArenaBox<ArenaType>),
    Datatype(VirtualTableKey),
    DatatypeInstantiation(ArenaBox<(VirtualTableKey, ArenaVec<ArenaType>)>),
    Reference(ArenaBox<ArenaType>),
    MutableReference(ArenaBox<ArenaType>),
    TyParam(u16),
    U16,
    U32,
    U256,
}

#[derive(Debug)]
pub(crate) struct DatatypeDescriptor {
    pub name: IdentifierKey,
    pub defining_id: ModuleIdKey,
    pub original_id: ModuleIdKey,
    pub datatype_info: ArenaBox<Datatype>,
    /// The datatype's through-field measure (`value_depth` and `layout_size`), computed while
    /// the package is JIT'd: plain constants when the datatype is fully concrete, otherwise a
    /// formula whose local field structure is folded into its constants, with type parameters
    /// and datatype applications remaining symbolic terms folded by the dispatch tables under a
    /// transaction's linkage view.
    measure: DatatypeMeasure,
}

#[derive(Debug)]
pub(crate) enum Datatype {
    Enum(VMPointer<EnumDef>),
    Struct(VMPointer<StructDef>),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ModuleIdKey {
    address: AccountAddress,
    name: IdentifierKey,
}

// -------------------------------------------------------------------------------------------------
// Runtime Type representation
// -------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
// TODO: These should not contain VirtualTableKeys, but this is used and matched all over the
// adapter. We would need to abstract away types similar to how we abstract away values to
// accomplish this, which we leave as future work.
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

// -------------------------------------------------------------------------------------------------
// Bytecode
// -------------------------------------------------------------------------------------------------

/// `Bytecode` is a VM instruction of variable size. The type of the bytecode (opcode) defines
/// the size of the bytecode.
///
/// Bytecodes operate on a stack machine and each bytecode has side effect on the stack and the
/// instruction stream.
pub(crate) enum Bytecode {
    /// Pop and discard the value at the top of the stack.
    /// The value on the stack must be a copyable type.
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
    LdU128(ArenaBox<u128>),
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
    /// ```..., integer_value -> ..., u64_value```
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
    LdConst(VMPointer<Constant>),
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
    CallGeneric(VMPointer<FunctionInstantiation>),
    /// Create an instance of the type specified via `DatatypeHandleIndex` and push it on the stack.
    /// The values of the fields of the struct, in the order they appear in the struct declaration,
    /// must be pushed on the stack. All fields must be provided.
    ///
    /// A Pack instruction must fully initialize an instance.
    ///
    /// Stack transition:
    ///
    /// ```..., field(1)_value, field(2)_value, ..., field(n)_value -> ..., instance_value```
    Pack(VMPointer<StructDef>),
    PackGeneric(VMPointer<StructInstantiation>),
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
    Unpack(VMPointer<StructDef>),
    UnpackGeneric(VMPointer<StructInstantiation>),
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
    MutBorrowField(VMPointer<FieldHandle>),
    /// Load a mutable reference to a field identified by `FieldInstantiationIndex`.
    /// The top of the stack must be a mutable reference to a type that contains the field
    /// definition.
    ///
    /// Stack transition:
    ///
    /// ```..., reference -> ..., field_reference```
    MutBorrowFieldGeneric(VMPointer<FieldInstantiation>),
    /// Load an immutable reference to a field identified by `FieldHandleIndex`.
    /// The top of the stack must be a reference to a type that contains the field definition.
    ///
    /// Stack transition:
    ///
    /// ```..., reference -> ..., field_reference```
    ImmBorrowField(VMPointer<FieldHandle>),
    /// Load an immutable reference to a field identified by `FieldInstantiationIndex`.
    /// The top of the stack must be a reference to a type that contains the field definition.
    ///
    /// Stack transition:
    ///
    /// ```..., reference -> ..., field_reference```
    ImmBorrowFieldGeneric(VMPointer<FieldInstantiation>),
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
    VecPack(VMPointer<FormulatedType>, u64),
    /// Return the length of the vector,
    ///
    /// Stack transition:
    ///
    /// ```..., vector_reference -> ..., u64_value```
    VecLen(VMPointer<FormulatedType>),
    /// Acquire an immutable reference to the element at a given index of the vector. Abort the
    /// execution if the index is out of bounds.
    ///
    /// Stack transition:
    ///
    /// ```..., vector_reference, u64_value -> .., element_reference```
    VecImmBorrow(VMPointer<FormulatedType>),
    /// Acquire a mutable reference to the element at a given index of the vector. Abort the
    /// execution if the index is out of bounds.
    ///
    /// Stack transition:
    ///
    /// ```..., vector_reference, u64_value -> .., element_reference```
    VecMutBorrow(VMPointer<FormulatedType>),
    /// Add an element to the end of the vector.
    ///
    /// Stack transition:
    ///
    /// ```..., vector_reference, element -> ...```
    VecPushBack(VMPointer<FormulatedType>),
    /// Pop an element from the end of vector. Aborts if the vector is empty.
    ///
    /// Stack transition:
    ///
    /// ```..., vector_reference -> ..., element```
    VecPopBack(VMPointer<FormulatedType>),
    /// Destroy the vector and unpack a statically known number of elements onto the stack. Aborts
    /// if the vector does not have a length N.
    ///
    /// Stack transition:
    ///
    /// ```..., vec[e1, e2, ..., eN] -> ..., e1, e2, ..., eN```
    VecUnpack(VMPointer<FormulatedType>, u64),
    /// Swaps the elements at two indices in the vector. Abort the execution if any of the indices
    /// is out of bounds.
    ///
    /// ```..., vector_reference, u64_value(1), u64_value(2) -> ...```
    VecSwap(VMPointer<FormulatedType>),
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
    LdU256(ArenaBox<move_core_types::u256::U256>),
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
    PackVariant(VMPointer<VariantDef>),
    PackVariantGeneric(VMPointer<VariantInstantiation>),
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
    UnpackVariant(VMPointer<VariantDef>),
    UnpackVariantImmRef(VMPointer<VariantDef>),
    UnpackVariantMutRef(VMPointer<VariantDef>),
    UnpackVariantGeneric(VMPointer<VariantInstantiation>),
    UnpackVariantGenericImmRef(VMPointer<VariantInstantiation>),
    UnpackVariantGenericMutRef(VMPointer<VariantInstantiation>),
    /// Branch on the tag value of the enum value reference that is on the top of the value stack,
    /// and jumps to the matching code offset for that tag within the `CodeUnit`. Code offsets are
    /// relative to the start of the instruction stream.
    ///
    /// Stack transition:
    /// ```..., enum_value_ref -> ...```
    VariantSwitch(VMPointer<VariantJumpTable>),
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl Function {
    pub fn vtable_key(&self) -> &VirtualTableKey {
        &self.name
    }

    #[allow(dead_code)]
    pub fn file_format_version(&self) -> u32 {
        self.file_format_version
    }

    pub fn module_id(&self, interner: &IdentifierInterner) -> ModuleId {
        // [SAFETY] If this is an error, that means we have an uninterned identifier key, which
        // should never happen in a well-formed module. This is as good a time to panic as any.
        self.name.module_id(interner)
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

    #[allow(dead_code)]
    pub fn name_str(&self, interner: &IdentifierInterner) -> String {
        self.name(interner).to_string()
    }

    pub fn name(&self, interner: &IdentifierInterner) -> Identifier {
        self.name.member_name(interner)
    }

    pub fn code(&self) -> &[Bytecode] {
        &self.code
    }

    #[allow(dead_code)]
    pub fn jump_tables(&self) -> &[VariantJumpTable] {
        &self.jump_tables
    }

    pub fn type_parameters(&self) -> &[AbilitySet] {
        &self.type_parameters
    }

    pub fn pretty_string(&self, interner: &IdentifierInterner) -> String {
        self.name.to_string(interner)
    }

    #[allow(dead_code)]
    pub fn pretty_short_string(&self, interner: &IdentifierInterner) -> String {
        self.name.to_short_string(interner)
    }

    pub fn is_native(&self) -> bool {
        self.def_is_native
    }

    pub fn get_native(
        &self,
        _interner: &IdentifierInterner,
    ) -> PartialVMResult<&UnboxedNativeFunction> {
        if cfg!(feature = "lazy_natives") {
            // If lazy_natives is configured, this is a MISSING_DEPENDENCY error, as we skip
            // checking those at module loading time.
            self.native
                .as_deref()
                .ok_or_else(|| partial_vm_error!(MISSING_DEPENDENCY, "Missing Native Function"))
        } else {
            // Otherwise this error should not happen, hence UNREACHABLE
            self.native
                .as_deref()
                .ok_or_else(|| partial_vm_error!(UNREACHABLE, "Missing Native Function"))
        }
    }
}

impl CallType {
    fn vtable_key(&self) -> &VirtualTableKey {
        match self {
            CallType::Direct(vmpointer) => vmpointer.vtable_key(),
            CallType::Virtual(vtable_key) => vtable_key,
        }
    }
}

impl StructDef {
    pub fn datatype(&self) -> Type {
        Type::Datatype(self.def_vtable_key.clone())
    }

    pub fn field_count(&self) -> usize {
        self.fields.len()
    }
}

impl EnumDef {
    pub fn datatype(&self) -> Type {
        Type::Datatype(self.def_vtable_key.clone())
    }
}

impl VariantDef {
    pub fn datatype(&self) -> Type {
        Type::Datatype(self.enum_def.to_ref().def_vtable_key.clone())
    }

    pub fn field_count(&self) -> usize {
        self.fields.len()
    }
}

impl VariantInstantiation {
    pub fn field_count(&self) -> usize {
        self.variant.fields.len()
    }
}

impl ArenaType {
    /// Convert to a runtime type by performing a deep copy, after checking the term against the
    /// configured type-traversal limits. The copy is equivalent to substituting each `TyParam`
    /// for itself, so this is just an identity substitution kept limit-free by the up-front
    /// measure check.
    pub fn to_type(&self) -> PartialVMResult<Type> {
        self.measure().check()?;
        Ok(self.to_type_unchecked())
    }

    /// Deep-copy into a runtime type without checking limits. The traversal (and recursion
    /// depth) is bounded by the size of `self`, which was already bounded when the arena type
    /// was built at translation time.
    fn to_type_unchecked(&self) -> Type {
        match self {
            ArenaType::TyParam(idx) => Type::TyParam(*idx),
            ArenaType::Bool => Type::Bool,
            ArenaType::U8 => Type::U8,
            ArenaType::U16 => Type::U16,
            ArenaType::U32 => Type::U32,
            ArenaType::U64 => Type::U64,
            ArenaType::U128 => Type::U128,
            ArenaType::U256 => Type::U256,
            ArenaType::Address => Type::Address,
            ArenaType::Signer => Type::Signer,
            ArenaType::Vector(ty) => Type::Vector(Box::new(ty.to_type_unchecked())),
            ArenaType::Reference(ty) => Type::Reference(Box::new(ty.to_type_unchecked())),
            ArenaType::MutableReference(ty) => {
                Type::MutableReference(Box::new(ty.to_type_unchecked()))
            }
            ArenaType::Datatype(def_idx) => Type::Datatype(def_idx.clone()),
            ArenaType::DatatypeInstantiation(def_inst) => {
                let (def_idx, instantiation) = &**def_inst;
                let inst = instantiation
                    .iter()
                    .map(|ty| ty.to_type_unchecked())
                    .collect::<Vec<_>>();
                Type::DatatypeInstantiation(Box::new((def_idx.clone(), inst)))
            }
        }
    }
}

impl ModuleIdKey {
    pub(crate) fn from_parts(address: AccountAddress, name: IdentifierKey) -> Self {
        Self { address, name }
    }

    #[allow(dead_code)]
    pub fn as_id(&self, interner: &IdentifierInterner) -> ModuleId {
        let name = interner.resolve_ident(&self.name, "module id");
        ModuleId::new(self.address, name)
    }

    pub fn address(&self) -> &AccountAddress {
        &self.address
    }

    pub fn name(&self, interner: &IdentifierInterner) -> Identifier {
        interner.resolve_ident(&self.name, "module name")
    }
}

impl DatatypeDescriptor {
    pub(crate) fn new(
        name: IdentifierKey,
        defining_id: ModuleIdKey,
        original_id: ModuleIdKey,
        datatype_info: ArenaBox<Datatype>,
        measure: DatatypeMeasure,
    ) -> Self {
        Self {
            name,
            defining_id,
            original_id,
            datatype_info,
            measure,
        }
    }

    /// The datatype's through-field measure (see [`DatatypeMeasure`]).
    pub(crate) fn datatype_measure(&self) -> &DatatypeMeasure {
        &self.measure
    }

    pub fn type_parameters(&self) -> &[DatatypeTyParameter] {
        match self.datatype_info.inner_ref() {
            Datatype::Enum(vmpointer) => &vmpointer.type_parameters,
            Datatype::Struct(vmpointer) => &vmpointer.type_parameters,
        }
    }

    pub fn abilities(&self) -> &AbilitySet {
        match self.datatype_info.inner_ref() {
            Datatype::Enum(vmpointer) => &vmpointer.abilities,
            Datatype::Struct(vmpointer) => &vmpointer.abilities,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn virtual_key(&self) -> VirtualTableKey {
        match self.datatype_info.inner_ref() {
            Datatype::Enum(ptr) => ptr.to_ref().def_vtable_key.clone(),
            Datatype::Struct(ptr) => ptr.to_ref().def_vtable_key.clone(),
        }
    }

    pub(crate) fn intra_package_key(&self) -> IntraPackageKey {
        match self.datatype_info.inner_ref() {
            Datatype::Enum(ptr) => *ptr.def_vtable_key.intra_package_key(),
            Datatype::Struct(ptr) => *ptr.def_vtable_key.intra_package_key(),
        }
    }
}

impl Type {
    const LEGACY_BASE_MEMORY_SIZE: AbstractMemorySize = AbstractMemorySize::new(1);

    /// Abstract memory size of a non-recursive ("primitive") type. Unlike [`Type::size`], this
    /// does not traverse into element/field types, so it needs no traversal limits or config. It
    /// errors if called on a composite type (vector/reference/datatype instantiation).
    pub fn primitive_size(&self) -> PartialVMResult<AbstractMemorySize> {
        use Type::*;
        match self {
            TyParam(_) | Bool | U8 | U16 | U32 | U64 | U128 | U256 | Address | Signer
            | Datatype(_) => Ok(Self::LEGACY_BASE_MEMORY_SIZE),
            Vector(_) | Reference(_) | MutableReference(_) | DatatypeInstantiation(_) => {
                Err(partial_vm_error!(
                    UNKNOWN_INVARIANT_VIOLATION_ERROR,
                    "primitive_size called on a non-primitive type"
                ))
            }
        }
    }

    pub fn check_vec_ref(&self, inner_ty: &Type, is_mut: bool) -> PartialVMResult<Type> {
        match self {
            Type::MutableReference(inner) => match &**inner {
                Type::Vector(inner) => {
                    inner.check_eq(inner_ty)?;
                    Ok(inner.as_ref().clone())
                }
                _ => Err(partial_vm_error!(
                    UNKNOWN_INVARIANT_VIOLATION_ERROR,
                    "VecMutBorrow expects a vector reference"
                )),
            },
            Type::Reference(inner) if !is_mut => match &**inner {
                Type::Vector(inner) => {
                    inner.check_eq(inner_ty)?;
                    Ok(inner.as_ref().clone())
                }
                _ => Err(partial_vm_error!(
                    UNKNOWN_INVARIANT_VIOLATION_ERROR,
                    "VecMutBorrow expects a vector reference"
                )),
            },
            _ => Err(partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "VecMutBorrow expects a vector reference"
            )),
        }
    }

    pub fn check_eq(&self, other: &Self) -> PartialVMResult<()> {
        if self != other {
            return Err(partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "Type mismatch: expected {:?}, got {:?}",
                self,
                other
            ));
        }
        Ok(())
    }

    pub fn check_ref_eq(&self, expected_inner: &Self) -> PartialVMResult<()> {
        match self {
            Type::MutableReference(inner) | Type::Reference(inner) => {
                inner.check_eq(expected_inner)
            }
            _ => Err(partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "VecMutBorrow expects a vector reference"
            )),
        }
    }
}

impl DatatypeDescriptor {
    #[allow(dead_code)]
    pub(crate) fn datatype_key(&self) -> VirtualTableKey {
        match &self.datatype_info.inner_ref() {
            Datatype::Enum(vmpointer) => vmpointer.def_vtable_key.clone(),
            Datatype::Struct(vmpointer) => vmpointer.def_vtable_key.clone(),
        }
    }

    pub fn type_param_constraints(&self) -> impl ExactSizeIterator<Item = &AbilitySet> {
        let type_params = match self.datatype_info.inner_ref() {
            Datatype::Enum(vmpointer) => &vmpointer.type_parameters,
            Datatype::Struct(vmpointer) => &vmpointer.type_parameters,
        };
        type_params.iter().map(|param| &param.constraints)
    }
}

// -------------------------------------------------------------------------------------------------
// Type Measurement and Substitution
// -------------------------------------------------------------------------------------------------
// The VM bounds types with four distinct quantities, each with its own configured limit:
//
// - `type_size`: the syntactic node count of a type term (`max_type_instantiation_nodes`);
// - `type_depth`: the syntactic depth of a type term (`max_type_depth`);
// - `value_depth`: the depth of a *value* of the type, through datatype fields
//   (`max_value_nest_depth`);
// - `layout_size`: the node count of the type's generated layout, through datatype fields
//   (`max_type_to_layout_nodes`).
//
// All four used to be enforced by threading counters (`TypeSize`) or runtime field traversals
// through every recursive type operation. Instead, we *predict* each quantity with a formula
// and check the prediction up front, so rejection is pure arithmetic and no part of an
// oversized type, value, or layout is ever built. There are two formula kinds, precomputed at
// translation time:
//
// - [`MeasureFormula`], on every signature-pool term ([`FormulatedType`]): predicts the
//   `type_size` and `type_depth` of a substitution result. Checked on every type construction.
// - [`DatatypeMeasure`], on every [`DatatypeDescriptor`]: the through-field `value_depth` and
//   `layout_size` — written down as plain constants when the datatype is fully concrete, and as
//   a formula otherwise. Folded by the dispatch tables under a transaction's linkage view, and
//   checked on every value construction (`Pack`, `PackVariant`, `VecPack`, and their generic
//   forms) and every layout generation.
//
// All four quantities of a call frame's type arguments are cached on the frame (see
// [`TypeSizes`] and [`TypeArguments`]), computed once when the frame is created.

/// The measured syntactic extent of a type term: `type_size` is the total node count and
/// `type_depth` the maximum nesting depth (a leaf has one node and depth one). `TyParam`s count
/// as ordinary leaves. Note this says nothing about *values* of the type — see
/// [`ValueDepthFormula`] for that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeMeasure {
    pub type_size: u64,
    pub type_depth: u64,
}

impl TypeMeasure {
    /// Check this measure against the `type_depth` and `type_size` limits.
    pub fn check(&self) -> PartialVMResult<()> {
        if self.type_depth > TYPE_DEPTH_MAX {
            return Err(partial_vm_error!(VM_MAX_TYPE_DEPTH_REACHED));
        }
        if self.type_size > MAX_TYPE_INSTANTIATION_NODES {
            return Err(partial_vm_error!(VM_MAX_TYPE_NODES_REACHED));
        }
        Ok(())
    }
}

/// All four size quantities of a concrete type: the syntactic pair plus the through-field pair.
/// These are cached per type argument on every call frame (see [`TypeArguments`]), computed once
/// when the frame is created, so every later limit check against a frame's type arguments is
/// pure arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeSizes {
    pub type_size: u64,
    pub type_depth: u64,
    pub value_depth: u64,
    pub layout_size: u64,
}

impl TypeSizes {
    /// The syntactic pair, as consumed by [`TypeFormula`]s.
    pub fn type_measure(&self) -> TypeMeasure {
        TypeMeasure {
            type_size: self.type_size,
            type_depth: self.type_depth,
        }
    }
}

impl From<TypeSizes> for TypeMeasure {
    fn from(sizes: TypeSizes) -> Self {
        sizes.type_measure()
    }
}

/// One term of a [`MeasureFormula`]: how often a type parameter occurs in the term and how deep
/// its deepest occurrence sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormulaTerm {
    param: u16,
    occurrences: u64,
    depth_offset: u64,
}

/// A measure formula predicts the *syntactic* [`TypeMeasure`] of `subst(term, ty_args)` from
/// the measures of the `ty_args` alone, without performing the substitution:
///
/// ```text
/// type_size  = size_constant + Σᵢ occurrences(i) × type_size(ty_args[i])
/// type_depth = max(depth_constant, maxᵢ (depth_offset(i) + type_depth(ty_args[i])))
/// ```
///
/// The prediction matches the counters of the historical checked traversal exactly: that
/// traversal counted both the `TyParam` node itself and every node of the argument cloned in
/// for it, with the argument's nodes sitting one level *below* the occurrence. Checking the
/// prediction therefore accepts and rejects exactly the substitutions the traversal did. (This
/// over-counts relative to the true measure of the *result*: the result's true node count is
/// the prediction minus the total parameter occurrences.)
///
/// All arithmetic saturates: a saturated measure simply exceeds any configured limit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MeasureFormula {
    size_constant: u64,
    depth_constant: u64,
    /// One term per distinct type parameter, sorted by parameter index.
    terms: Vec<FormulaTerm>,
}

/// The shared surface of the heap- and arena-resident measure-formula representations
/// ([`MeasureFormula`] and [`ArenaMeasureFormula`]): the formula parts, plus the prediction
/// arithmetic implemented exactly once as provided methods over those parts.
pub trait TypeFormula {
    /// The formula's constant node count: the measure of the bare term (`TyParam`s count as
    /// leaves).
    fn size_constant(&self) -> u64;

    /// The formula's constant depth: the maximum nesting depth of the bare term.
    fn depth_constant(&self) -> u64;

    /// One term per distinct type parameter, sorted by parameter index.
    fn terms(&self) -> &[FormulaTerm];

    /// The formula's constant part as a measure: the measure of the term itself (`TyParam`s
    /// count as leaves).
    fn constant_measure(&self) -> TypeMeasure {
        TypeMeasure {
            type_size: self.size_constant(),
            type_depth: self.depth_constant(),
        }
    }

    /// Total number of type-parameter occurrences in the formula.
    fn occurrences(&self) -> u64 {
        self.terms()
            .iter()
            .fold(0u64, |acc, term| acc.saturating_add(term.occurrences))
    }

    /// Predict the measure of substituting arguments with the given measures into this
    /// formula's term (see [`MeasureFormula`] for the equations). Arguments may be anything a
    /// syntactic measure can be read from (e.g. frame-cached [`TypeSizes`]). Errors if the term
    /// mentions a type parameter with no argument.
    fn apply<M: Copy + Into<TypeMeasure>>(
        &self,
        arg_measures: &[M],
    ) -> PartialVMResult<TypeMeasure> {
        let mut type_size = self.size_constant();
        let mut type_depth = self.depth_constant();
        for term in self.terms() {
            let arg: TypeMeasure = arg_measures
                .get(term.param as usize)
                .copied()
                .ok_or_else(|| {
                    partial_vm_error!(
                        UNKNOWN_INVARIANT_VIOLATION_ERROR,
                        "type substitution failed: index out of bounds -- len {} got {}",
                        arg_measures.len(),
                        term.param
                    )
                })?
                .into();
            type_size = type_size.saturating_add(term.occurrences.saturating_mul(arg.type_size));
            type_depth = type_depth.max(term.depth_offset.saturating_add(arg.type_depth));
        }
        Ok(TypeMeasure {
            type_size,
            type_depth,
        })
    }
}

impl TypeFormula for MeasureFormula {
    fn size_constant(&self) -> u64 {
        self.size_constant
    }

    fn depth_constant(&self) -> u64 {
        self.depth_constant
    }

    fn terms(&self) -> &[FormulaTerm] {
        &self.terms
    }
}

impl MeasureFormula {
    /// Move this formula's terms into `arena`, producing the arena-resident form stored in
    /// loaded packages.
    pub(crate) fn allocate(self, arena: &ArenaBuilder) -> PartialVMResult<ArenaMeasureFormula> {
        let MeasureFormula {
            size_constant,
            depth_constant,
            terms,
        } = self;
        let terms = arena.alloc_vec(terms.into_iter())?;
        Ok(ArenaMeasureFormula {
            size_constant,
            depth_constant,
            terms,
        })
    }
}

/// The arena-resident form of a [`MeasureFormula`], stored on every [`FormulatedType`] in a
/// loaded package's signature pool so the package arena remains the sole owner of loaded-code
/// memory.
#[derive(Debug)]
pub(crate) struct ArenaMeasureFormula {
    size_constant: u64,
    depth_constant: u64,
    terms: ArenaVec<FormulaTerm>,
}

impl TypeFormula for ArenaMeasureFormula {
    fn size_constant(&self) -> u64 {
        self.size_constant
    }

    fn depth_constant(&self) -> u64 {
        self.depth_constant
    }

    fn terms(&self) -> &[FormulaTerm] {
        &self.terms
    }
}

// -------------------------------------------------------------------------------------------------
// Datatype (Through-Field) Measure
// -------------------------------------------------------------------------------------------------

/// The variable of a [`DatatypeFormula`] term.
#[derive(Debug)]
pub(crate) enum FieldVar {
    /// A type parameter of the datatype the formula describes.
    Param(u16),
    /// A datatype application (`Datatype` or `DatatypeInstantiation` subterm) appearing in a
    /// field of the datatype the formula describes. These stay symbolic at translation time
    /// because the referenced datatype may live in another package, whose descriptor is only
    /// resolvable under a transaction's linkage view; the dispatch tables fold them at runtime.
    App(VMPointer<ArenaType>),
}

/// One term of a [`DatatypeFormula`]: a variable, the number of value-nesting levels sitting
/// above its deepest occurrence in the datatype's fields (for `value_depth`), and how many
/// times it occurs (for `layout_size`).
#[derive(Debug)]
pub(crate) struct DatatypeFormulaTerm {
    pub(crate) var: FieldVar,
    pub(crate) depth_offset: u64,
    pub(crate) occurrences: u64,
}

/// The through-field sizes of a fully concrete datatype: the maximum nesting depth of a value
/// of the type, and the node count of its generated layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DatatypeSizes {
    pub(crate) value_depth: u64,
    pub(crate) layout_size: u64,
}

/// A datatype formula predicts the through-field quantities of a datatype instantiation from
/// the corresponding quantities of its variables:
///
/// ```text
/// value_depth = max(value_depth_constant, maxᵥ (depth_offset(v) + value_depth(v)))
/// layout_size = layout_size_constant + Σᵥ occurrences(v) × layout_size(v)
/// ```
///
/// The depth component is the seed of the runtime `DepthFormula`; the layout component is its
/// linear analogue. All purely local field structure (primitives, vectors, references, the
/// datatype's own node and nesting level, and — for enums — one layout node per variant) is
/// folded into the constants at translation time. The dispatch tables derive the
/// linkage-resolved formulas by folding the symbolic terms — see
/// `calculate_measure_of_datatype_and_cache`. All arithmetic saturates.
#[derive(Debug)]
pub(crate) struct DatatypeFormula {
    value_depth_constant: u64,
    layout_size_constant: u64,
    terms: ArenaVec<DatatypeFormulaTerm>,
}

impl DatatypeFormula {
    pub(crate) fn value_depth_constant(&self) -> u64 {
        self.value_depth_constant
    }

    pub(crate) fn layout_size_constant(&self) -> u64 {
        self.layout_size_constant
    }

    pub(crate) fn terms(&self) -> &[DatatypeFormulaTerm] {
        &self.terms
    }
}

/// The through-field measure of a datatype, living on its [`DatatypeDescriptor`],
/// arena-allocated and computed while the package is JIT'd. When the datatype is fully concrete
/// — no type parameters and no datatype references in its fields — the quantities are known
/// exactly at translation time and are written down as plain constants; otherwise they are a
/// [`DatatypeFormula`].
#[derive(Debug)]
pub(crate) enum DatatypeMeasure {
    Constant(DatatypeSizes),
    Formula(DatatypeFormula),
}

impl DatatypeMeasure {
    /// Compute the through-field measure for a datatype with the given field types (for enums,
    /// the fields of every variant), allocating any formula terms in `arena`.
    /// `extra_layout_nodes` is the datatype's flat layout overhead beyond its own node — one
    /// per variant for enums, zero for structs — mirroring the per-variant node the legacy
    /// layout traversal counted.
    pub(crate) fn for_datatype_fields<'a>(
        field_types: impl Iterator<Item = &'a ArenaType>,
        extra_layout_nodes: u64,
        arena: &ArenaBuilder,
    ) -> PartialVMResult<DatatypeMeasure> {
        // `prefix_depth` is the number of value-nesting levels strictly above the visited term
        // (starting at 1: the datatype itself).
        fn visit(
            ty: &ArenaType,
            prefix_depth: u64,
            value_depth_constant: &mut u64,
            layout_size_constant: &mut u64,
            apps: &mut Vec<DatatypeFormulaTerm>,
            params: &mut BTreeMap<u16, (u64, u64)>,
        ) {
            match ty {
                ArenaType::TyParam(idx) => {
                    let (offset, occurrences) = params.entry(*idx).or_insert((0, 0));
                    *offset = (*offset).max(prefix_depth);
                    *occurrences = occurrences.saturating_add(1);
                }
                ArenaType::Vector(inner)
                | ArenaType::Reference(inner)
                | ArenaType::MutableReference(inner) => {
                    *value_depth_constant =
                        (*value_depth_constant).max(prefix_depth.saturating_add(1));
                    *layout_size_constant = layout_size_constant.saturating_add(1);
                    visit(
                        inner,
                        prefix_depth.saturating_add(1),
                        value_depth_constant,
                        layout_size_constant,
                        apps,
                        params,
                    );
                }
                ArenaType::Datatype(_) | ArenaType::DatatypeInstantiation(_) => {
                    apps.push(DatatypeFormulaTerm {
                        var: FieldVar::App(VMPointer::from_ref(ty)),
                        depth_offset: prefix_depth,
                        occurrences: 1,
                    });
                }
                _ => {
                    *value_depth_constant =
                        (*value_depth_constant).max(prefix_depth.saturating_add(1));
                    *layout_size_constant = layout_size_constant.saturating_add(1);
                }
            }
        }
        // The datatype itself, plus its flat layout overhead.
        let mut value_depth_constant = 1u64;
        let mut layout_size_constant = 1u64.saturating_add(extra_layout_nodes);
        let mut terms = vec![];
        let mut params = BTreeMap::new();
        for field_ty in field_types {
            visit(
                field_ty,
                1,
                &mut value_depth_constant,
                &mut layout_size_constant,
                &mut terms,
                &mut params,
            );
        }
        terms.extend(
            params
                .into_iter()
                .map(|(param, (depth_offset, occurrences))| DatatypeFormulaTerm {
                    var: FieldVar::Param(param),
                    depth_offset,
                    occurrences,
                }),
        );
        if terms.is_empty() {
            // Fully concrete: just write the sizes down.
            Ok(DatatypeMeasure::Constant(DatatypeSizes {
                value_depth: value_depth_constant,
                layout_size: layout_size_constant,
            }))
        } else {
            let terms = arena.alloc_vec(terms.into_iter())?;
            Ok(DatatypeMeasure::Formula(DatatypeFormula {
                value_depth_constant,
                layout_size_constant,
                terms,
            }))
        }
    }
}

/// Fully-instantiated type arguments paired with all four of their size quantities, computed
/// once at construction (when a call frame is created) and passed down with the frame. Every
/// later limit check against these arguments is pure arithmetic. The pairing is private so the
/// sizes can never drift from the types.
#[derive(Debug, Clone)]
pub struct TypeArguments {
    types: Vec<Type>,
    sizes: Vec<TypeSizes>,
}

impl TypeArguments {
    /// Pair `types` with their sizes. `sizes_of` computes the quartet for each type — the
    /// dispatch tables provide this (the through-field quantities need datatype resolution
    /// under the transaction's linkage view); see `VMDispatchTables::make_type_arguments`.
    pub(crate) fn new(
        types: Vec<Type>,
        mut sizes_of: impl FnMut(&Type) -> PartialVMResult<TypeSizes>,
    ) -> PartialVMResult<Self> {
        let sizes = types
            .iter()
            .map(&mut sizes_of)
            .collect::<PartialVMResult<Vec<_>>>()?;
        Ok(Self { types, sizes })
    }

    pub fn empty() -> Self {
        Self {
            types: vec![],
            sizes: vec![],
        }
    }

    pub fn types(&self) -> &[Type] {
        &self.types
    }

    pub fn sizes(&self) -> &[TypeSizes] {
        &self.sizes
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}

/// An arena-resident type term paired with its substitution [`MeasureFormula`], computed once
/// at translation time. All fields are private: the only way to turn one of these into a
/// runtime `Type` is [`FormulatedType::instantiate`], which checks the predicted measure
/// against the limits before building anything.
#[derive(Debug)]
pub(crate) struct FormulatedType {
    ty: ArenaType,
    formula: ArenaMeasureFormula,
}

impl FormulatedType {
    /// Compute the formula for `ty` and store it alongside the term, with the formula's terms
    /// allocated in `arena`.
    pub(crate) fn new(ty: ArenaType, arena: &ArenaBuilder) -> PartialVMResult<Self> {
        let formula = ty.formula().allocate(arena)?;
        Ok(Self { ty, formula })
    }

    /// Read-only access to the underlying term.
    pub(crate) fn ty(&self) -> &ArenaType {
        &self.ty
    }

    /// The term's own measure — the formula constants (`TyParam`s count as leaves). Equal to
    /// the node count / max depth of the raw term, with no traversal.
    pub(crate) fn measure(&self) -> TypeMeasure {
        self.formula.constant_measure()
    }

    /// Total number of type-parameter occurrences in the term. The true node count of an
    /// instantiation is the predicted count minus this (see [`MeasureFormula`]).
    pub(crate) fn occurrences(&self) -> u64 {
        self.formula.occurrences()
    }

    /// Predict the measure of instantiating this term with arguments of the given measures —
    /// pure arithmetic, no traversal.
    pub(crate) fn predict(&self, ty_args: &TypeArguments) -> PartialVMResult<TypeMeasure> {
        self.formula.apply(ty_args.sizes())
    }

    /// Identity conversion to a runtime `Type` (`TyParam`s map to themselves), checked against
    /// the limits. This is the conversion for terms used outside a generic context.
    pub(crate) fn to_type(&self) -> PartialVMResult<Type> {
        self.measure().check()?;
        Ok(self.ty.to_type_unchecked())
    }

    /// Substitute `ty_args` into this term: check the predicted measure against the limits,
    /// then build. Rejection is pure arithmetic — no part of an oversized type is ever
    /// constructed. A term mentioning a parameter with no matching argument is an invariant
    /// violation (use [`FormulatedType::to_type`] for identity conversion).
    pub(crate) fn instantiate(&self, ty_args: &TypeArguments) -> PartialVMResult<Type> {
        self.predict(ty_args)?.check()?;
        self.ty.subst_unchecked(ty_args.types())
    }
}

impl Type {
    /// The syntactic measure of this term itself, counting `TyParam`s as ordinary leaves.
    /// Crate-private by design: measurement is the dispatch tables' concern — external callers
    /// go through them (e.g. `VMDispatchTables::sizes_of_type`), so every size and limit
    /// decision flows through one place.
    pub(crate) fn measure(&self) -> TypeMeasure {
        match self {
            Type::Vector(ty) | Type::Reference(ty) | Type::MutableReference(ty) => {
                let inner = ty.measure();
                TypeMeasure {
                    type_size: inner.type_size.saturating_add(1),
                    type_depth: inner.type_depth.saturating_add(1),
                }
            }
            Type::DatatypeInstantiation(def_inst) => {
                let (_, instantiation) = &**def_inst;
                let mut type_size = 1u64;
                let mut type_depth = 1u64;
                for ty in instantiation.iter() {
                    let m = ty.measure();
                    type_size = type_size.saturating_add(m.type_size);
                    type_depth = type_depth.max(m.type_depth.saturating_add(1));
                }
                TypeMeasure {
                    type_size,
                    type_depth,
                }
            }
            _ => TypeMeasure {
                type_size: 1,
                type_depth: 1,
            },
        }
    }
}

impl ArenaType {
    /// The syntactic measure of this term itself, counting `TyParam`s as ordinary leaves.
    pub(crate) fn measure(&self) -> TypeMeasure {
        match self {
            ArenaType::Vector(ty) | ArenaType::Reference(ty) | ArenaType::MutableReference(ty) => {
                let inner = ty.measure();
                TypeMeasure {
                    type_size: inner.type_size.saturating_add(1),
                    type_depth: inner.type_depth.saturating_add(1),
                }
            }
            ArenaType::DatatypeInstantiation(def_inst) => {
                let (_, instantiation) = &**def_inst;
                let mut type_size = 1u64;
                let mut type_depth = 1u64;
                for ty in instantiation.iter() {
                    let m = ty.measure();
                    type_size = type_size.saturating_add(m.type_size);
                    type_depth = type_depth.max(m.type_depth.saturating_add(1));
                }
                TypeMeasure {
                    type_size,
                    type_depth,
                }
            }
            _ => TypeMeasure {
                type_size: 1,
                type_depth: 1,
            },
        }
    }

    /// The substitution formula for this term. Costs one traversal of the (static) term. Hot
    /// paths do not use this: instantiation sites get their formulas precomputed at translation
    /// time (see [`FormulatedType`]).
    pub(crate) fn formula(&self) -> MeasureFormula {
        fn visit(
            ty: &ArenaType,
            depth: u64,
            size_constant: &mut u64,
            depth_constant: &mut u64,
            terms: &mut BTreeMap<u16, (u64, u64)>,
        ) {
            *size_constant = size_constant.saturating_add(1);
            *depth_constant = (*depth_constant).max(depth);
            match ty {
                ArenaType::TyParam(idx) => {
                    let (occurrences, offset) = terms.entry(*idx).or_insert((0, 0));
                    *occurrences = occurrences.saturating_add(1);
                    *offset = (*offset).max(depth);
                }
                ArenaType::Vector(ty)
                | ArenaType::Reference(ty)
                | ArenaType::MutableReference(ty) => {
                    visit(
                        ty,
                        depth.saturating_add(1),
                        size_constant,
                        depth_constant,
                        terms,
                    );
                }
                ArenaType::DatatypeInstantiation(def_inst) => {
                    let (_, instantiation) = &**def_inst;
                    for ty in instantiation.iter() {
                        visit(
                            ty,
                            depth.saturating_add(1),
                            size_constant,
                            depth_constant,
                            terms,
                        );
                    }
                }
                _ => (),
            }
        }
        let mut size_constant = 0u64;
        let mut depth_constant = 0u64;
        let mut term_map = BTreeMap::new();
        visit(
            self,
            1,
            &mut size_constant,
            &mut depth_constant,
            &mut term_map,
        );
        MeasureFormula {
            size_constant,
            depth_constant,
            terms: term_map
                .into_iter()
                .map(|(param, (occurrences, depth_offset))| FormulaTerm {
                    param,
                    occurrences,
                    depth_offset,
                })
                .collect(),
        }
    }

    /// Checked substitution: predict the result's measure, check it against the limits, and
    /// only then build the result. This is the only on-the-fly substitution entry point — the
    /// raw builder is private to this module, so there is no unchecked route to a substituted
    /// type.
    pub(crate) fn subst(&self, ty_args: &[Type]) -> PartialVMResult<Type> {
        let arg_measures = ty_args.iter().map(|ty| ty.measure()).collect::<Vec<_>>();
        self.formula().apply(&arg_measures)?.check()?;
        self.subst_unchecked(ty_args)
    }

    /// Substitute `ty_args` into this term WITHOUT enforcing size or depth limits. Private to
    /// this module by design: every route to a substituted type ([`ArenaType::subst`],
    /// [`FormulatedType::instantiate`]) checks a predicted measure against the limits before
    /// calling this.
    fn subst_unchecked(&self, ty_args: &[Type]) -> PartialVMResult<Type> {
        Ok(match self {
            ArenaType::TyParam(idx) => match ty_args.get(*idx as usize) {
                Some(ty) => ty.clone(),
                None => {
                    return Err(partial_vm_error!(
                        UNKNOWN_INVARIANT_VIOLATION_ERROR,
                        "type substitution failed: index out of bounds -- len {} got {}",
                        ty_args.len(),
                        idx
                    ));
                }
            },
            ArenaType::Bool => Type::Bool,
            ArenaType::U8 => Type::U8,
            ArenaType::U16 => Type::U16,
            ArenaType::U32 => Type::U32,
            ArenaType::U64 => Type::U64,
            ArenaType::U128 => Type::U128,
            ArenaType::U256 => Type::U256,
            ArenaType::Address => Type::Address,
            ArenaType::Signer => Type::Signer,
            ArenaType::Vector(ty) => Type::Vector(Box::new(ty.subst_unchecked(ty_args)?)),
            ArenaType::Reference(ty) => Type::Reference(Box::new(ty.subst_unchecked(ty_args)?)),
            ArenaType::MutableReference(ty) => {
                Type::MutableReference(Box::new(ty.subst_unchecked(ty_args)?))
            }
            ArenaType::Datatype(def_idx) => Type::Datatype(def_idx.clone()),
            ArenaType::DatatypeInstantiation(def_inst) => {
                let (def_idx, instantiation) = &**def_inst;
                let inst = instantiation
                    .iter()
                    .map(|ty| ty.subst_unchecked(ty_args))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                Type::DatatypeInstantiation(Box::new((def_idx.clone(), inst)))
            }
        })
    }
}

// -------------------------------------------------------------------------------------------------
// Into
// -------------------------------------------------------------------------------------------------

impl From<&Bytecode> for Opcodes {
    fn from(val: &Bytecode) -> Self {
        match val {
            Bytecode::Pop => Opcodes::POP,
            Bytecode::Ret => Opcodes::RET,
            Bytecode::BrTrue(_) => Opcodes::BR_TRUE,
            Bytecode::BrFalse(_) => Opcodes::BR_FALSE,
            Bytecode::Branch(_) => Opcodes::BRANCH,
            Bytecode::LdU8(_) => Opcodes::LD_U8,
            Bytecode::LdU64(_) => Opcodes::LD_U64,
            Bytecode::LdU128(_) => Opcodes::LD_U128,
            Bytecode::CastU8 => Opcodes::CAST_U8,
            Bytecode::CastU64 => Opcodes::CAST_U64,
            Bytecode::CastU128 => Opcodes::CAST_U128,
            Bytecode::LdConst(_) => Opcodes::LD_CONST,
            Bytecode::LdTrue => Opcodes::LD_TRUE,
            Bytecode::LdFalse => Opcodes::LD_FALSE,
            Bytecode::CopyLoc(_) => Opcodes::COPY_LOC,
            Bytecode::MoveLoc(_) => Opcodes::MOVE_LOC,
            Bytecode::StLoc(_) => Opcodes::ST_LOC,
            Bytecode::DirectCall(_) => Opcodes::CALL,
            Bytecode::VirtualCall(_) => Opcodes::CALL,
            Bytecode::CallGeneric(_) => Opcodes::CALL_GENERIC,
            Bytecode::Pack(_) => Opcodes::PACK,
            Bytecode::PackGeneric(_) => Opcodes::PACK_GENERIC,
            Bytecode::Unpack(_) => Opcodes::UNPACK,
            Bytecode::UnpackGeneric(_) => Opcodes::UNPACK_GENERIC,
            Bytecode::ReadRef => Opcodes::READ_REF,
            Bytecode::WriteRef => Opcodes::WRITE_REF,
            Bytecode::FreezeRef => Opcodes::FREEZE_REF,
            Bytecode::MutBorrowLoc(_) => Opcodes::MUT_BORROW_LOC,
            Bytecode::ImmBorrowLoc(_) => Opcodes::IMM_BORROW_LOC,
            Bytecode::MutBorrowField(_) => Opcodes::MUT_BORROW_FIELD,
            Bytecode::MutBorrowFieldGeneric(_) => Opcodes::MUT_BORROW_FIELD_GENERIC,
            Bytecode::ImmBorrowField(_) => Opcodes::IMM_BORROW_FIELD,
            Bytecode::ImmBorrowFieldGeneric(_) => Opcodes::IMM_BORROW_FIELD_GENERIC,
            Bytecode::Add => Opcodes::ADD,
            Bytecode::Sub => Opcodes::SUB,
            Bytecode::Mul => Opcodes::MUL,
            Bytecode::Mod => Opcodes::MOD,
            Bytecode::Div => Opcodes::DIV,
            Bytecode::BitOr => Opcodes::BIT_OR,
            Bytecode::BitAnd => Opcodes::BIT_AND,
            Bytecode::Xor => Opcodes::XOR,
            Bytecode::Shl => Opcodes::SHL,
            Bytecode::Shr => Opcodes::SHR,
            Bytecode::Or => Opcodes::OR,
            Bytecode::And => Opcodes::AND,
            Bytecode::Not => Opcodes::NOT,
            Bytecode::Eq => Opcodes::EQ,
            Bytecode::Neq => Opcodes::NEQ,
            Bytecode::Lt => Opcodes::LT,
            Bytecode::Gt => Opcodes::GT,
            Bytecode::Le => Opcodes::LE,
            Bytecode::Ge => Opcodes::GE,
            Bytecode::Abort => Opcodes::ABORT,
            Bytecode::Nop => Opcodes::NOP,
            Bytecode::VecPack(..) => Opcodes::VEC_PACK,
            Bytecode::VecLen(_) => Opcodes::VEC_LEN,
            Bytecode::VecImmBorrow(_) => Opcodes::VEC_IMM_BORROW,
            Bytecode::VecMutBorrow(_) => Opcodes::VEC_MUT_BORROW,
            Bytecode::VecPushBack(_) => Opcodes::VEC_PUSH_BACK,
            Bytecode::VecPopBack(_) => Opcodes::VEC_POP_BACK,
            Bytecode::VecUnpack(..) => Opcodes::VEC_UNPACK,
            Bytecode::VecSwap(_) => Opcodes::VEC_SWAP,
            Bytecode::LdU16(_) => Opcodes::LD_U16,
            Bytecode::LdU32(_) => Opcodes::LD_U32,
            Bytecode::LdU256(_) => Opcodes::LD_U256,
            Bytecode::CastU16 => Opcodes::CAST_U16,
            Bytecode::CastU32 => Opcodes::CAST_U32,
            Bytecode::CastU256 => Opcodes::CAST_U256,
            Bytecode::PackVariant(_) => Opcodes::PACK_VARIANT,
            Bytecode::PackVariantGeneric(_) => Opcodes::PACK_VARIANT_GENERIC,
            Bytecode::UnpackVariant(_) => Opcodes::UNPACK_VARIANT,
            Bytecode::UnpackVariantImmRef(_) => Opcodes::UNPACK_VARIANT_IMM_REF,
            Bytecode::UnpackVariantMutRef(_) => Opcodes::UNPACK_VARIANT_MUT_REF,
            Bytecode::UnpackVariantGeneric(_) => Opcodes::UNPACK_VARIANT_GENERIC,
            Bytecode::UnpackVariantGenericImmRef(_) => Opcodes::UNPACK_VARIANT_GENERIC_IMM_REF,
            Bytecode::UnpackVariantGenericMutRef(_) => Opcodes::UNPACK_VARIANT_GENERIC_MUT_REF,
            Bytecode::VariantSwitch(_) => Opcodes::VARIANT_SWITCH,
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Debug
// -------------------------------------------------------------------------------------------------

impl ::std::fmt::Debug for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}#{}", self.name, self.index)
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
            Bytecode::LdU128(a) => write!(f, "LdU128({})", **a),
            Bytecode::LdU256(a) => write!(f, "LdU256({})", **a),
            Bytecode::CastU8 => write!(f, "CastU8"),
            Bytecode::CastU16 => write!(f, "CastU16"),
            Bytecode::CastU32 => write!(f, "CastU32"),
            Bytecode::CastU64 => write!(f, "CastU64"),
            Bytecode::CastU128 => write!(f, "CastU128"),
            Bytecode::CastU256 => write!(f, "CastU256"),
            Bytecode::LdConst(a) => write!(f, "LdConst({})", a.to_ref().value),
            Bytecode::LdTrue => write!(f, "LdTrue"),
            Bytecode::LdFalse => write!(f, "LdFalse"),
            Bytecode::CopyLoc(a) => write!(f, "CopyLoc({})", a),
            Bytecode::MoveLoc(a) => write!(f, "MoveLoc({})", a),
            Bytecode::StLoc(a) => write!(f, "StLoc({})", a),
            Bytecode::DirectCall(fun) => write!(f, "Call({:?})", fun.name),
            Bytecode::VirtualCall(vtable_key) => {
                write!(f, "Call(~{:?})", vtable_key)
            }
            Bytecode::CallGeneric(inst) => write!(f, "CallGeneric({:?})", inst.handle.vtable_key()),
            Bytecode::Pack(a) => write!(f, "Pack({:?})", a.def_vtable_key),
            Bytecode::PackGeneric(a) => write!(f, "PackGeneric({:?})", a.def_vtable_key),
            Bytecode::Unpack(a) => write!(f, "Unpack({:?})", a.def_vtable_key),
            Bytecode::UnpackGeneric(a) => write!(f, "UnpackGeneric({:?})", a.def_vtable_key),
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
            Bytecode::VecPack(a, n) => write!(f, "VecPack({:?}, {})", a.to_ref(), n),
            Bytecode::VecLen(a) => write!(f, "VecLen({:?})", a.to_ref()),
            Bytecode::VecImmBorrow(a) => write!(f, "VecImmBorrow({:?})", a.to_ref()),
            Bytecode::VecMutBorrow(a) => write!(f, "VecMutBorrow({:?})", a.to_ref()),
            Bytecode::VecPushBack(a) => write!(f, "VecPushBack({:?})", a.to_ref()),
            Bytecode::VecPopBack(a) => write!(f, "VecPopBack({:?})", a.to_ref()),
            Bytecode::VecUnpack(a, n) => write!(f, "VecUnpack({:?}, {})", a.to_ref(), n),
            Bytecode::VecSwap(a) => write!(f, "VecSwap({:?})", a.to_ref()),
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
            CallType::Direct(fun) => write!(f, "Known({:?})", fun.vtable_key()),
            CallType::Virtual(vtable_key) => {
                write!(f, "Virtual({:?})", vtable_key)
            }
        }
    }
}

// Manually implementing Debug for Package
impl std::fmt::Debug for Package {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Package")
            .field("version_id", &self.version_id)
            .field("original_id", &self.original_id)
            .field("loaded_modules", &self.loaded_modules)
            .field("vtable", &self.vtable)
            .finish()
    }
}

impl std::fmt::Debug for ArenaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArenaType::Bool => write!(f, "bool"),
            ArenaType::U8 => write!(f, "u8"),
            ArenaType::U64 => write!(f, "u64"),
            ArenaType::U128 => write!(f, "u128"),
            ArenaType::Address => write!(f, "address"),
            ArenaType::Signer => write!(f, "signer"),
            ArenaType::Vector(inner) => write!(f, "vector<{:?}>", inner.inner_ref()),
            ArenaType::Datatype(key) => write!(f, "{:?}", key),
            ArenaType::DatatypeInstantiation(inst) => {
                // inst is an ArenaBox<(VirtualTableKey, ArenaVec<ArenaType>)>
                let (key, types) = inst.inner_ref();
                write!(f, "{:?}<", key)?;
                let types = types
                    .iter()
                    .map(|x| format!("{:?}", x) + ",")
                    .collect::<String>();
                write!(f, "{}>", types)
            }
            ArenaType::Reference(inner) => write!(f, "&{:?}", inner.inner_ref()),
            ArenaType::MutableReference(inner) => write!(f, "&mut {:?}", inner.inner_ref()),
            ArenaType::TyParam(idx) => write!(f, "T{}", idx),
            ArenaType::U16 => write!(f, "u16"),
            ArenaType::U32 => write!(f, "u32"),
            ArenaType::U256 => write!(f, "u256"),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Interned Display Printing
// -------------------------------------------------------------------------------------------------
// This is to easily print out interned structures, passing the interner around to resolve names.

/// Trait for types that can be displayed with an interner.
/// This is similar to `std::fmt::Display` but takes an interner as argument. It is used for
/// printing stack traces and other debug situations.
pub trait InternedDisplay<B: std::fmt::Write> {
    fn fmt(&self, f: &mut B, interner: &IdentifierInterner) -> ::std::fmt::Result;
}

impl<B: std::fmt::Write> InternedDisplay<B> for IdentifierKey {
    fn fmt(&self, f: &mut B, interner: &IdentifierInterner) -> ::std::fmt::Result {
        let name = interner.resolve_ident(self, "module name");
        write!(f, "{}", name)
    }
}

impl<B: std::fmt::Write> InternedDisplay<B> for VirtualTableKey {
    fn fmt(&self, f: &mut B, interner: &IdentifierInterner) -> ::std::fmt::Result {
        let str = self.to_short_string(interner);
        write!(f, "{}", str)
    }
}

impl<B: std::fmt::Write> InternedDisplay<B> for StructDef {
    fn fmt(&self, f: &mut B, interner: &IdentifierInterner) -> ::std::fmt::Result {
        // Name
        self.def_vtable_key.fmt(f, interner)?;
        // Fields (name: ty)
        let field_tys = self.fields.as_ref();
        let field_names = self.field_names.as_ref();
        if !field_tys.is_empty() {
            write!(f, " {{ ")?;
            for (i, ty) in field_tys.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                if let Some(name) = field_names.get(i) {
                    name.fmt(f, interner)?;
                    write!(f, ": ")?;
                } else {
                    write!(f, "_{}: ", i)?;
                }
                ty.fmt(f, interner)?;
            }
            write!(f, " }}")?;
        }
        Ok(())
    }
}

impl<B: std::fmt::Write> InternedDisplay<B> for EnumDef {
    fn fmt(&self, f: &mut B, interner: &IdentifierInterner) -> ::std::fmt::Result {
        // Name
        self.def_vtable_key.fmt(f, interner)?;
        // Variants (just names, compact)
        let variants = self.variants.as_ref();
        if !variants.is_empty() {
            write!(f, " {{ ")?;
            for (i, v) in variants.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                v.fmt(f, interner)?;
            }
            write!(f, " }}")?;
        }
        Ok(())
    }
}

impl<B: std::fmt::Write> InternedDisplay<B> for VariantDef {
    fn fmt(&self, f: &mut B, interner: &IdentifierInterner) -> ::std::fmt::Result {
        // Prefix with Enum name
        self.enum_def.to_ref().def_vtable_key.fmt(f, interner)?;
        write!(f, "::")?;
        // Variant name
        self.variant_name.fmt(f, interner)?;
        // Fields (name: ty)
        let tys = self.fields.as_ref();
        let names = self.field_names.as_ref();
        if !tys.is_empty() {
            write!(f, " {{ ")?;
            for (i, ty) in tys.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                if let Some(n) = names.get(i) {
                    n.fmt(f, interner)?;
                    write!(f, ": ")?;
                }
                ty.fmt(f, interner)?;
            }
            write!(f, " }}")?;
        }
        Ok(())
    }
}

impl<B: std::fmt::Write> InternedDisplay<B> for FunctionInstantiation {
    fn fmt(&self, f: &mut B, interner: &IdentifierInterner) -> ::std::fmt::Result {
        // callee
        self.handle.fmt(f, interner)?;
        // type args
        let targs = self.instantiation.to_ref();
        if !targs.is_empty() {
            write!(f, "<")?;
            for (i, t) in targs.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                t.fmt(f, interner)?;
            }
            write!(f, ">")?;
        }
        Ok(())
    }
}

impl<B: std::fmt::Write> InternedDisplay<B> for StructInstantiation {
    fn fmt(&self, f: &mut B, interner: &IdentifierInterner) -> ::std::fmt::Result {
        // Head
        self.def_vtable_key.fmt(f, interner)?;
        // <type params>
        let tps = self.type_params.to_ref();
        if !tps.is_empty() {
            write!(f, "<")?;
            for (i, t) in tps.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                t.fmt(f, interner)?;
            }
            write!(f, ">")?;
        }
        Ok(())
    }
}

impl<B: std::fmt::Write> InternedDisplay<B> for FieldHandle {
    fn fmt(&self, f: &mut B, interner: &IdentifierInterner) -> ::std::fmt::Result {
        self.owner.fmt(f, interner)?;
        write!(f, ".{}", self.offset)
    }
}

impl<B: std::fmt::Write> InternedDisplay<B> for FieldInstantiation {
    fn fmt(&self, f: &mut B, interner: &IdentifierInterner) -> ::std::fmt::Result {
        // Owner may be unused operationally, but it’s very helpful in logs
        self.owner.fmt(f, interner)?;
        write!(f, ".{}", self.offset)
    }
}

impl<B: std::fmt::Write> InternedDisplay<B> for EnumInstantiation {
    fn fmt(&self, f: &mut B, interner: &IdentifierInterner) -> ::std::fmt::Result {
        // Head
        self.def_vtable_key.fmt(f, interner)?;
        // <type params>
        let tps = self.type_params.to_ref();
        if !tps.is_empty() {
            write!(f, "<")?;
            for (i, t) in tps.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                t.fmt(f, interner)?;
            }
            write!(f, ">")?;
        }
        Ok(())
    }
}

impl<B: std::fmt::Write> InternedDisplay<B> for VariantInstantiation {
    fn fmt(&self, f: &mut B, interner: &IdentifierInterner) -> ::std::fmt::Result {
        // Enum head with type params from enum_inst
        let einst = self.enum_inst.to_ref();
        einst.def_vtable_key.fmt(f, interner)?;
        let tps = einst.type_params.to_ref();
        if !tps.is_empty() {
            write!(f, "<")?;
            for (i, t) in tps.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                t.fmt(f, interner)?;
            }
            write!(f, ">")?;
        }
        write!(f, "::")?;
        // Variant name
        self.variant.to_ref().variant_name.fmt(f, interner)
    }
}

impl<B: std::fmt::Write> InternedDisplay<B> for Bytecode {
    fn fmt(&self, f: &mut B, interner: &IdentifierInterner) -> ::std::fmt::Result {
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
            Bytecode::LdU128(a) => write!(f, "LdU128({})", **a),
            Bytecode::LdU256(a) => write!(f, "LdU256({})", **a),

            Bytecode::CastU8 => write!(f, "CastU8"),
            Bytecode::CastU16 => write!(f, "CastU16"),
            Bytecode::CastU32 => write!(f, "CastU32"),
            Bytecode::CastU64 => write!(f, "CastU64"),
            Bytecode::CastU128 => write!(f, "CastU128"),
            Bytecode::CastU256 => write!(f, "CastU256"),

            Bytecode::LdConst(a) => write!(f, "LdConst({})", a.to_ref().value),

            Bytecode::LdTrue => write!(f, "LdTrue"),
            Bytecode::LdFalse => write!(f, "LdFalse"),

            Bytecode::CopyLoc(a) => write!(f, "CopyLoc({})", a),
            Bytecode::MoveLoc(a) => write!(f, "MoveLoc({})", a),
            Bytecode::StLoc(a) => write!(f, "StLoc({})", a),

            // Calls
            Bytecode::DirectCall(fun) => {
                write!(f, "Call(")?;
                fun.vtable_key().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::VirtualCall(vtable_key) => {
                write!(f, "Call(~")?;
                vtable_key.fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::CallGeneric(inst) => {
                write!(f, "CallGeneric(")?;
                inst.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }

            // Structs
            Bytecode::Pack(a) => {
                write!(f, "Pack(")?;
                a.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::PackGeneric(a) => {
                write!(f, "PackGeneric(")?;
                a.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::Unpack(a) => {
                write!(f, "Unpack(")?;
                a.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::UnpackGeneric(a) => {
                write!(f, "UnpackGeneric(")?;
                a.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }

            // References & fields
            Bytecode::ReadRef => write!(f, "ReadRef"),
            Bytecode::WriteRef => write!(f, "WriteRef"),
            Bytecode::FreezeRef => write!(f, "FreezeRef"),
            Bytecode::MutBorrowLoc(a) => write!(f, "MutBorrowLoc({})", a),
            Bytecode::ImmBorrowLoc(a) => write!(f, "ImmBorrowLoc({})", a),
            Bytecode::MutBorrowField(h) => {
                write!(f, "MutBorrowField(")?;
                h.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::MutBorrowFieldGeneric(h) => {
                write!(f, "MutBorrowFieldGeneric(")?;
                h.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::ImmBorrowField(h) => {
                write!(f, "ImmBorrowField(")?;
                h.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::ImmBorrowFieldGeneric(h) => {
                write!(f, "ImmBorrowFieldGeneric(")?;
                h.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }

            // ALU / logic
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

            // Vectors
            Bytecode::VecPack(a, n) => {
                write!(f, "VecPack(")?;
                a.to_ref().fmt(f, interner)?;
                write!(f, ", {})", n)
            }
            Bytecode::VecLen(a) => {
                write!(f, "VecLen(")?;
                a.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::VecImmBorrow(a) => {
                write!(f, "VecImmBorrow(")?;
                a.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::VecMutBorrow(a) => {
                write!(f, "VecMutBorrow(")?;
                a.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::VecPushBack(a) => {
                write!(f, "VecPushBack(")?;
                a.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::VecPopBack(a) => {
                write!(f, "VecPopBack(")?;
                a.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::VecUnpack(a, n) => {
                write!(f, "VecUnpack(")?;
                a.to_ref().fmt(f, interner)?;
                write!(f, ", {})", n)
            }
            Bytecode::VecSwap(a) => {
                write!(f, "VecSwap(")?;
                a.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }

            // Variants
            Bytecode::PackVariant(h) => {
                write!(f, "PackVariant(")?;
                h.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::PackVariantGeneric(h) => {
                write!(f, "PackVariantGeneric(")?;
                h.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::UnpackVariant(h) => {
                write!(f, "UnpackVariant(")?;
                h.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::UnpackVariantGeneric(h) => {
                write!(f, "UnpackVariantGeneric(")?;
                h.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::UnpackVariantImmRef(h) => {
                write!(f, "UnpackVariantImmRef(")?;
                h.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::UnpackVariantGenericImmRef(h) => {
                write!(f, "UnpackVariantGenericImmRef(")?;
                h.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::UnpackVariantMutRef(h) => {
                write!(f, "UnpackVariantMutRef(")?;
                h.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }
            Bytecode::UnpackVariantGenericMutRef(h) => {
                write!(f, "UnpackVariantGenericMutRef(")?;
                h.to_ref().fmt(f, interner)?;
                write!(f, ")")
            }

            // Still using Debug for the jump table unless you have an InternedDisplay for it
            Bytecode::VariantSwitch(jt) => write!(f, "VariantSwitch({:?})", jt),
        }
    }
}

impl<B: std::fmt::Write> InternedDisplay<B> for CallType {
    fn fmt(&self, f: &mut B, interner: &IdentifierInterner) -> ::std::fmt::Result {
        match self {
            CallType::Direct(vmpointer) => vmpointer.vtable_key().fmt(f, interner),
            CallType::Virtual(vtable_key) => {
                write!(f, "~")?;
                vtable_key.fmt(f, interner)
            }
        }
    }
}

impl<B: std::fmt::Write> InternedDisplay<B> for FormulatedType {
    fn fmt(&self, f: &mut B, interner: &IdentifierInterner) -> ::std::fmt::Result {
        self.ty().fmt(f, interner)
    }
}

impl<B: std::fmt::Write> InternedDisplay<B> for ArenaType {
    fn fmt(&self, f: &mut B, interner: &IdentifierInterner) -> ::std::fmt::Result {
        match self {
            ArenaType::TyParam(idx) => write!(f, "T{}", idx),
            ArenaType::Bool => write!(f, "bool"),
            ArenaType::U8 => write!(f, "u8"),
            ArenaType::U16 => write!(f, "u16"),
            ArenaType::U32 => write!(f, "u32"),
            ArenaType::U64 => write!(f, "u64"),
            ArenaType::U128 => write!(f, "u128"),
            ArenaType::U256 => write!(f, "u256"),
            ArenaType::Address => write!(f, "address"),
            ArenaType::Signer => write!(f, "signer"),
            ArenaType::Vector(ty) => {
                write!(f, "vector<")?;
                ty.fmt(f, interner)?;
                write!(f, ">")
            }
            ArenaType::Reference(ty) => {
                write!(f, "&")?;
                ty.fmt(f, interner)
            }
            ArenaType::MutableReference(ty) => {
                write!(f, "&mut ")?;
                ty.fmt(f, interner)
            }
            ArenaType::Datatype(def_idx) => def_idx.fmt(f, interner),
            ArenaType::DatatypeInstantiation(def_inst) => {
                let (def_idx, instantiation) = &**def_inst;
                def_idx.fmt(f, interner)?;
                write!(f, "<")?;
                for (i, ty) in instantiation.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    ty.fmt(f, interner)?;
                }
                write!(f, ">")
            }
        }
    }
}
