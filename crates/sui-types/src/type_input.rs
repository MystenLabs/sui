// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{Display, Formatter};

use anyhow::Result;
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};
use sui_macros::EnumVariantOrder;

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Eq, Clone, PartialOrd, Ord)]
pub struct StructInput {
    pub address: AccountAddress,
    pub module: String,
    pub name: String,
    // alias for compatibility with old json serialized data.
    #[serde(rename = "type_args", alias = "type_params")]
    pub type_params: Vec<TypeInput>,
}

#[derive(
    Serialize, Deserialize, Debug, PartialEq, Hash, Eq, Clone, PartialOrd, Ord, EnumVariantOrder,
)]
pub enum TypeInput {
    // alias for compatibility with old json serialized data.
    #[serde(rename = "bool", alias = "Bool")]
    Bool,
    #[serde(rename = "u8", alias = "U8")]
    U8,
    #[serde(rename = "u64", alias = "U64")]
    U64,
    #[serde(rename = "u128", alias = "U128")]
    U128,
    #[serde(rename = "address", alias = "Address")]
    Address,
    #[serde(rename = "signer", alias = "Signer")]
    Signer,
    #[serde(rename = "vector", alias = "Vector")]
    Vector(Box<TypeInput>),
    #[serde(rename = "struct", alias = "Struct")]
    Struct(Box<StructInput>),

    // NOTE: Added in bytecode version v6, do not reorder!
    #[serde(rename = "u16", alias = "U16")]
    U16,
    #[serde(rename = "u32", alias = "U32")]
    U32,
    #[serde(rename = "u256", alias = "U256")]
    U256,
}

impl TypeInput {
    /// Return a canonical string representation of the type. All types are represented using their
    /// source syntax:
    ///
    /// - "bool", "u8", "u16", "u32", "u64", "u128", "u256", "address", "signer", "vector" for
    ///   ground types.
    ///
    /// - Structs are represented as fully qualified type names, with or without the prefix "0x"
    ///   depending on the `with_prefix` flag, e.g. `0x000...0001::string::String` or
    ///   `0x000...000a::m::T<0x000...000a::n::U<u64>>`.
    ///
    /// - Addresses are hex-encoded lowercase values of length 32 (zero-padded).
    ///
    /// Note: this function is guaranteed to be stable -- suitable for use inside Move native
    /// functions or the VM. By contrast, this type's `Display` implementation is subject to change
    /// and should be used inside code that needs to return a stable output (e.g. that might be
    /// committed to effects on-chain).
    pub fn to_canonical_string(&self, with_prefix: bool) -> String {
        self.to_canonical_display(with_prefix).to_string()
    }

