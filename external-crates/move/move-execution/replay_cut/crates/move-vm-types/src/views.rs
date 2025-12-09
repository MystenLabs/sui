// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    account_address::AccountAddress, gas_algebra::AbstractMemorySize, language_storage::TypeTag,
};

pub struct SizeConfig {
    /// If true, the reference values will be traversed recursively.
    pub traverse_references: bool,
    /// If true, the size of the vector will be included in the abstract memory size.
    pub include_vector_size: bool,
    /// If true, use a more fine-grained size calculation for primitive values.
    pub fine_grained_value_size: bool,
}

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
    fn abstract_memory_size(&self, config: &SizeConfig) -> AbstractMemorySize {
        use crate::values::{LEGACY_CONST_SIZE, LEGACY_REFERENCE_SIZE, LEGACY_STRUCT_SIZE};
        /// The size for primitives smaller than u128
        const PRIMITIVE_SIZE: AbstractMemorySize = AbstractMemorySize::new(8);
        /// The size for u128
        const U128_SIZE: AbstractMemorySize = AbstractMemorySize::new(16);
        /// The size for u256
        const U256_SIZE: AbstractMemorySize = AbstractMemorySize::new(32);

        struct Acc<'b> {
            accumulated_size: AbstractMemorySize,
            config: &'b SizeConfig,
        }

        impl ValueVisitor for Acc<'_> {
            fn visit_u8(&mut self, _depth: usize, _val: u8) {
                self.accumulated_size += if self.config.fine_grained_value_size {
                    PRIMITIVE_SIZE
                } else {
                    LEGACY_CONST_SIZE
                }
            }

            fn visit_u16(&mut self, _depth: usize, _val: u16) {
                self.accumulated_size += if self.config.fine_grained_value_size {
                    PRIMITIVE_SIZE
                } else {
                    LEGACY_CONST_SIZE
                }
            }

            fn visit_u32(&mut self, _depth: usize, _val: u32) {
                self.accumulated_size += if self.config.fine_grained_value_size {
                    PRIMITIVE_SIZE
                } else {
                    LEGACY_CONST_SIZE
                }
            }

            fn visit_u64(&mut self, _depth: usize, _val: u64) {
                self.accumulated_size += if self.config.fine_grained_value_size {
                    PRIMITIVE_SIZE
                } else {
                    LEGACY_CONST_SIZE
                }
            }

            fn visit_u128(&mut self, _depth: usize, _val: u128) {
                self.accumulated_size += if self.config.fine_grained_value_size {
                    U128_SIZE
                } else {
                    LEGACY_CONST_SIZE
                }
            }

            fn visit_u256(&mut self, _depth: usize, _val: move_core_types::u256::U256) {
                self.accumulated_size += if self.config.fine_grained_value_size {
                    U256_SIZE
                } else {
                    LEGACY_CONST_SIZE
                }
            }

            fn visit_bool(&mut self, _depth: usize, _val: bool) {
                self.accumulated_size += if self.config.fine_grained_value_size {
                    PRIMITIVE_SIZE
                } else {
                    LEGACY_CONST_SIZE
                }
            }

            fn visit_address(&mut self, _depth: usize, _val: AccountAddress) {
                self.accumulated_size += AbstractMemorySize::new(AccountAddress::LENGTH as u64);
            }

            fn visit_struct(&mut self, _depth: usize, _len: usize) -> bool {
                self.accumulated_size += LEGACY_STRUCT_SIZE;
                true
            }

            fn visit_variant(&mut self, _depth: usize, _len: usize) -> bool {
                self.accumulated_size += LEGACY_STRUCT_SIZE;
                true
            }

            fn visit_vec(&mut self, _depth: usize, _len: usize) -> bool {
                self.accumulated_size += LEGACY_STRUCT_SIZE;
                true
            }

            fn visit_vec_u8(&mut self, _depth: usize, vals: &[u8]) {
                if self.config.include_vector_size {
                    self.accumulated_size += LEGACY_STRUCT_SIZE;
                }
                self.accumulated_size += (std::mem::size_of_val(vals) as u64).into();
            }

            fn visit_vec_u16(&mut self, _depth: usize, vals: &[u16]) {
                if self.config.include_vector_size {
                    self.accumulated_size += LEGACY_STRUCT_SIZE;
                }
                self.accumulated_size += (std::mem::size_of_val(vals) as u64).into();
            }

            fn visit_vec_u32(&mut self, _depth: usize, vals: &[u32]) {
                if self.config.include_vector_size {
                    self.accumulated_size += LEGACY_STRUCT_SIZE;
                }
                self.accumulated_size += (std::mem::size_of_val(vals) as u64).into();
            }

            fn visit_vec_u64(&mut self, _depth: usize, vals: &[u64]) {
                if self.config.include_vector_size {
                    self.accumulated_size += LEGACY_STRUCT_SIZE;
                }
                self.accumulated_size += (std::mem::size_of_val(vals) as u64).into();
            }

            fn visit_vec_u128(&mut self, _depth: usize, vals: &[u128]) {
                if self.config.include_vector_size {
                    self.accumulated_size += LEGACY_STRUCT_SIZE;
                }
                self.accumulated_size += (std::mem::size_of_val(vals) as u64).into();
            }

            fn visit_vec_u256(&mut self, _depth: usize, vals: &[move_core_types::u256::U256]) {
                if self.config.include_vector_size {
                    self.accumulated_size += LEGACY_STRUCT_SIZE;
                }
                self.accumulated_size += (std::mem::size_of_val(vals) as u64).into();
            }

            fn visit_vec_bool(&mut self, _depth: usize, vals: &[bool]) {
                if self.config.include_vector_size {
                    self.accumulated_size += LEGACY_STRUCT_SIZE;
                }
                self.accumulated_size += (std::mem::size_of_val(vals) as u64).into();
            }

            fn visit_vec_address(&mut self, _depth: usize, vals: &[AccountAddress]) {
                if self.config.include_vector_size {
                    self.accumulated_size += LEGACY_STRUCT_SIZE;
                }
                self.accumulated_size += (std::mem::size_of_val(vals) as u64).into();
            }

            fn visit_ref(&mut self, _depth: usize, _is_global: bool) -> bool {
                self.accumulated_size += LEGACY_REFERENCE_SIZE;
                self.config.traverse_references
            }
        }

        let mut acc = Acc {
            accumulated_size: 0.into(),
            config,
        };
        self.visit(&mut acc);

        acc.accumulated_size
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
