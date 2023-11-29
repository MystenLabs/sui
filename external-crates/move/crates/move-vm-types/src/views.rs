// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    account_address::AccountAddress, gas_algebra::AbstractMemorySize, language_storage::TypeTag,
};
use std::mem::size_of;

/// Trait that provides an abstract view into a Move type.
///
/// This is used to expose certain info to clients (e.g. the gas meter),
/// usually in a lazily evaluated fashion.
pub trait TypeView {
    /// Returns the `TypeTag` (fully qualified name) of the type.
    fn to_type_tag(&self) -> TypeTag;
}

/// Trait that provides an abstract view into a Move Value.
///
/// This is used to expose certain info to clients (e.g. the gas meter),
/// usually in a lazily evaluated fashion.
pub trait ValueView {
    fn visit(&self, visitor: &mut impl ValueVisitor);

    /// Returns the abstract memory size of the value.
    ///
    /// This version of abstract memory size is not well-defined and is only kept for backward
    /// compatibility.  New applications should avoid using this.
    fn legacy_abstract_memory_size(&self) -> AbstractMemorySize {
        use crate::values::{LEGACY_CONST_SIZE, LEGACY_REFERENCE_SIZE, LEGACY_STRUCT_SIZE};

        struct Acc(AbstractMemorySize);

        impl ValueVisitor for Acc {
            fn visit_u8(&mut self, _depth: usize, _val: u8) {
                self.0 += LEGACY_CONST_SIZE;
            }

            fn visit_u16(&mut self, _depth: usize, _val: u16) {
                self.0 += LEGACY_CONST_SIZE;
            }

            fn visit_u32(&mut self, _depth: usize, _val: u32) {
                self.0 += LEGACY_CONST_SIZE;
            }

            fn visit_u64(&mut self, _depth: usize, _val: u64) {
                self.0 += LEGACY_CONST_SIZE;
            }

            fn visit_u128(&mut self, _depth: usize, _val: u128) {
                self.0 += LEGACY_CONST_SIZE;
            }

            fn visit_u256(&mut self, _depth: usize, _val: move_core_types::u256::U256) {
                self.0 += LEGACY_CONST_SIZE;
            }

            fn visit_bool(&mut self, _depth: usize, _val: bool) {
                self.0 += LEGACY_CONST_SIZE;
            }

            fn visit_address(&mut self, _depth: usize, _val: AccountAddress) {
                self.0 += AbstractMemorySize::new(AccountAddress::LENGTH as u64);
            }

            fn visit_struct(&mut self, _depth: usize, _len: usize) -> bool {
                self.0 += LEGACY_STRUCT_SIZE;
                true
            }

            fn visit_variant(&mut self, _depth: usize, _len: usize) -> bool {
                self.0 += LEGACY_STRUCT_SIZE;
                true
            }

            fn visit_vec(&mut self, _depth: usize, _len: usize) -> bool {
                self.0 += LEGACY_STRUCT_SIZE;
                true
            }

            fn visit_vec_u8(&mut self, _depth: usize, vals: &[u8]) {
                self.0 += ((size_of::<u8>() * vals.len()) as u64).into();
            }

            fn visit_vec_u16(&mut self, _depth: usize, vals: &[u16]) {
                self.0 += ((size_of::<u16>() * vals.len()) as u64).into();
            }

            fn visit_vec_u32(&mut self, _depth: usize, vals: &[u32]) {
                self.0 += ((size_of::<u32>() * vals.len()) as u64).into();
            }

            fn visit_vec_u64(&mut self, _depth: usize, vals: &[u64]) {
                self.0 += ((size_of::<u64>() * vals.len()) as u64).into();
            }

            fn visit_vec_u128(&mut self, _depth: usize, vals: &[u128]) {
                self.0 += ((size_of::<u128>() * vals.len()) as u64).into();
            }

            fn visit_vec_u256(&mut self, _depth: usize, vals: &[move_core_types::u256::U256]) {
                self.0 += ((size_of::<move_core_types::u256::U256>() * vals.len()) as u64).into();
            }

            fn visit_vec_bool(&mut self, _depth: usize, vals: &[bool]) {
                self.0 += ((size_of::<bool>() * vals.len()) as u64).into();
            }

            fn visit_vec_address(&mut self, _depth: usize, vals: &[AccountAddress]) {
                self.0 += ((size_of::<AccountAddress>() * vals.len()) as u64).into();
            }

            fn visit_ref(&mut self, _depth: usize, _is_global: bool) -> bool {
                self.0 += LEGACY_REFERENCE_SIZE;
                false
            }
        }

        let mut acc = Acc(0.into());
        self.visit(&mut acc);

        acc.0
    }

