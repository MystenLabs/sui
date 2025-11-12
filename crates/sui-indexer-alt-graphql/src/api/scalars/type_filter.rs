// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use move_core_types::language_storage::StructTag;
use std::{fmt, str::FromStr};
use sui_types::{
    TypeTag, parse_sui_address, parse_sui_module_id, parse_sui_struct_tag, parse_sui_type_tag,
};

use crate::api::scalars::{impl_string_input, sui_address::SuiAddress};

/// A GraphQL scalar for accepting a type as input (exact type with all type parameters).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TypeInput(pub TypeTag);

/// GraphQL scalar containing a filter on types. The filter can be one of:
///
/// - A package address: `0x2`,
/// - A module: `0x2::coin`,
/// - A fully-qualified name: `0x2::coin::Coin`,
/// - A type instantiation: `0x2::coin::Coin<0x2::sui::SUI>`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TypeFilter {
    /// Filter by package address
    Package(SuiAddress),
    /// Filter by module (package and module name)
    Module(SuiAddress, String),
    /// Filter by type (with or without type parameters)
    Type(StructTag),
}

#[derive(thiserror::Error, Debug)]
#[error("Invalid type, expected: package::module::type[<type_params, ...>] or primitive type")]
pub(crate) struct TypeInputError;

#[derive(thiserror::Error, Debug)]
#[error("Invalid filter format, expected: package[::module[::type[<type_params, ...>]]]")]
pub(crate) struct TypeFilterError;

impl TypeFilter {
    /// Returns the package address if this filter contains one
    pub(crate) fn package(&self) -> SuiAddress {
        match self {
            TypeFilter::Package(p) => *p,
            TypeFilter::Module(p, _) => *p,
            TypeFilter::Type(t) => t.address.into(),
        }
    }

    /// Returns the module name if this filter contains one
    pub(crate) fn module(&self) -> Option<&str> {
        match self {
            TypeFilter::Package(_) => None,
            TypeFilter::Module(_, m) => Some(m.as_str()),
            TypeFilter::Type(t) => Some(t.module.as_str()),
        }
    }

    /// Returns the type name if this filter contains one
    pub(crate) fn type_name(&self) -> Option<&str> {
        match self {
            TypeFilter::Package(_) | TypeFilter::Module(_, _) => None,
            TypeFilter::Type(t) => Some(t.name.as_str()),
        }
    }

    /// Returns the type's type parameters if this filter has any
    pub(crate) fn type_params(&self) -> Option<&[TypeTag]> {
        match self {
            TypeFilter::Type(t) if !t.type_params.is_empty() => Some(&t.type_params),
            TypeFilter::Package(_) | TypeFilter::Module(_, _) | TypeFilter::Type(_) => None,
        }
    }

    /// Try to create a filter whose results are the intersection of `self`'s results and `other`'s
    /// results. May return `None` if the filters are incompatible (would result in no matches)
    pub(crate) fn intersect(self, other: Self) -> Option<Self> {
        use TypeFilter as F;

        match (&self, &other) {
            (F::Package(p), F::Package(q)) => (p == q).then_some(self),

            (F::Package(p), F::Module(q, _)) => (p == q).then_some(other),
            (F::Module(p, _), F::Package(q)) => (p == q).then_some(self),

            (F::Package(p), F::Type(t)) => (p == &t.address.into()).then_some(other),
            (F::Type(t), F::Package(p)) => (p == &t.address.into()).then_some(self),

            (F::Module(p, m), F::Module(q, n)) => ((p, m) == (q, n)).then_some(self),

            (F::Module(p, m), F::Type(t)) => {
                ((p, m.as_str()) == (&t.address.into(), t.module.as_str())).then_some(other)
            }

            (F::Type(t), F::Module(p, m)) => {
                ((p, m.as_str()) == (&t.address.into(), t.module.as_str())).then_some(self)
            }

            (F::Type(t), F::Type(u)) if t.type_params.is_empty() => {
                ((&t.address, &t.module, &t.name) == (&u.address, &u.module, &u.name))
                    .then_some(other)
            }

            (F::Type(t), F::Type(u)) if u.type_params.is_empty() => {
                ((&t.address, &t.module, &t.name) == (&u.address, &u.module, &u.name))
                    .then_some(self)
            }

            // If both sides are Type filters, then at this point, we know both have type
            // parameteres so are exact type filters, which must match exactly to intersect.
            (F::Type(t), F::Type(u)) => (t == u).then_some(self),
        }
    }
}

impl_string_input!(TypeInput);
impl_string_input!(TypeFilter);

impl From<TypeInput> for TypeTag {
    fn from(input: TypeInput) -> Self {
        input.0
    }
}

impl FromStr for TypeInput {
    type Err = TypeInputError;

