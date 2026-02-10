// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::fmt;

pub mod binary_config;
pub mod check_bounds;
pub mod compatibility;
pub mod compatibility_mode;
#[macro_use]
pub mod errors;
pub mod constant;
pub mod deserializer;
pub mod file_format;
pub mod file_format_common;
pub mod internals;
pub mod normalized;
#[cfg(any(test, feature = "fuzzing"))]
pub mod proptest_types;
pub mod serializer;

pub mod inclusion_mode;
#[cfg(test)]
mod unit_tests;

pub use file_format::CompiledModule;

/// Represents a kind of index -- useful for error messages.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IndexKind {
    ModuleHandle,
    DatatypeHandle,
    FunctionHandle,
    FieldHandle,
    FriendDeclaration,
    FunctionInstantiation,
    FieldInstantiation,
    StructDefinition,
    StructDefInstantiation,
    FunctionDefinition,
    FieldDefinition,
    Signature,
    Identifier,
    AddressIdentifier,
    ConstantPool,
    LocalPool,
    CodeDefinition,
    TypeParameter,
    MemberCount,
    EnumDefinition,
    EnumDefInstantiation,
    VariantHandle,
    VariantInstantiationHandle,
    VariantJumpTable,
    VariantTag,
}

impl IndexKind {
    pub fn variants() -> &'static [IndexKind] {
        use IndexKind::*;

        // XXX ensure this list stays up to date!
        &[
            ModuleHandle,
            DatatypeHandle,
            FunctionHandle,
            FieldHandle,
            FriendDeclaration,
            StructDefInstantiation,
            FunctionInstantiation,
            FieldInstantiation,
            StructDefinition,
            FunctionDefinition,
            FieldDefinition,
            Signature,
            Identifier,
            ConstantPool,
            LocalPool,
            CodeDefinition,
            TypeParameter,
            MemberCount,
            EnumDefinition,
            EnumDefInstantiation,
            VariantHandle,
            VariantInstantiationHandle,
            VariantJumpTable,
            VariantTag,
        ]
    }
}

impl fmt::Display for IndexKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use IndexKind::*;

        let desc = match self {
            ModuleHandle => "module handle",
            DatatypeHandle => "datatype handle",
            FunctionHandle => "function handle",
            FieldHandle => "field handle",
            FriendDeclaration => "friend declaration",
            StructDefInstantiation => "struct instantiation",
            FunctionInstantiation => "function instantiation",
            FieldInstantiation => "field instantiation",
            StructDefinition => "struct definition",
            FunctionDefinition => "function definition",
            FieldDefinition => "field definition",
            Signature => "signature",
            Identifier => "identifier",
            AddressIdentifier => "address identifier",
            ConstantPool => "constant pool",
            LocalPool => "local pool",
            CodeDefinition => "code definition pool",
            TypeParameter => "type parameter",
            MemberCount => "field offset",
            EnumDefinition => "enum definition",
            EnumDefInstantiation => "enum instantiation",
            VariantHandle => "variant handle",
            VariantInstantiationHandle => "variant instantiation handle",
            VariantJumpTable => "jump table",
            VariantTag => "variant tag",
        };

        f.write_str(desc)
    }
}

/// A macro which should be preferred in critical runtime paths for unwrapping an option
/// if a `PartialVMError` is expected. In debug mode, this will panic. Otherwise
/// we return an Err.
#[macro_export]
macro_rules! safe_unwrap {
    ($e:expr) => {{
        match $e {
            Some(x) => x,
            None => {
                let err = move_binary_format::errors::PartialVMError::new(
                    move_core_types::vm_status::StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                )
                .with_message(format!("{}:{} (none)", file!(), line!()));
                if cfg!(debug_assertions) {
                    panic!("{:?}", err);
                } else {
                    return Err(err);
                }
            }
        }
    }};
}

/// Similar as above but for Result
#[macro_export]
macro_rules! safe_unwrap_err {
    ($e:expr) => {{
        match $e {
            Ok(x) => x,
            Err(e) => {
                let err = PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(format!("{}:{} {:#}", file!(), line!(), e));
                if cfg!(debug_assertions) {
                    panic!("{:?}", err);
                } else {
                    return Err(err);
                }
            }
        }
    }};
}

/// Similar as above, but asserts a boolean expression to be true.
#[macro_export]
macro_rules! safe_assert {
    ($e:expr) => {{
        if !$e {
            let err = PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message(format!("{}:{} (assert)", file!(), line!()));
            if cfg!(debug_assertions) {
                panic!("{:?}", err)
            } else {
                return Err(err);
            }
        }
    }};
}

/// Create a PartialVMError with the given error code and an optional message.
#[macro_export]
macro_rules! partial_vm_error {
    ($error_name:ident $(,)?) => {{
        $crate::errors::PartialVMError::new(
            move_core_types::vm_status::StatusCode::$error_name,
        )
    }};
    ($error_name:ident, $($body:tt)*) => {{
        $crate::errors::PartialVMError::new(
            move_core_types::vm_status::StatusCode::$error_name,
        ).with_message(
            format!($($body)*),
        )
    }};
}

/// A macro for performing a checked cast from one type to another, returning a
/// PartialVMError if the cast fails.
#[macro_export]
macro_rules! checked_as {
    ($value:expr, $target_type:ty) => {{
        let v = $value;
        <$target_type>::try_from(v).map_err(|e| {
            $crate::partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "Value {} cannot be safely cast to {}: {:?}",
                v,
                stringify!($target_type),
                e
            )
        })
    }};
}