    /// Returns the abstract memory size of the value.
    fn abstract_memory_size(&self) -> AbstractMemorySize {
        use crate::values::{LEGACY_CONST_SIZE, LEGACY_REFERENCE_SIZE, LEGACY_STRUCT_SIZE};

        struct Acc(AbstractMemorySize);

        impl ValueVisitor for Acc {
            fn visit_u8(&mut self, _depth: usize, _val: u8) {
                self.0 += LEGACY_CONST_SIZE;
            }

            fn visit_u16(&mut self, _depth: usize, _val: u16) {
                self.0 += LEGACY_CONST_SIZE;
            }

            fn visit_u32(&mut self, _depth: usize, _val: u32) {
                self.0 += LEGACY_CONST_SIZE;
            }

            fn visit_u64(&mut self, _depth: usize, _val: u64) {
                self.0 += LEGACY_CONST_SIZE;
            }

            fn visit_u128(&mut self, _depth: usize, _val: u128) {
                self.0 += LEGACY_CONST_SIZE;
            }

            fn visit_u256(&mut self, _depth: usize, _val: move_core_types::u256::U256) {
                self.0 += LEGACY_CONST_SIZE;
            }

            fn visit_bool(&mut self, _depth: usize, _val: bool) {
                self.0 += LEGACY_CONST_SIZE;
            }

            fn visit_address(&mut self, _depth: usize, _val: AccountAddress) {
                self.0 += AbstractMemorySize::new(AccountAddress::LENGTH as u64);
            }

            fn visit_struct(&mut self, _depth: usize, _len: usize) -> bool {
                self.0 += LEGACY_STRUCT_SIZE;
                true
            }

            fn visit_variant(&mut self, _depth: usize, _len: usize) -> bool {
                self.0 += LEGACY_STRUCT_SIZE;
                true
            }

            fn visit_vec(&mut self, _depth: usize, _len: usize) -> bool {
                self.0 += LEGACY_STRUCT_SIZE;
                true
            }

            fn visit_vec_u8(&mut self, _depth: usize, vals: &[u8]) {
                self.0 += LEGACY_STRUCT_SIZE;
                self.0 += ((size_of::<u8>() * vals.len()) as u64).into();
            }

            fn visit_vec_u16(&mut self, _depth: usize, vals: &[u16]) {
                self.0 += LEGACY_STRUCT_SIZE;
                self.0 += ((size_of::<u16>() * vals.len()) as u64).into();
            }

            fn visit_vec_u32(&mut self, _depth: usize, vals: &[u32]) {
                self.0 += LEGACY_STRUCT_SIZE;
                self.0 += ((size_of::<u32>() * vals.len()) as u64).into();
            }

            fn visit_vec_u64(&mut self, _depth: usize, vals: &[u64]) {
                self.0 += LEGACY_STRUCT_SIZE;
                self.0 += ((size_of::<u64>() * vals.len()) as u64).into();
            }

            fn visit_vec_u128(&mut self, _depth: usize, vals: &[u128]) {
                self.0 += LEGACY_STRUCT_SIZE;
                self.0 += ((size_of::<u128>() * vals.len()) as u64).into();
            }

            fn visit_vec_u256(&mut self, _depth: usize, vals: &[move_core_types::u256::U256]) {
                self.0 += LEGACY_STRUCT_SIZE;
                self.0 += ((size_of::<move_core_types::u256::U256>() * vals.len()) as u64).into();
            }

            fn visit_vec_bool(&mut self, _depth: usize, vals: &[bool]) {
                self.0 += LEGACY_STRUCT_SIZE;
                self.0 += ((size_of::<bool>() * vals.len()) as u64).into();
            }

            fn visit_vec_address(&mut self, _depth: usize, vals: &[AccountAddress]) {
                self.0 += LEGACY_STRUCT_SIZE;
                self.0 += ((size_of::<AccountAddress>() * vals.len()) as u64).into();
            }

            fn visit_ref(&mut self, _depth: usize, _is_global: bool) -> bool {
                self.0 += LEGACY_REFERENCE_SIZE;
                false
            }
        }

        let mut acc = Acc(0.into());
        self.visit(&mut acc);

        acc.0
    }
}

