use std::collections::{BTreeMap, HashMap};

use move_binary_format::{
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        AbilitySet, Bytecode, EnumDefInstantiationIndex, EnumDefinitionIndex, FieldHandleIndex,
        FieldInstantiationIndex, FunctionDefinitionIndex, SignatureIndex, StructDefinitionIndex,
        VariantHandle, VariantHandleIndex, VariantInstantiationHandle,
        VariantInstantiationHandleIndex, VariantJumpTable, VariantTag,
    },
};
use move_vm_types::loaded_data::runtime_types::{CachedTypeIndex, Type};

use move_core_types::{
    account_address::AccountAddress,
    annotated_value as A,
    identifier::Identifier,
    language_storage::{ModuleId, StructTag},
    runtime_value as R,
    vm_status::StatusCode,
};

use crate::native_functions::{NativeFunction, UnboxedNativeFunction};

use super::{Loader, Resolver};

// A LoadedModule is very similar to a CompiledModule but data is "transformed" to a representation
// more appropriate to execution.
// When code executes indexes in instructions are resolved against those runtime structure
// so that any data needed for execution is immediately available
#[derive(Debug)]
pub(crate) struct LoadedModule {
    #[allow(dead_code)]
    pub(crate) id: ModuleId,

    //
    // types as indexes into the Loader type list
    //
    #[allow(dead_code)]
    pub(crate) type_refs: Vec<CachedTypeIndex>,

    // struct references carry the index into the global vector of types.
    // That is effectively an indirection over the ref table:
    // the instruction carries an index into this table which contains the index into the
    // glabal table of types. No instantiation of generic types is saved into the global table.
    pub(crate) structs: Vec<StructDef>,
    // materialized instantiations, whether partial or not
    pub(crate) struct_instantiations: Vec<StructInstantiation>,

    // enum references carry the index into the global vector of types.
    // That is effectively an indirection over the ref table:
    // the instruction carries an index into this table which contains the index into the
    // glabal table of types. No instantiation of generic types is saved into the global table.
    // Note that variants are not carried in the global table as these should stay in sync with the
    // enum type.
    pub(crate) enums: Vec<EnumDef>,
    // materialized instantiations
    pub(crate) enum_instantiations: Vec<EnumInstantiation>,

    pub(crate) variant_handles: Vec<VariantHandle>,
    pub(crate) variant_instantiation_handles: Vec<VariantInstantiationHandle>,

    // functions as indexes into the Loader function list
    // That is effectively an indirection over the ref table:
    // the instruction carries an index into this table which contains the index into the
    // glabal table of functions. No instantiation of generic functions is saved into
    // the global table.
    pub(crate) function_refs: Vec<usize>,
    // materialized instantiations, whether partial or not
    pub(crate) function_instantiations: Vec<FunctionInstantiation>,

    // fields as a pair of index, first to the type, second to the field position in that type
    pub(crate) field_handles: Vec<FieldHandle>,
    // materialized instantiations, whether partial or not
    pub(crate) field_instantiations: Vec<FieldInstantiation>,

    // function name to index into the Loader function list.
    // This allows a direct access from function name to `Function`
    pub(crate) function_map: HashMap<Identifier, usize>,

    // a map of single-token signature indices to type.
    // Single-token signatures are usually indexed by the `SignatureIndex` in bytecode. For example,
    // `VecMutBorrow(SignatureIndex)`, the `SignatureIndex` maps to a single `SignatureToken`, and
    // hence, a single type.
    pub(crate) single_signature_token_map: BTreeMap<SignatureIndex, Type>,

    // a map from signatures in instantiations to the `Vec<Type>` that reperesent it.
    pub(crate) instantiation_signatures: BTreeMap<SignatureIndex, Vec<Type>>,
}

impl LoadedModule {
    pub(crate) fn struct_at(&self, idx: StructDefinitionIndex) -> CachedTypeIndex {
        self.structs[idx.0 as usize].idx
    }

    pub(crate) fn struct_instantiation_at(&self, idx: u16) -> &StructInstantiation {
        &self.struct_instantiations[idx as usize]
    }

    pub(crate) fn function_at(&self, idx: u16) -> usize {
        self.function_refs[idx as usize]
    }

    pub(crate) fn function_instantiation_at(&self, idx: u16) -> &FunctionInstantiation {
        &self.function_instantiations[idx as usize]
    }