    /// Return the canonical string representation of the TypeTag conditionally with prefix 0x
    pub fn to_canonical_display(&self, with_prefix: bool) -> impl std::fmt::Display + '_ {
        struct CanonicalDisplay<'a> {
            data: &'a TypeInput,
            with_prefix: bool,
        }

        impl std::fmt::Display for CanonicalDisplay<'_> {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                match self.data {
                    TypeInput::Bool => write!(f, "bool"),
                    TypeInput::U8 => write!(f, "u8"),
                    TypeInput::U16 => write!(f, "u16"),
                    TypeInput::U32 => write!(f, "u32"),
                    TypeInput::U64 => write!(f, "u64"),
                    TypeInput::U128 => write!(f, "u128"),
                    TypeInput::U256 => write!(f, "u256"),
                    TypeInput::Address => write!(f, "address"),
                    TypeInput::Signer => write!(f, "signer"),
                    TypeInput::Vector(t) => {
                        write!(f, "vector<{}>", t.to_canonical_display(self.with_prefix))
                    }
                    TypeInput::Struct(s) => {
                        write!(f, "{}", s.to_canonical_display(self.with_prefix))
                    }
                }
            }
        }

        CanonicalDisplay {
            data: self,
            with_prefix,
        }
    }

    /// Convert the TypeInput into a TypeTag without checking for validity of identifiers within
    /// the StructTag. DO NOT USE UNLESS YOU KNOW WHAT YOU ARE DOING AND WHY THIS IS SAFE TO CALL.
    ///
    /// # Safety
    ///
    /// Preserving existing behaviour for identifier deserialization within type
    /// tags and inputs.
    pub unsafe fn into_type_tag_unchecked(self) -> TypeTag {
        match self {
            TypeInput::Bool => TypeTag::Bool,
            TypeInput::U8 => TypeTag::U8,
            TypeInput::U16 => TypeTag::U16,
            TypeInput::U32 => TypeTag::U32,
            TypeInput::U64 => TypeTag::U64,
            TypeInput::U128 => TypeTag::U128,
            TypeInput::U256 => TypeTag::U256,
            TypeInput::Address => TypeTag::Address,
            TypeInput::Signer => TypeTag::Signer,
            TypeInput::Vector(inner) => TypeTag::Vector(Box::new(inner.into_type_tag_unchecked())),
            TypeInput::Struct(inner) => {
                let StructInput {
                    address,
                    module,
                    name,
                    type_params,
                } = *inner;
                TypeTag::Struct(Box::new(StructTag {
                    address,
                    module: Identifier::new_unchecked(module),
                    name: Identifier::new_unchecked(name),
                    type_params: type_params
                        .into_iter()
                        .map(|ty| ty.into_type_tag_unchecked())
                        .collect(),
                }))
            }
        }
    }

    /// Convert to a `TypeTag` consuming `self`. This can fail if this value includes invalid
    /// identifiers.
    pub fn into_type_tag(self) -> Result<TypeTag> {
        use TypeInput as I;
        use TypeTag as T;
        Ok(match self {
            I::Bool => T::Bool,
            I::U8 => T::U8,
            I::U16 => T::U16,
            I::U32 => T::U32,
            I::U64 => T::U64,
            I::U128 => T::U128,
            I::U256 => T::U256,
            I::Address => T::Address,
            I::Signer => T::Signer,
            I::Vector(t) => T::Vector(Box::new(t.into_type_tag()?)),
            I::Struct(s) => {
                let StructInput {
                    address,
                    module,
                    name,
                    type_params,
                } = *s;
                let type_params = type_params
                    .into_iter()
                    .map(|t| t.into_type_tag())
                    .collect::<Result<_>>()?;
                T::Struct(Box::new(StructTag {
                    address,
                    module: Identifier::new(module)?,
                    name: Identifier::new(name)?,
                    type_params,
                }))
            }
        })
    }

    /// Conversion to a `TypeTag`, which can fail if this value includes invalid identifiers.
    pub fn as_type_tag(&self) -> Result<TypeTag> {
        use TypeInput as I;
        use TypeTag as T;
        Ok(match self {
            I::Bool => T::Bool,
            I::U8 => T::U8,
            I::U16 => T::U16,
            I::U32 => T::U32,
            I::U64 => T::U64,
            I::U128 => T::U128,
            I::U256 => T::U256,
            I::Address => T::Address,
            I::Signer => T::Signer,
            I::Vector(t) => T::Vector(Box::new(t.as_type_tag()?)),
            I::Struct(s) => {
                let StructInput {
                    address,
                    module,
                    name,
                    type_params,
                } = s.as_ref();
                let type_params = type_params
                    .iter()
                    .map(|t| t.as_type_tag())
                    .collect::<Result<_>>()?;
                T::Struct(Box::new(StructTag {
                    address: *address,
                    module: Identifier::new(module.to_owned())?,
                    name: Identifier::new(name.to_owned())?,
                    type_params,
                }))
            }
        })
    }
}

impl StructInput {
    /// Return a canonical string representation of the struct.
    ///
    /// - Structs are represented as fully qualified type names, with or without the prefix "0x"
    ///   depending on the `with_prefix` flag, e.g. `0x000...0001::string::String` or
    ///   `0x000...000a::m::T<0x000...000a::n::U<u64>>`.
    ///
    /// - Addresses are hex-encoded lowercase values of length 32 (zero-padded).
    ///
    /// Note: this function is guaranteed to be stable -- suitable for use inside Move native
    /// functions or the VM. By contrast, this type's `Display` implementation is subject to change
    /// and should be used inside code that needs to return a stable output (e.g. that might be
    /// committed to effects on-chain).
    pub fn to_canonical_string(&self, with_prefix: bool) -> String {
        self.to_canonical_display(with_prefix).to_string()
    }