/// Trait that defines a visitor that could be used to traverse a value recursively.
pub trait ValueVisitor {
    fn visit_u8(&mut self, depth: usize, val: u8);
    fn visit_u16(&mut self, depth: usize, val: u16);
    fn visit_u32(&mut self, depth: usize, val: u32);
    fn visit_u64(&mut self, depth: usize, val: u64);
    fn visit_u128(&mut self, depth: usize, val: u128);
    fn visit_u256(&mut self, depth: usize, val: move_core_types::u256::U256);
    fn visit_bool(&mut self, depth: usize, val: bool);
    fn visit_address(&mut self, depth: usize, val: AccountAddress);

    fn visit_struct(&mut self, depth: usize, len: usize) -> bool;
    fn visit_variant(&mut self, depth: usize, len: usize) -> bool;
    fn visit_vec(&mut self, depth: usize, len: usize) -> bool;

    fn visit_ref(&mut self, depth: usize, is_global: bool) -> bool;

    fn visit_vec_u8(&mut self, depth: usize, vals: &[u8]) {
        self.visit_vec(depth, vals.len());
        for val in vals {
            self.visit_u8(depth + 1, *val);
        }
    }

    fn visit_vec_u16(&mut self, depth: usize, vals: &[u16]) {
        self.visit_vec(depth, vals.len());
        for val in vals {
            self.visit_u16(depth + 1, *val);
        }
    }

    fn visit_vec_u32(&mut self, depth: usize, vals: &[u32]) {
        self.visit_vec(depth, vals.len());
        for val in vals {
            self.visit_u32(depth + 1, *val);
        }
    }

    fn visit_vec_u64(&mut self, depth: usize, vals: &[u64]) {
        self.visit_vec(depth, vals.len());
        for val in vals {
            self.visit_u64(depth + 1, *val);
        }
    }

    fn visit_vec_u128(&mut self, depth: usize, vals: &[u128]) {
        self.visit_vec(depth, vals.len());
        for val in vals {
            self.visit_u128(depth + 1, *val);
        }
    }

    fn visit_vec_u256(&mut self, depth: usize, vals: &[move_core_types::u256::U256]) {
        self.visit_vec(depth, vals.len());
        for val in vals {
            self.visit_u256(depth + 1, *val);
        }
    }

    fn visit_vec_bool(&mut self, depth: usize, vals: &[bool]) {
        self.visit_vec(depth, vals.len());
        for val in vals {
            self.visit_bool(depth + 1, *val);
        }
    }

    fn visit_vec_address(&mut self, depth: usize, vals: &[AccountAddress]) {
        self.visit_vec(depth, vals.len());
        for val in vals {
            self.visit_address(depth + 1, *val);
        }
    }
}

impl<T> ValueView for &T
where
    T: ValueView,
{
    fn legacy_abstract_memory_size(&self) -> AbstractMemorySize {
        <T as ValueView>::legacy_abstract_memory_size(*self)
    }

    fn visit(&self, visitor: &mut impl ValueVisitor) {
        <T as ValueView>::visit(*self, visitor)
    }
}

impl<T> TypeView for &T
where
    T: TypeView,
{
    fn to_type_tag(&self) -> TypeTag {
        <T as TypeView>::to_type_tag(*self)
    }
}