    pub(crate) fn field_count(&self, idx: u16) -> u16 {
        self.structs[idx as usize].field_count
    }

    pub(crate) fn field_instantiation_count(&self, idx: u16) -> u16 {
        self.struct_instantiations[idx as usize].field_count
    }

    pub(crate) fn field_offset(&self, idx: FieldHandleIndex) -> usize {
        self.field_handles[idx.0 as usize].offset
    }

    pub(crate) fn field_instantiation_offset(&self, idx: FieldInstantiationIndex) -> usize {
        self.field_instantiations[idx.0 as usize].offset
    }

    pub(crate) fn single_type_at(&self, idx: SignatureIndex) -> &Type {
        self.single_signature_token_map.get(&idx).unwrap()
    }

    pub(crate) fn instantiation_signature_at(
        &self,
        idx: SignatureIndex,
    ) -> Result<&Vec<Type>, PartialVMError> {
        self.instantiation_signatures.get(&idx).ok_or_else(|| {
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message("Instantiation signature not found".to_string())
        })
    }

    pub(crate) fn enum_at(&self, idx: EnumDefinitionIndex) -> CachedTypeIndex {
        self.enums[idx.0 as usize].idx
    }

    pub(crate) fn enum_instantiation_at(
        &self,
        idx: EnumDefInstantiationIndex,
    ) -> &EnumInstantiation {
        &self.enum_instantiations[idx.0 as usize]
    }

    pub(crate) fn variant_at(&self, vidx: VariantHandleIndex) -> &VariantDef {
        let variant_handle = &self.variant_handles[vidx.0 as usize];
        let enum_def = &self.enums[variant_handle.enum_def.0 as usize];
        &enum_def.variants[variant_handle.variant as usize]
    }

    pub(crate) fn variant_handle_at(&self, vidx: VariantHandleIndex) -> &VariantHandle {
        &self.variant_handles[vidx.0 as usize]
    }

    pub(crate) fn variant_field_count(&self, vidx: VariantHandleIndex) -> (u16, VariantTag) {
        let variant = self.variant_at(vidx);
        (variant.field_count, variant.tag)
    }

    pub(crate) fn variant_instantiation_handle_at(
        &self,
        vidx: VariantInstantiationHandleIndex,
    ) -> &VariantInstantiationHandle {
        &self.variant_instantiation_handles[vidx.0 as usize]
    }

    pub(crate) fn variant_instantiantiation_field_count_and_tag(
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
}

// A runtime function
// #[derive(Debug)]
// https://github.com/rust-lang/rust/issues/70263
pub(crate) struct Function {
    #[allow(unused)]
    pub(crate) file_format_version: u32,
    pub(crate) index: FunctionDefinitionIndex,
    pub(crate) code: Vec<Bytecode>,
    pub(crate) parameters: SignatureIndex,
    pub(crate) return_: SignatureIndex,
    pub(crate) type_parameters: Vec<AbilitySet>,
    pub(crate) native: Option<NativeFunction>,
    pub(crate) def_is_native: bool,
    pub(crate) module: ModuleId,
    pub(crate) name: Identifier,
    pub(crate) parameters_len: usize,
    pub(crate) locals_len: usize,
    pub(crate) return_len: usize,
    pub(crate) jump_tables: Vec<VariantJumpTable>,
}

impl Function {
    #[allow(unused)]
    pub(crate) fn file_format_version(&self) -> u32 {
        self.file_format_version
    }

    pub(crate) fn module_id(&self) -> &ModuleId {
        &self.module
    }

    pub(crate) fn index(&self) -> FunctionDefinitionIndex {
        self.index
    }