    /// Implements the canonical string representation of the StructTag with the prefix 0x
    pub fn to_canonical_display(&self, with_prefix: bool) -> impl std::fmt::Display + '_ {
        struct CanonicalDisplay<'a> {
            data: &'a StructInput,
            with_prefix: bool,
        }

        impl std::fmt::Display for CanonicalDisplay<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(
                    f,
                    "{}::{}::{}",
                    self.data.address.to_canonical_display(self.with_prefix),
                    self.data.module,
                    self.data.name
                )?;

                if let Some(first_ty) = self.data.type_params.first() {
                    write!(f, "<")?;
                    write!(f, "{}", first_ty.to_canonical_display(self.with_prefix))?;
                    for ty in self.data.type_params.iter().skip(1) {
                        // Note that unlike Display for StructTag, there is no space between the comma and canonical display.
                        // This follows the original to_canonical_string() implementation.
                        write!(f, ",{}", ty.to_canonical_display(self.with_prefix))?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
        }

        CanonicalDisplay {
            data: self,
            with_prefix,
        }
    }
}

impl From<TypeTag> for TypeInput {
    fn from(tag: TypeTag) -> Self {
        match tag {
            TypeTag::Bool => TypeInput::Bool,
            TypeTag::U8 => TypeInput::U8,
            TypeTag::U64 => TypeInput::U64,
            TypeTag::U128 => TypeInput::U128,
            TypeTag::Address => TypeInput::Address,
            TypeTag::Signer => TypeInput::Signer,
            TypeTag::Vector(inner) => TypeInput::Vector(Box::new(TypeInput::from(*inner))),
            TypeTag::Struct(inner) => TypeInput::Struct(Box::new(StructInput::from(*inner))),
            TypeTag::U16 => TypeInput::U16,
            TypeTag::U32 => TypeInput::U32,
            TypeTag::U256 => TypeInput::U256,
        }
    }
}

impl From<StructTag> for StructInput {
    fn from(tag: StructTag) -> Self {
        StructInput {
            address: tag.address,
            module: tag.module.to_string(),
            name: tag.name.to_string(),
            type_params: tag.type_params.into_iter().map(TypeInput::from).collect(),
        }
    }
}

impl From<StructInput> for TypeInput {
    fn from(t: StructInput) -> TypeInput {
        TypeInput::Struct(Box::new(t))
    }
}

impl Display for StructInput {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "0x{}::{}::{}",
            self.address.short_str_lossless(),
            self.module,
            self.name
        )?;

        let mut prefix = "<";
        for ty in &self.type_params {
            write!(f, "{}{}", prefix, ty)?;
            prefix = ", ";
        }
        if !self.type_params.is_empty() {
            write!(f, ">")?;
        }

        Ok(())
    }
}

impl Display for TypeInput {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeInput::Struct(s) => write!(f, "{}", s),
            TypeInput::Vector(ty) => write!(f, "vector<{}>", ty),
            TypeInput::U8 => write!(f, "u8"),
            TypeInput::U16 => write!(f, "u16"),
            TypeInput::U32 => write!(f, "u32"),
            TypeInput::U64 => write!(f, "u64"),
            TypeInput::U128 => write!(f, "u128"),
            TypeInput::U256 => write!(f, "u256"),
            TypeInput::Address => write!(f, "address"),
            TypeInput::Signer => write!(f, "signer"),
            TypeInput::Bool => write!(f, "bool"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::TypeInput;
    use sui_enum_compat_util::*;

    #[test]
    fn enforce_order_test() {
        let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.extend(["tests", "staged", "type_input.yaml"]);
        check_enum_compat_order::<TypeInput>(path);
    }
}
