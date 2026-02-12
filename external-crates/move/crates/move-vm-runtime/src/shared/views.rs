// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use move_core_types::{account_address::AccountAddress, gas_algebra::AbstractMemorySize};

use crate::shared::SafeArithmetic as _;

// -------------------------------------------------------------------------------------------------
// Abstract Memory Size
// -------------------------------------------------------------------------------------------------

pub struct SizeConfig {
    /// If true, the reference values will be traversed recursively.
    pub traverse_references: bool,
    /// If true, the size of the vector will be included in the abstract memory size.
    pub include_vector_size: bool,
}

/// Trait that provides an abstract view into a Move Value.
///
/// This is used to expose certain info to clients (e.g. the gas meter),
/// usually in a lazily evaluated fashion.
pub trait ValueView {
    fn visit(&self, visitor: &mut impl ValueVisitor) -> PartialVMResult<()>;

    /// Returns the abstract memory size of the value.
    ///
    /// SAFETY: This uses the addition over `AbstractMemorySize` which is implemented as a
    /// saturating addition. This means that if the value is too large, it will not overflow.
    #[allow(clippy::arithmetic_side_effects)]
    fn abstract_memory_size(&self, config: &SizeConfig) -> PartialVMResult<AbstractMemorySize> {
        /// The size for primitives smaller than u128
        const PRIMITIVE_SIZE: AbstractMemorySize = AbstractMemorySize::new(8);
        /// The size for u128
        const U128_SIZE: AbstractMemorySize = AbstractMemorySize::new(16);
        /// The size for u256
        const U256_SIZE: AbstractMemorySize = AbstractMemorySize::new(32);

        /// The size of a struct
        const STRUCT_SIZE: AbstractMemorySize = AbstractMemorySize::new(2);

        /// The size in bytes for a reference on the stack
        const REFERENCE_SIZE: AbstractMemorySize = AbstractMemorySize::new(8);

        struct Acc<'b> {
            accumulated_size: AbstractMemorySize,
            config: &'b SizeConfig,
        }

        impl ValueVisitor for Acc<'_> {
            fn visit_u8(&mut self, _depth: usize, _val: u8) -> PartialVMResult<()> {
                self.accumulated_size += PRIMITIVE_SIZE;
                Ok(())
            }

            fn visit_u16(&mut self, _depth: usize, _val: u16) -> PartialVMResult<()> {
                self.accumulated_size += PRIMITIVE_SIZE;
                Ok(())
            }

            fn visit_u32(&mut self, _depth: usize, _val: u32) -> PartialVMResult<()> {
                self.accumulated_size += PRIMITIVE_SIZE;
                Ok(())
            }

            fn visit_u64(&mut self, _depth: usize, _val: u64) -> PartialVMResult<()> {
                self.accumulated_size += PRIMITIVE_SIZE;
                Ok(())
            }

            fn visit_u128(&mut self, _depth: usize, _val: u128) -> PartialVMResult<()> {
                self.accumulated_size += U128_SIZE;
                Ok(())
            }

            fn visit_u256(
                &mut self,
                _depth: usize,
                _val: move_core_types::u256::U256,
            ) -> PartialVMResult<()> {
                self.accumulated_size += U256_SIZE;
                Ok(())
            }

            fn visit_bool(&mut self, _depth: usize, _val: bool) -> PartialVMResult<()> {
                self.accumulated_size += PRIMITIVE_SIZE;
                Ok(())
            }

            fn visit_address(
                &mut self,
                _depth: usize,
                _val: AccountAddress,
            ) -> PartialVMResult<()> {
                self.accumulated_size += AbstractMemorySize::new(AccountAddress::LENGTH as u64);
                Ok(())
            }

            fn visit_struct(&mut self, _depth: usize, _len: usize) -> PartialVMResult<bool> {
                self.accumulated_size += STRUCT_SIZE;
                Ok(true)
            }

            fn visit_variant(&mut self, _depth: usize, _len: usize) -> PartialVMResult<bool> {
                self.accumulated_size += STRUCT_SIZE;
                Ok(true)
            }

            fn visit_vec(&mut self, _depth: usize, _len: usize) -> PartialVMResult<bool> {
                self.accumulated_size += STRUCT_SIZE;
                Ok(true)
            }

            fn visit_vec_u8(&mut self, _depth: usize, vals: &[u8]) -> PartialVMResult<()> {
                if self.config.include_vector_size {
                    self.accumulated_size += STRUCT_SIZE;
                }
                self.accumulated_size += (std::mem::size_of_val(vals) as u64).into();
                Ok(())
            }

            fn visit_vec_u16(&mut self, _depth: usize, vals: &[u16]) -> PartialVMResult<()> {
                if self.config.include_vector_size {
                    self.accumulated_size += STRUCT_SIZE;
                }
                self.accumulated_size += (std::mem::size_of_val(vals) as u64).into();
                Ok(())
            }

            fn visit_vec_u32(&mut self, _depth: usize, vals: &[u32]) -> PartialVMResult<()> {
                if self.config.include_vector_size {
                    self.accumulated_size += STRUCT_SIZE;
                }
                self.accumulated_size += (std::mem::size_of_val(vals) as u64).into();
                Ok(())
            }

            fn visit_vec_u64(&mut self, _depth: usize, vals: &[u64]) -> PartialVMResult<()> {
                if self.config.include_vector_size {
                    self.accumulated_size += STRUCT_SIZE;
                }
                self.accumulated_size += (std::mem::size_of_val(vals) as u64).into();
                Ok(())
            }

