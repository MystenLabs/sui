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
use serde::Deserialize;
use serde::Serialize;

use crate::dynamic_field::DynamicFieldInfo;

/// A Move struct type at one of four levels of specificity.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

impl StructTypePattern {
    pub fn matches(&self, tag: &StructTag) -> bool {
        match self {
            Self::Package(package) => tag.address == *package,
            Self::Module { package, module } => tag.address == *package && tag.module == *module,
            Self::Type(pattern) if pattern.type_params.is_empty() => {
                tag.address == pattern.address
                    && tag.module == pattern.module
                    && tag.name == pattern.name
            }
            Self::Type(pattern) => tag == pattern,
        }
    }
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
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NameTypePattern {
    /// A struct type, at any level of specificity.
    Struct(StructTypePattern),
    /// Exactly this non-struct type, e.g. `u64` or `vector<u8>`.
    Exact(TypeTag),
}

impl NameTypePattern {
    pub fn matches(&self, name_type: &TypeTag) -> bool {
        match (self, name_type) {
            (Self::Struct(pattern), TypeTag::Struct(tag)) => pattern.matches(tag),
            (Self::Struct(_), _) => false,
            (Self::Exact(pattern), name_type) => pattern == name_type,
        }
    }
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
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DynamicFieldPredicate {
    /// The field's name type. For a dynamic object field this is the name
    /// type wrapped in `0x2::dynamic_object_field::Wrapper`, not the wrapper.
    NameType(NameTypePattern),
    /// The field was added with `0x2::dynamic_object_field`.
    DynamicObjectField,
}

impl DynamicFieldPredicate {
    pub fn matches(&self, field_type: &StructTag) -> bool {
        let Some(field) = DynamicFieldTypeParts::new(field_type) else {
            return false;
        };
        match self {
            Self::NameType(pattern) => field.name_type.is_some_and(|name| pattern.matches(name)),
            Self::DynamicObjectField => field.is_dynamic_object_field,
        }
    }
}

/// The name type and kind encoded in a `Field<Name, Value>` type.
struct DynamicFieldTypeParts<'a> {
    /// `None` only for a malformed `Wrapper` without exactly one type parameter.
    name_type: Option<&'a TypeTag>,
    is_dynamic_object_field: bool,
}

impl<'a> DynamicFieldTypeParts<'a> {
    fn new(field_type: &'a StructTag) -> Option<Self> {
        if !DynamicFieldInfo::is_dynamic_field(field_type) {
            return None;
        }
        let [name_type, _value_type] = field_type.type_params.as_slice() else {
            return None;
        };
        Some(match name_type {
            TypeTag::Struct(wrapper)
                if DynamicFieldInfo::is_dynamic_object_field_wrapper(wrapper) =>
            {
                Self {
                    name_type: match wrapper.type_params.as_slice() {
                        [name_type] => Some(name_type),
                        _ => None,
                    },
                    is_dynamic_object_field: true,
                }
            }
            name_type => Self {
                name_type: Some(name_type),
                is_dynamic_object_field: false,
            },
        })
    }
}

/// A predicate, optionally negated.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FilterLiteral<P> {
    pub negated: bool,
    pub predicate: P,
}

/// An OR of terms, each an AND of literals. A filter with no terms matches
/// nothing; a term with no literals matches everything.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DnfFilter<P> {
    pub terms: Vec<Vec<FilterLiteral<P>>>,
}

impl<P> DnfFilter<P> {
    /// Whether the filter accepts a value that each predicate is evaluated
    /// against with `predicate_matches`.
    pub fn matches(&self, mut predicate_matches: impl FnMut(&P) -> bool) -> bool {
        self.terms.iter().any(|term| {
            term.iter()
                .all(|literal| predicate_matches(&literal.predicate) != literal.negated)
        })
    }

    pub fn literal_count(&self) -> usize {
        self.terms.iter().map(Vec::len).sum()
    }
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

    pub fn matches_object_type(&self, object_type: &StructTag) -> bool {
        self.matches(|pattern| pattern.matches(object_type))
    }
}

/// Filters dynamic fields by their name type and kind.
pub type DynamicFieldFilter = DnfFilter<DynamicFieldPredicate>;

impl DynamicFieldFilter {
    /// Whether the filter accepts the dynamic field whose `Field` object has
    /// type `field_type`.
    pub fn matches_field_type(&self, field_type: &StructTag) -> bool {
        self.matches(|predicate| predicate.matches(field_type))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn struct_tag(s: &str) -> StructTag {
        crate::parse_sui_struct_tag(s).unwrap()
    }

    fn type_tag(s: &str) -> TypeTag {
        crate::parse_sui_type_tag(s).unwrap()
    }

    fn dynamic_field(name: &str) -> StructTag {
        DynamicFieldInfo::dynamic_field_type(type_tag(name), type_tag("u64"))
    }

    fn dynamic_object_field(name: &str) -> StructTag {
        DynamicFieldInfo::dynamic_field_type(
            TypeTag::Struct(Box::new(DynamicFieldInfo::dynamic_object_field_wrapper(
                type_tag(name),
            ))),
            type_tag("0x2::object::ID"),
        )
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
    fn type_without_params_matches_every_instantiation() {
        let any_coin: StructTypePattern = "0x2::coin::Coin".parse().unwrap();
        let sui_coin: StructTypePattern = "0x2::coin::Coin<0x2::sui::SUI>".parse().unwrap();
        let usdc = struct_tag("0x2::coin::Coin<0xa::usdc::USDC>");
        assert!(any_coin.matches(&usdc));
        assert!(!sui_coin.matches(&usdc));
        assert!(!any_coin.matches(&struct_tag("0x2::coin::CoinMetadata<0x2::sui::SUI>")));
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

    #[test]
    fn name_type_matches_the_unwrapped_name_of_dynamic_object_fields() {
        let item = DynamicFieldPredicate::NameType("0x2::kiosk::Item".parse().unwrap());
        assert!(item.matches(&dynamic_field("0x2::kiosk::Item")));
        assert!(item.matches(&dynamic_object_field("0x2::kiosk::Item")));

        // The wrapper is defined at 0x2, but the name type is not.
        let framework = DynamicFieldPredicate::NameType("0x2".parse().unwrap());
        assert!(!framework.matches(&dynamic_object_field("0xabc::m::Key")));
        assert!(framework.matches(&dynamic_object_field("0x2::kiosk::Item")));

        let is_object = DynamicFieldPredicate::DynamicObjectField;
        assert!(is_object.matches(&dynamic_object_field("u64")));
        assert!(!is_object.matches(&dynamic_field("u64")));
        assert!(!is_object.matches(&struct_tag("0x2::coin::Coin<0x2::sui::SUI>")));
    }

    #[test]
    fn dnf_filter_ors_terms_and_ands_literals() {
        let not = |s: &str| FilterLiteral {
            negated: true,
            predicate: s.parse::<StructTypePattern>().unwrap(),
        };
        // Everything except coins and SuiNS registrations.
        let filter = OwnedObjectFilter {
            terms: vec![vec![
                not("0x2::coin::Coin"),
                not("0xd22b::suins_registration"),
            ]],
        };
        assert!(!filter.matches_object_type(&struct_tag("0x2::coin::Coin<0x2::sui::SUI>")));
        assert!(
            !filter
                .matches_object_type(&struct_tag("0xd22b::suins_registration::SuinsRegistration"))
        );
        assert!(filter.matches_object_type(&struct_tag("0x2::kiosk::KioskOwnerCap")));
        assert!(!OwnedObjectFilter { terms: vec![] }.matches_object_type(&struct_tag("0x2::a::B")));
    }
}
