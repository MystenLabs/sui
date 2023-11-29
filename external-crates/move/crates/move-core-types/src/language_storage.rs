// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    account_address::AccountAddress,
    gas_algebra::{AbstractMemorySize, BOX_ABSTRACT_SIZE, ENUM_BASE_ABSTRACT_SIZE},
    identifier::{IdentStr, Identifier},
    parser::{parse_struct_tag, parse_type_tag},
};
use move_proc_macros::test_variant_order;
use once_cell::sync::Lazy;
#[cfg(any(test, feature = "fuzzing"))]
use proptest_derive::Arbitrary;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};

pub const CODE_TAG: u8 = 0;
pub const RESOURCE_TAG: u8 = 1;

/// Hex address: 0x1
pub const CORE_CODE_ADDRESS: AccountAddress = AccountAddress::ONE;

/// Rough estimate of abstract size for TypeTag
pub static TYPETAG_ENUM_ABSTRACT_SIZE: Lazy<AbstractMemorySize> =
    Lazy::new(|| ENUM_BASE_ABSTRACT_SIZE + BOX_ABSTRACT_SIZE);

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Eq, Clone, PartialOrd, Ord)]
#[test_variant_order(src/unit_tests/staged_enum_variant_order/type_tag.yaml)]
pub enum TypeTag {
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
    Vector(Box<TypeTag>),
    #[serde(rename = "struct", alias = "Struct")]
    Struct(Box<StructTag>),

    // NOTE: Added in bytecode version v6, do not reorder!
    #[serde(rename = "u16", alias = "U16")]
    U16,
    #[serde(rename = "u32", alias = "U32")]
    U32,
    #[serde(rename = "u256", alias = "U256")]
    U256,
}

impl TypeTag {
    /// Return a canonical string representation of the type. All types are represented
    /// using their source syntax:
    /// "u8", "u64", "u128", "bool", "address", "vector", "signer" for ground types.
    /// Struct types are represented as fully qualified type names; e.g.
    /// `00000000000000000000000000000001::string::String` or
    /// `0000000000000000000000000000000a::module_name1::type_name1<0000000000000000000000000000000a::module_name2::type_name2<u64>>`
    /// With or without the prefix 0x depending on the `with_prefix` flag.
    /// Addresses are hex-encoded lowercase values of length ADDRESS_LENGTH (16, 20, or 32 depending on the Move platform)
    /// Note: this function is guaranteed to be stable, and this is suitable for use inside
    /// Move native functions or the VM. By contrast, the `Display` implementation is subject
    /// to change and should not be used inside stable code.
    pub fn to_canonical_string(&self, with_prefix: bool) -> String {
        self.to_canonical_display(with_prefix).to_string()
    }

    /// Return the canonical string representation of the TypeTag conditionally with prefix 0x
    pub fn to_canonical_display(&self, with_prefix: bool) -> impl std::fmt::Display + '_ {
        struct CanonicalDisplay<'a> {
            data: &'a TypeTag,
            with_prefix: bool,
        }

        impl std::fmt::Display for CanonicalDisplay<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                match self.data {
                    TypeTag::Bool => write!(f, "bool"),
                    TypeTag::U8 => write!(f, "u8"),
                    TypeTag::U16 => write!(f, "u16"),
                    TypeTag::U32 => write!(f, "u32"),
                    TypeTag::U64 => write!(f, "u64"),
                    TypeTag::U128 => write!(f, "u128"),
                    TypeTag::U256 => write!(f, "u256"),
                    TypeTag::Address => write!(f, "address"),
                    TypeTag::Signer => write!(f, "signer"),
                    TypeTag::Vector(t) => {
                        write!(f, "vector<{}>", t.to_canonical_display(self.with_prefix))
                    }
                    TypeTag::Struct(s) => write!(f, "{}", s.to_canonical_display(self.with_prefix)),
                }
            }
        }

        CanonicalDisplay {
            data: self,
            with_prefix,
        }
    }

    /// Return the abstract size we use for gas metering
    /// This size might be imperfect but should be consistent across platforms
    /// TODO (ade): use macro to enfornce determinism
    pub fn abstract_size_for_gas_metering(&self) -> AbstractMemorySize {
        *TYPETAG_ENUM_ABSTRACT_SIZE
            + match self {
                TypeTag::Bool
                | TypeTag::U8
                | TypeTag::U64
                | TypeTag::U128
                | TypeTag::Address
                | TypeTag::Signer
                | TypeTag::U16
                | TypeTag::U32
                | TypeTag::U256 => AbstractMemorySize::new(0),
                TypeTag::Vector(x) => x.abstract_size_for_gas_metering(),
                TypeTag::Struct(y) => y.abstract_size_for_gas_metering(),
            }
    }
}