            fn visit_vec_u128(&mut self, _depth: usize, vals: &[u128]) -> PartialVMResult<()> {
                if self.config.include_vector_size {
                    self.accumulated_size += STRUCT_SIZE;
                }
                self.accumulated_size += (std::mem::size_of_val(vals) as u64).into();
                Ok(())
            }

            fn visit_vec_u256(
                &mut self,
                _depth: usize,
                vals: &[move_core_types::u256::U256],
            ) -> PartialVMResult<()> {
                if self.config.include_vector_size {
                    self.accumulated_size += STRUCT_SIZE;
                }
                self.accumulated_size += (std::mem::size_of_val(vals) as u64).into();
                Ok(())
            }

            fn visit_vec_bool(&mut self, _depth: usize, vals: &[bool]) -> PartialVMResult<()> {
                if self.config.include_vector_size {
                    self.accumulated_size += STRUCT_SIZE;
                }
                self.accumulated_size += (std::mem::size_of_val(vals) as u64).into();
                Ok(())
            }

            fn visit_vec_address(
                &mut self,
                _depth: usize,
                vals: &[AccountAddress],
            ) -> PartialVMResult<()> {
                if self.config.include_vector_size {
                    self.accumulated_size += STRUCT_SIZE;
                }
                self.accumulated_size += (std::mem::size_of_val(vals) as u64).into();
                Ok(())
            }

            fn visit_ref(&mut self, _depth: usize) -> PartialVMResult<bool> {
                self.accumulated_size += REFERENCE_SIZE;
                Ok(self.config.traverse_references)
            }
        }

        let mut acc = Acc {
            accumulated_size: 0.into(),
            config,
        };
        self.visit(&mut acc)?;

        Ok(acc.accumulated_size)
    }
}

/// Trait that defines a visitor that could be used to traverse a value recursively.
pub trait ValueVisitor {
    fn visit_u8(&mut self, depth: usize, val: u8) -> PartialVMResult<()>;
    fn visit_u16(&mut self, depth: usize, val: u16) -> PartialVMResult<()>;
    fn visit_u32(&mut self, depth: usize, val: u32) -> PartialVMResult<()>;
    fn visit_u64(&mut self, depth: usize, val: u64) -> PartialVMResult<()>;
    fn visit_u128(&mut self, depth: usize, val: u128) -> PartialVMResult<()>;
    fn visit_u256(&mut self, depth: usize, val: move_core_types::u256::U256)
    -> PartialVMResult<()>;
    fn visit_bool(&mut self, depth: usize, val: bool) -> PartialVMResult<()>;
    fn visit_address(&mut self, depth: usize, val: AccountAddress) -> PartialVMResult<()>;

    fn visit_struct(&mut self, depth: usize, len: usize) -> PartialVMResult<bool>;
    fn visit_variant(&mut self, depth: usize, len: usize) -> PartialVMResult<bool>;
    fn visit_vec(&mut self, depth: usize, len: usize) -> PartialVMResult<bool>;

    fn visit_ref(&mut self, depth: usize) -> PartialVMResult<bool>;

    fn visit_vec_u8(&mut self, depth: usize, vals: &[u8]) -> PartialVMResult<()> {
        self.visit_vec(depth, vals.len())?;
        for val in vals {
            self.visit_u8(depth.safe_add(1)?, *val)?;
        }
        Ok(())
    }

    fn visit_vec_u16(&mut self, depth: usize, vals: &[u16]) -> PartialVMResult<()> {
        self.visit_vec(depth, vals.len())?;
        for val in vals {
            self.visit_u16(depth.safe_add(1)?, *val)?;
        }
        Ok(())
    }

    fn visit_vec_u32(&mut self, depth: usize, vals: &[u32]) -> PartialVMResult<()> {
        self.visit_vec(depth, vals.len())?;
        for val in vals {
            self.visit_u32(depth.safe_add(1)?, *val)?;
        }
        Ok(())
    }

    fn visit_vec_u64(&mut self, depth: usize, vals: &[u64]) -> PartialVMResult<()> {
        self.visit_vec(depth, vals.len())?;
        for val in vals {
            self.visit_u64(depth.safe_add(1)?, *val)?;
        }
        Ok(())
    }

    fn visit_vec_u128(&mut self, depth: usize, vals: &[u128]) -> PartialVMResult<()> {
        self.visit_vec(depth, vals.len())?;
        for val in vals {
            self.visit_u128(depth.safe_add(1)?, *val)?;
        }
        Ok(())
    }

    fn visit_vec_u256(
        &mut self,
        depth: usize,
        vals: &[move_core_types::u256::U256],
    ) -> PartialVMResult<()> {
        self.visit_vec(depth, vals.len())?;
        for val in vals {
            self.visit_u256(depth.safe_add(1)?, *val)?;
        }
        Ok(())
    }

    fn visit_vec_bool(&mut self, depth: usize, vals: &[bool]) -> PartialVMResult<()> {
        self.visit_vec(depth, vals.len())?;
        for val in vals {
            self.visit_bool(depth.safe_add(1)?, *val)?;
        }
        Ok(())
    }

    fn visit_vec_address(&mut self, depth: usize, vals: &[AccountAddress]) -> PartialVMResult<()> {
        self.visit_vec(depth, vals.len())?;
        for val in vals {
            self.visit_address(depth.safe_add(1)?, *val)?;
        }
        Ok(())
    }
}

impl<T> ValueView for &T
where
    T: ValueView,
{
    fn visit(&self, visitor: &mut impl ValueVisitor) -> PartialVMResult<()> {
        <T as ValueView>::visit(*self, visitor)
    }
}
