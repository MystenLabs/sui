// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::arena::{ArenaBox, ArenaBuilder},
    execution::dispatch_tables::VirtualTableKey,
    jit::execution::ast::ArenaType,
    shared::vm_pointer::VMPointer,
};
use indexmap::IndexMap;
use move_binary_format::{errors::PartialVMResult, partial_vm_error};

/// Stable index into [`ArenaTypeInterner::types`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct TypeId(usize);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PrimitiveType {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
    Signer,
}

#[derive(Debug, Eq, Hash, PartialEq)]
enum TypeNodeKey {
    Primitive(PrimitiveType),
    TypeParameter(u16),
    Vector(TypeId),
    Reference(TypeId),
    MutableReference(TypeId),
    Datatype(VirtualTableKey),
    DatatypeInstantiation {
        datatype: VirtualTableKey,
        type_arguments: Box<[TypeId]>,
    },
}

/// Canonical arena type paired with its stable index in the interner.
pub(crate) struct InternedType {
    id: TypeId,
    ptr: VMPointer<ArenaType>,
}

impl InternedType {
    pub(crate) fn ptr(&self) -> VMPointer<ArenaType> {
        self.ptr.ptr_clone()
    }
}

#[derive(Default)]
pub(crate) struct ArenaTypeInterner {
    types: IndexMap<TypeNodeKey, ArenaBox<ArenaType>>,
}

impl ArenaTypeInterner {
    pub(crate) fn intern_primitive(
        &mut self,
        arena: &ArenaBuilder,
        primitive: PrimitiveType,
    ) -> PartialVMResult<InternedType> {
        let ty = match primitive {
            PrimitiveType::Bool => ArenaType::Bool,
            PrimitiveType::U8 => ArenaType::U8,
            PrimitiveType::U16 => ArenaType::U16,
            PrimitiveType::U32 => ArenaType::U32,
            PrimitiveType::U64 => ArenaType::U64,
            PrimitiveType::U128 => ArenaType::U128,
            PrimitiveType::U256 => ArenaType::U256,
            PrimitiveType::Address => ArenaType::Address,
            PrimitiveType::Signer => ArenaType::Signer,
        };
        self.intern(arena, TypeNodeKey::Primitive(primitive), ty)
    }

    pub(crate) fn intern_type_parameter(
        &mut self,
        arena: &ArenaBuilder,
        index: u16,
    ) -> PartialVMResult<InternedType> {
        self.intern(
            arena,
            TypeNodeKey::TypeParameter(index),
            ArenaType::TyParam(index),
        )
    }

    pub(crate) fn intern_vector(
        &mut self,
        arena: &ArenaBuilder,
        element: InternedType,
    ) -> PartialVMResult<InternedType> {
        self.intern(
            arena,
            TypeNodeKey::Vector(element.id),
            ArenaType::Vector(element.ptr),
        )
    }

    pub(crate) fn intern_reference(
        &mut self,
        arena: &ArenaBuilder,
        referenced: InternedType,
    ) -> PartialVMResult<InternedType> {
        self.intern(
            arena,
            TypeNodeKey::Reference(referenced.id),
            ArenaType::Reference(referenced.ptr),
        )
    }

    pub(crate) fn intern_mutable_reference(
        &mut self,
        arena: &ArenaBuilder,
        referenced: InternedType,
    ) -> PartialVMResult<InternedType> {
        self.intern(
            arena,
            TypeNodeKey::MutableReference(referenced.id),
            ArenaType::MutableReference(referenced.ptr),
        )
    }

    pub(crate) fn intern_datatype(
        &mut self,
        arena: &ArenaBuilder,
        datatype: VirtualTableKey,
    ) -> PartialVMResult<InternedType> {
        self.intern(
            arena,
            TypeNodeKey::Datatype(datatype.clone()),
            ArenaType::Datatype(datatype),
        )
    }

    pub(crate) fn intern_datatype_instantiation(
        &mut self,
        arena: &ArenaBuilder,
        datatype: VirtualTableKey,
        type_arguments: Vec<InternedType>,
    ) -> PartialVMResult<InternedType> {
        let key = TypeNodeKey::DatatypeInstantiation {
            datatype: datatype.clone(),
            type_arguments: type_arguments.iter().map(|argument| argument.id).collect(),
        };
        if let Some(interned) = self.get(&key) {
            return Ok(interned);
        }

        let type_arguments = arena.alloc_vec(type_arguments.iter().map(InternedType::ptr))?;
        let instantiation = arena.alloc_box((datatype, type_arguments))?;
        let ty = ArenaType::DatatypeInstantiation(VMPointer::from_ref(instantiation.inner_ref()));
        self.insert(arena, key, ty)
    }

    fn intern(
        &mut self,
        arena: &ArenaBuilder,
        key: TypeNodeKey,
        ty: ArenaType,
    ) -> PartialVMResult<InternedType> {
        if let Some(interned) = self.get(&key) {
            return Ok(interned);
        }
        self.insert(arena, key, ty)
    }

    fn insert(
        &mut self,
        arena: &ArenaBuilder,
        key: TypeNodeKey,
        ty: ArenaType,
    ) -> PartialVMResult<InternedType> {
        let id = TypeId(self.types.len());
        let ty = arena.alloc_box(ty)?;
        let ptr = VMPointer::from_ref(ty.inner_ref());
        let replaced = self.types.insert(key, ty);
        if replaced.is_some() {
            return Err(partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "type interner replaced an existing entry"
            ));
        }
        Ok(InternedType { id, ptr })
    }

    fn get(&self, key: &TypeNodeKey) -> Option<InternedType> {
        let (index, _, ty) = self.types.get_full(key)?;
        let id = TypeId(index);
        Some(InternedType {
            id,
            ptr: VMPointer::from_ref(ty.inner_ref()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use move_vm_config::runtime::VMConfig;

    #[test]
    fn repeated_types_share_nodes() -> PartialVMResult<()> {
        let arena = ArenaBuilder::new_bounded(&VMConfig::new_for_test(false, None));
        let mut interner = ArenaTypeInterner::default();

        let first_u64 = interner.intern_primitive(&arena, PrimitiveType::U64)?;
        let second_u64 = interner.intern_primitive(&arena, PrimitiveType::U64)?;
        assert!(first_u64.ptr.ptr_eq(&second_u64.ptr));

        let first_vector = interner.intern_vector(&arena, first_u64)?;
        let second_vector = interner.intern_vector(&arena, second_u64)?;
        assert!(first_vector.ptr.ptr_eq(&second_vector.ptr));
        assert_eq!(interner.types.len(), 2);
        Ok(())
    }
}