impl FromStr for TypeTag {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_type_tag(s)
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Eq, Clone, PartialOrd, Ord)]
pub struct StructTag {
    pub address: AccountAddress,
    pub module: Identifier,
    pub name: Identifier,
    // alias for compatibility with old json serialized data.
    #[serde(rename = "type_args", alias = "type_params")]
    pub type_params: Vec<TypeTag>,
}

impl StructTag {
    pub fn access_vector(&self) -> Vec<u8> {
        let mut key = vec![RESOURCE_TAG];
        key.append(&mut bcs::to_bytes(self).unwrap());
        key
    }

    /// Returns true if this is a `StructTag` for an `std::ascii::String` struct defined in the
    /// standard library at address `move_std_addr`.
    pub fn is_ascii_string(&self, move_std_addr: &AccountAddress) -> bool {
        self.address == *move_std_addr
            && self.module.as_str().eq("ascii")
            && self.name.as_str().eq("String")
    }

    /// Returns true if this is a `StructTag` for an `std::string::String` struct defined in the
    /// standard library at address `move_std_addr`.
    pub fn is_std_string(&self, move_std_addr: &AccountAddress) -> bool {
        self.address == *move_std_addr
            && self.module.as_str().eq("string")
            && self.name.as_str().eq("String")
    }

    pub fn module_id(&self) -> ModuleId {
        ModuleId::new(self.address, self.module.to_owned())
    }

    /// Return a canonical string representation of the struct.
    /// Struct types are represented as fully qualified type names; e.g.
    /// `00000000000000000000000000000001::string::String`,
    /// `0000000000000000000000000000000a::module_name1::type_name1<0000000000000000000000000000000a::module_name2::type_name2<u64>>`,
    /// or `0000000000000000000000000000000a::module_name2::type_name2<bool,u64,u128>.
    /// With or without the prefix 0x depending on the `with_prefix` flag.
    /// Addresses are hex-encoded lowercase values of length ADDRESS_LENGTH (16, 20, or 32 depending on the Move platform)
    /// Note: this function is guaranteed to be stable, and this is suitable for use inside
    /// Move native functions or the VM. By contrast, the `Display` implementation is subject
    /// to change and should not be used inside stable code.
    pub fn to_canonical_string(&self, with_prefix: bool) -> String {
        self.to_canonical_display(with_prefix).to_string()
    }

    /// Implements the canonical string representation of the StructTag with the prefix 0x
    pub fn to_canonical_display(&self, with_prefix: bool) -> impl std::fmt::Display + '_ {
        struct CanonicalDisplay<'a> {
            data: &'a StructTag,
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

    /// Return the abstract size we use for gas metering
    /// This size might be imperfect but should be consistent across platforms
    /// TODO (ade): use macro to enfornce determinism
    pub fn abstract_size_for_gas_metering(&self) -> AbstractMemorySize {
        // TODO: make this more robust as struct size changes
        self.address.abstract_size_for_gas_metering()
            + self.module.abstract_size_for_gas_metering()
            + self.name.abstract_size_for_gas_metering()
            + self
                .type_params
                .iter()
                .fold(AbstractMemorySize::new(0), |accum, val| {
                    accum + val.abstract_size_for_gas_metering()
                })
    }
}

impl FromStr for StructTag {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_struct_tag(s)
    }
}

/// Represents the initial key into global storage where we first index by the address, and then
/// the struct tag
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Eq, Clone, PartialOrd, Ord)]
pub struct ResourceKey {
    pub address: AccountAddress,
    pub type_: StructTag,
}

impl ResourceKey {
    pub fn address(&self) -> AccountAddress {
        self.address
    }

    pub fn type_(&self) -> &StructTag {
        &self.type_
    }
}

impl ResourceKey {
    pub fn new(address: AccountAddress, type_: StructTag) -> Self {
        ResourceKey { address, type_ }
    }
}

/// Represents the initial key into global storage where we first index by the address, and then
/// the struct tag
#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Eq, Clone, PartialOrd, Ord)]
#[cfg_attr(any(test, feature = "fuzzing"), derive(Arbitrary))]
#[cfg_attr(any(test, feature = "fuzzing"), proptest(no_params))]
pub struct ModuleId {
    address: AccountAddress,
    name: Identifier,
}