    fn from_str(s: &str) -> Result<Self, TypeInputError> {
        if let Ok(tag) = parse_sui_type_tag(s) {
            Ok(TypeInput(tag))
        } else {
            Err(TypeInputError)
        }
    }
}

impl FromStr for TypeFilter {
    type Err = TypeFilterError;

    fn from_str(s: &str) -> Result<Self, TypeFilterError> {
        if let Ok(tag) = parse_sui_struct_tag(s) {
            // Try to parse as a struct tag first (most specific)
            Ok(TypeFilter::Type(tag))
        } else if let Ok(module) = parse_sui_module_id(s) {
            // Then try as a module ID
            Ok(TypeFilter::Module(
                SuiAddress::from(*module.address()),
                module.name().to_string(),
            ))
        } else if let Ok(package) = parse_sui_address(s) {
            // Finally try as just a package address
            Ok(TypeFilter::Package(package.into()))
        } else {
            Err(TypeFilterError)
        }
    }
}

impl fmt::Display for TypeInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.to_canonical_display(/* with_prefix */ true))
    }
}

impl fmt::Display for TypeFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeFilter::Package(p) => write!(f, "{p}"),
            TypeFilter::Module(p, m) => write!(f, "{p}::{m}"),
            TypeFilter::Type(t) => write!(f, "{}", t.to_canonical_display(/* with_prefix */ true)),
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use super::*;

    #[test]
    fn test_parse_type_input() {
        let inputs: Vec<_> = [
            "u8",
            "address",
            "bool",
            "0x2::coin::Coin",
            "0x2::coin::Coin<0x2::sui::SUI>",
            "vector<u256>",
            "vector<0x3::staking_pool::StakedSui>",
        ]
        .into_iter()
        .map(|i| TypeInput::from_str(i).unwrap().to_string())
        .collect();

        assert_snapshot!(&inputs.join("\n"), @r###"
        u8
        address
        bool
        0x0000000000000000000000000000000000000000000000000000000000000002::coin::Coin
        0x0000000000000000000000000000000000000000000000000000000000000002::coin::Coin<0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>
        vector<u256>
        vector<0x0000000000000000000000000000000000000000000000000000000000000003::staking_pool::StakedSui>
        "###);
    }

    #[test]
    fn test_invalid_type_input() {
        for invalid_type_input in [
            "not_a_real_type",
            "0x1:missing::colon",
            "0x2",
            "0x2::coin",
            "0x2::trailing::",
            "0x3::mismatched::bra<0x4::ke::ts",
            "vector",
        ] {
            assert!(TypeInput::from_str(invalid_type_input).is_err());
        }
    }

    #[test]
    fn test_parse_type_filter() {
        // Test package filter
        let filter = TypeFilter::from_str("0x2").unwrap();
        assert!(matches!(filter, TypeFilter::Package(_)));

        // Test module filter
        let filter = TypeFilter::from_str("0x2::coin").unwrap();
        assert!(matches!(filter, TypeFilter::Module(_, ref m) if m == "coin"));

        // Test type filter without params
        let filter = TypeFilter::from_str("0x2::coin::Coin").unwrap();
        assert!(matches!(filter, TypeFilter::Type(ref t) if t.type_params.is_empty()));

        // Test type filter with params
        let filter = TypeFilter::from_str("0x2::coin::Coin<0x2::sui::SUI>").unwrap();
        assert!(matches!(filter, TypeFilter::Type(ref t) if !t.type_params.is_empty()));
    }

    #[test]
    fn test_invalid_type_filter() {
        assert!(TypeFilter::from_str("not_valid").is_err());
        assert!(TypeFilter::from_str("0x2::").is_err());
        assert!(TypeFilter::from_str("::module").is_err());
    }

    #[test]
    fn test_type_filter_intersect() {
        let pkg = TypeFilter::from_str("0x2").unwrap();
        let module = TypeFilter::from_str("0x2::coin").unwrap();
        let type_no_params = TypeFilter::from_str("0x2::coin::Coin").unwrap();
        let type_with_params = TypeFilter::from_str("0x2::coin::Coin<0x2::sui::SUI>").unwrap();

        // Package intersect with module in same package
        assert!(matches!(
            pkg.clone().intersect(module.clone()),
            Some(TypeFilter::Module(_, _))
        ));

        // Module intersect with type in same module
        assert!(matches!(
            module.clone().intersect(type_no_params.clone()),
            Some(TypeFilter::Type(_))
        ));

        // Type without params intersect with type with params
        assert!(matches!(
            type_no_params.clone().intersect(type_with_params.clone()),
            Some(TypeFilter::Type(ref t)) if !t.type_params.is_empty()
        ));

        // Incompatible filters
        let other_pkg = TypeFilter::from_str("0x3").unwrap();
        assert!(pkg.intersect(other_pkg).is_none());
    }
}
