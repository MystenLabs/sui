// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::jit::execution::ast::{AsInternalType, Type as InternalType, TypeSubst};

use move_binary_format::errors::PartialVMResult;
use move_core_types::gas_algebra::AbstractMemorySize;

// Types that can be passed to and from the VM. These are wrappers around an internal type
// representation, and are intentionally opaque to users of the VM. This is due to the nature of
// the internal type representation, and to allow us to ensure we can revise it in the future.
/// Type the VM can produce and consume as an argument. Intentionally opaque.
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct Type(InternalType);

macro_rules! make_prim(
    ($name:ident,$ctor:ident) => {
        impl Type {
            /// Creates a $name type.
            pub const fn $name() -> Self {
                Type(InternalType::$ctor)
            }
        }
    };
);

impl Type {
    /// Returns the abstract memory size the data structure occupies.
    ///
    /// This kept only for legacy reasons.
    /// New applications should not use this.
    pub fn size(&self) -> AbstractMemorySize {
        self.0.size()
    }

    /// Substitutes the type parameters in this type with the given type arguments.
    pub fn subst(&self, ty_args: &[Type]) -> PartialVMResult<Type> {
        self.0.subst(ty_args).map(Type)
    }

    /// Creates a vector of the provided type.
    pub fn vector(ty: Type) -> Self {
        Type(InternalType::Vector(Box::new(ty.into_inner())))
    }

    /// Creates a `vector<u8>` type.
    pub fn vector_u8() -> Self {
        Type(InternalType::Vector(Box::new(InternalType::U8)))
    }

    /// [SAFETY] THIS MUST NEVER BE EXPOSED TO USERS OF THE VM. This is only for internal use
    /// within the VM to convert external types to internal types for introspection.
    pub(crate) fn into_inner(self) -> InternalType {
        self.0
    }
}

make_prim!(bool, Bool);
make_prim!(u8, U8);
make_prim!(u16, U16);
make_prim!(u32, U32);
make_prim!(u64, U64);
make_prim!(u128, U128);
make_prim!(u256, U256);
make_prim!(i8, I8);
make_prim!(i16, I16);
make_prim!(i32, I32);
make_prim!(i64, I64);
make_prim!(i128, I128);
make_prim!(i256, I256);

impl AsInternalType for Type {
    fn as_internal_type(&self) -> &InternalType {
        &self.0
    }
}

impl From<InternalType> for Type {
    fn from(t: InternalType) -> Self {
        Type(t)
    }
}