    pub(crate) fn get_resolver<'a>(
        &self,
        link_context: AccountAddress,
        loader: &'a Loader,
    ) -> Resolver<'a> {
        let (compiled, loaded) = loader.get_module(link_context, &self.module);
        Resolver::for_module(loader, compiled, loaded)
    }

    pub(crate) fn local_count(&self) -> usize {
        self.locals_len
    }

    pub(crate) fn arg_count(&self) -> usize {
        self.parameters_len
    }

    pub(crate) fn return_type_count(&self) -> usize {
        self.return_len
    }

    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    pub(crate) fn code(&self) -> &[Bytecode] {
        &self.code
    }

    pub(crate) fn jump_tables(&self) -> &[VariantJumpTable] {
        &self.jump_tables
    }

    pub(crate) fn type_parameters(&self) -> &[AbilitySet] {
        &self.type_parameters
    }

    pub(crate) fn pretty_string(&self) -> String {
        let id = &self.module;
        format!(
            "0x{}::{}::{}",
            id.address(),
            id.name().as_str(),
            self.name.as_str()
        )
    }

    #[cfg(any(debug_assertions, feature = "debugging"))]
    pub(crate) fn pretty_short_string(&self) -> String {
        let id = &self.module;
        format!(
            "0x{}::{}::{}",
            id.address().short_str_lossless(),
            id.name().as_str(),
            self.name.as_str()
        )
    }

    pub(crate) fn is_native(&self) -> bool {
        self.def_is_native
    }

    pub(crate) fn get_native(&self) -> PartialVMResult<&UnboxedNativeFunction> {
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

//
// Internal structures that are saved at the proper index in the proper tables to access
// execution information (interpreter).
// The following structs are internal to the loader and never exposed out.
// The `Loader` will create those struct and the proper table when loading a module.
// The `Resolver` uses those structs to return information to the `Interpreter`.
//

// A function instantiation.
#[derive(Debug)]
pub(crate) struct FunctionInstantiation {
    // index to `ModuleCache::functions` global table
    pub(crate) handle: usize,
    pub(crate) instantiation_idx: SignatureIndex,
}

#[derive(Debug)]
pub(crate) struct StructDef {
    // struct field count
    pub(crate) field_count: u16,
    // `ModuelCache::structs` global table index
    pub(crate) idx: CachedTypeIndex,
}

#[derive(Debug)]
pub(crate) struct StructInstantiation {
    // struct field count
    pub(crate) field_count: u16,
    // `ModuleCache::structs` global table index. It is the generic type.
    pub(crate) def: CachedTypeIndex,
    pub(crate) instantiation_idx: SignatureIndex,
}

// A field handle. The offset is the only used information when operating on a field
#[derive(Debug)]
pub(crate) struct FieldHandle {
    pub(crate) offset: usize,
    // `ModuelCache::structs` global table index. It is the generic type.
    pub(crate) owner: CachedTypeIndex,
}

// A field instantiation. The offset is the only used information when operating on a field
#[derive(Debug)]
pub(crate) struct FieldInstantiation {
    pub(crate) offset: usize,
    // `ModuleCache::structs` global table index. It is the generic type.
    #[allow(unused)]
    pub(crate) owner: CachedTypeIndex,
}

#[derive(Debug)]
pub(crate) struct EnumDef {
    // enum variant count
    #[allow(unused)]
    pub(crate) variant_count: u16,
    pub(crate) variants: Vec<VariantDef>,
    // `ModuelCache::types` global table index
    pub(crate) idx: CachedTypeIndex,
}

#[derive(Debug)]
pub(crate) struct EnumInstantiation {
    // enum variant count
    pub(crate) variant_count_map: Vec<u16>,
    // `ModuelCache::types` global table index
    pub(crate) def: CachedTypeIndex,
    pub(crate) instantiation_idx: SignatureIndex,
}

#[derive(Debug)]
pub(crate) struct VariantDef {
    #[allow(unused)]
    pub(crate) tag: u16,
    pub(crate) field_count: u16,
    #[allow(unused)]
    pub(crate) field_types: Vec<Type>,
}

//
// Cache for data associated to a Struct, used for de/serialization and more
//

pub(crate) struct DatatypeInfo {
    pub(crate) runtime_tag: Option<StructTag>,
    pub(crate) defining_tag: Option<StructTag>,
    pub(crate) layout: Option<R::MoveDatatypeLayout>,
    pub(crate) annotated_layout: Option<A::MoveDatatypeLayout>,
    pub(crate) node_count: Option<u64>,
    pub(crate) annotated_node_count: Option<u64>,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum DatatypeTagType {
    Runtime,
    Defining,
}

impl DatatypeInfo {
    pub fn new() -> Self {
        Self {
            runtime_tag: None,
            defining_tag: None,
            layout: None,
            annotated_layout: None,
            node_count: None,
            annotated_node_count: None,
        }
    }
}
