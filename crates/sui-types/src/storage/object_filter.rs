// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Type filters for listing the objects an address owns and the dynamic fields
//! of an object, independent of how an index stores them.
//!
//! Both filters are in disjunctive normal form: a [`DnfFilter`] matches when
//! any of its terms matches, and a term matches when every one of its
//! literals does. A literal is a predicate, optionally negated.

use std::str::FromStr;

use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use move_core_types::language_storage::TypeTag;

/// A Move struct type at one of four levels of specificity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructTypePattern {
    /// Every struct type defined at this address.
    Package(AccountAddress),
    /// Every struct type defined in this module.
    Module {
        package: AccountAddress,
        module: Identifier,
    },
    /// A struct type. With empty `type_params` it matches every
    /// instantiation of the type; otherwise only this exact instantiation.
    Type(StructTag),
}

/// Parses `address`, `address::module`, `address::module::Name` or
/// `address::module::Name<T1, ..>`. Addresses may be named (`sui`, `std`, ..).
impl FromStr for StructTypePattern {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let head = s.split_once('<').map_or(s, |(head, _)| head);
        match head.split("::").count() {
            1 => Ok(Self::Package(crate::parse_sui_address(s)?.into())),
            2 => {
                let module_id = crate::parse_sui_module_id(s)?;
                Ok(Self::Module {
                    package: *module_id.address(),
                    module: module_id.name().to_owned(),
                })
            }
            3 => Ok(Self::Type(crate::parse_sui_struct_tag(s)?)),
            _ => anyhow::bail!("expected `address[::module[::Name[<type_params>]]]`"),
        }
    }
}

/// The type of a dynamic field's name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameTypePattern {
    /// A struct type, at any level of specificity.
    Struct(StructTypePattern),
    /// Exactly this non-struct type, e.g. `u64` or `vector<u8>`.
    Exact(TypeTag),
}

/// Parses a complete Move type, or a struct type pattern as accepted by
/// [`StructTypePattern`].
impl FromStr for NameTypePattern {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match crate::parse_sui_type_tag(s) {
            Ok(TypeTag::Struct(tag)) => Ok(Self::Struct(StructTypePattern::Type(*tag))),
            Ok(name_type) => Ok(Self::Exact(name_type)),
            // `address` and `address::module` are struct patterns but not types.
            Err(type_error) => s
                .parse::<StructTypePattern>()
                .map(Self::Struct)
                .map_err(|_| type_error),
        }
    }
}

/// A predicate on a dynamic field, evaluated against the type of its
/// `0x2::dynamic_field::Field<Name, Value>` object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynamicFieldPredicate {
    /// The field's name type. For a dynamic object field this is the name
    /// type wrapped in `0x2::dynamic_object_field::Wrapper`, not the wrapper.
    NameType(NameTypePattern),
    /// The field was added with `0x2::dynamic_object_field`.
    DynamicObjectField,
}

/// A predicate, optionally negated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterLiteral<P> {
    pub negated: bool,
    pub predicate: P,
}

/// An OR of terms, each an AND of literals. A filter with no terms matches
/// nothing; a term with no literals matches everything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DnfFilter<P> {
    pub terms: Vec<Vec<FilterLiteral<P>>>,
}

/// Filters owned objects by their Move type.
pub type OwnedObjectFilter = DnfFilter<StructTypePattern>;

impl OwnedObjectFilter {
    /// Matches objects of `object_type`, with the instantiation semantics of
    /// [`StructTypePattern::Type`].
    pub fn object_type(object_type: StructTag) -> Self {
        Self {
            terms: vec![vec![FilterLiteral {
                negated: false,
                predicate: StructTypePattern::Type(object_type),
            }]],
        }
    }
}

/// Filters dynamic fields by their name type and kind.
pub type DynamicFieldFilter = DnfFilter<DynamicFieldPredicate>;

#[cfg(test)]
mod tests {
    use super::*;

    fn struct_tag(s: &str) -> StructTag {
        crate::parse_sui_struct_tag(s).unwrap()
    }

    #[test]
    fn struct_type_pattern_parses_each_level() {
        assert_eq!(
            "sui".parse::<StructTypePattern>().unwrap(),
            StructTypePattern::Package(AccountAddress::TWO),
        );
        assert_eq!(
            "0x2::kiosk".parse::<StructTypePattern>().unwrap(),
            StructTypePattern::Module {
                package: AccountAddress::TWO,
                module: Identifier::new("kiosk").unwrap(),
            },
        );
        assert_eq!(
            "0x2::coin::Coin<0x2::sui::SUI>"
                .parse::<StructTypePattern>()
                .unwrap(),
            StructTypePattern::Type(struct_tag("0x2::coin::Coin<0x2::sui::SUI>")),
        );
        assert!("0x2::a::B::c".parse::<StructTypePattern>().is_err());
        assert!("0x2::kiosk<u8>".parse::<StructTypePattern>().is_err());
    }

    #[test]
    fn name_type_pattern_accepts_types_and_struct_patterns() {
        assert_eq!(
            "u64".parse::<NameTypePattern>().unwrap(),
            NameTypePattern::Exact(TypeTag::U64),
        );
        assert_eq!(
            "0x2".parse::<NameTypePattern>().unwrap(),
            NameTypePattern::Struct(StructTypePattern::Package(AccountAddress::TWO)),
        );
        assert!("u64::x".parse::<NameTypePattern>().is_err());
    }
}