impl From<ModuleId> for (AccountAddress, Identifier) {
    fn from(module_id: ModuleId) -> Self {
        (module_id.address, module_id.name)
    }
}

impl ModuleId {
    pub fn new(address: AccountAddress, name: Identifier) -> Self {
        ModuleId { address, name }
    }

    pub fn name(&self) -> &IdentStr {
        &self.name
    }

    pub fn address(&self) -> &AccountAddress {
        &self.address
    }

    pub fn access_vector(&self) -> Vec<u8> {
        let mut key = vec![CODE_TAG];
        key.append(&mut bcs::to_bytes(self).unwrap());
        key
    }

    /// Proxy type for overriding `ModuleId`'s display implementation, to use a canonical form
    /// (full-width addresses), with an optional "0x" prefix (controlled by the `with_prefix` flag).
    pub fn to_canonical_display(&self, with_prefix: bool) -> impl Display + '_ {
        struct IdDisplay<'a> {
            id: &'a ModuleId,
            with_prefix: bool,
        }

        impl<'a> Display for IdDisplay<'a> {
            fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
                write!(
                    f,
                    "{}::{}",
                    self.id.address.to_canonical_display(self.with_prefix),
                    self.id.name,
                )
            }
        }

        IdDisplay {
            id: self,
            with_prefix,
        }
    }
}

impl Display for ModuleId {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.to_canonical_display(/* with_prefix */ false))
    }
}

impl ModuleId {
    pub fn short_str_lossless(&self) -> String {
        format!("0x{}::{}", self.address.short_str_lossless(), self.name)
    }
}

impl Display for StructTag {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(
            f,
            "0x{}::{}::{}",
            self.address.short_str_lossless(),
            self.module,
            self.name
        )?;
        if let Some(first_ty) = self.type_params.first() {
            write!(f, "<")?;
            write!(f, "{}", first_ty)?;
            for ty in self.type_params.iter().skip(1) {
                write!(f, ", {}", ty)?;
            }
            write!(f, ">")?;
        }
        Ok(())
    }
}

impl Display for TypeTag {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            TypeTag::Struct(s) => write!(f, "{}", s),
            TypeTag::Vector(ty) => write!(f, "vector<{}>", ty),
            TypeTag::U8 => write!(f, "u8"),
            TypeTag::U16 => write!(f, "u16"),
            TypeTag::U32 => write!(f, "u32"),
            TypeTag::U64 => write!(f, "u64"),
            TypeTag::U128 => write!(f, "u128"),
            TypeTag::U256 => write!(f, "u256"),
            TypeTag::Address => write!(f, "address"),
            TypeTag::Signer => write!(f, "signer"),
            TypeTag::Bool => write!(f, "bool"),
        }
    }
}

impl Display for ResourceKey {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "0x{}/{}", self.address.short_str_lossless(), self.type_)
    }
}

impl From<StructTag> for TypeTag {
    fn from(t: StructTag) -> TypeTag {
        TypeTag::Struct(Box::new(t))
    }
}

#[cfg(test)]
mod tests {
    use super::{ModuleId, TypeTag};
    use crate::{
        account_address::AccountAddress, ident_str, identifier::Identifier,
        language_storage::StructTag,
    };
    use std::mem;

    #[test]
    fn test_type_tag_serde() {
        let a = TypeTag::Struct(Box::new(StructTag {
            address: AccountAddress::ONE,
            module: Identifier::from_utf8(("abc".as_bytes()).to_vec()).unwrap(),
            name: Identifier::from_utf8(("abc".as_bytes()).to_vec()).unwrap(),
            type_params: vec![TypeTag::U8],
        }));
        let b = serde_json::to_string(&a).unwrap();
        let c: TypeTag = serde_json::from_str(&b).unwrap();
        assert!(a.eq(&c), "Typetag serde error");
        assert_eq!(mem::size_of::<TypeTag>(), 16);
    }

    #[test]
    fn test_module_id_display() {
        let id = ModuleId::new(AccountAddress::ONE, ident_str!("foo").to_owned());

        assert_eq!(
            format!("{id}"),
            "0000000000000000000000000000000000000000000000000000000000000001::foo",
        );

        assert_eq!(
            format!("{}", id.to_canonical_display(/* with_prefix */ false)),
            "0000000000000000000000000000000000000000000000000000000000000001::foo",
        );

        assert_eq!(
            format!("{}", id.to_canonical_display(/* with_prefix */ true)),
            "0x0000000000000000000000000000000000000000000000000000000000000001::foo",
        );
    }
}
