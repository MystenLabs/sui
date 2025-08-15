// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use async_graphql::{CustomValidator, InputObject, InputValueError};

use crate::{
    api::scalars::{owner_kind::OwnerKind, sui_address::SuiAddress},
    intersect,
};

use super::type_filter::TypeFilter;

/// A filter over the live object set, the filter can be one of:
///
/// - A filter on type (all live objects whose type matches that filter).
/// - Fetching all objects owned by an address or object, optionally filtered by type.
/// - Fetching all shared or immutable objects, filtered by type.
#[derive(InputObject, Default, Debug, Clone, Eq, PartialEq)]
pub(crate) struct ObjectFilter {
    /// Filter on whether the object is address-owned, object-owned, shared, or immutable.
    ///
    /// - If this field is set to "ADDRESS" or "OBJECT", then an owner filter must also be provided.
    /// - If this field is set to "SHARED" or "IMMUTABLE", then a type filter must also be provided.
    pub owner_kind: Option<OwnerKind>,

    /// Specifies the address of the owning address or object.
    ///
    /// This field is required if `ownerKind` is "ADDRESS" or "OBJECT". If provided without `ownerKind`, `ownerKind` defaults to "ADDRESS".
    pub owner: Option<SuiAddress>,

    /// Filter on the object's type. The filter can be one of:
    ///
    /// - A package address: `0x2`,
    /// - A module: `0x2::coin`,
    /// - A fully-qualified name: `0x2::coin::Coin`,
    /// - A type instantiation: `0x2::coin::Coin<0x2::sui::SUI>`.
    pub type_: Option<TypeFilter>,
}

#[derive(Default)]
pub(crate) struct Validator {
    /// Whether to allow an empty filter on input.
    allow_empty: bool,
}

impl Validator {
    /// Create a validator that allows empty filters.
    pub(crate) fn allows_empty() -> Self {
        Self { allow_empty: true }
    }
}

impl ObjectFilter {
    /// Try to create a filter whose results are the intersection of `self`'s results and `other`'s
    /// results. May return `None` if the filters are incompatible (would result in no matches)
    pub(crate) fn intersect(self, other: Self) -> Option<Self> {
        macro_rules! intersect {
            ($field:ident, $body:expr) => {
                intersect::field(self.$field, other.$field, $body)
            };
        }

        Some(Self {
            owner_kind: intersect!(owner_kind, intersect::by_eq)?,
            owner: intersect!(owner, intersect::by_eq)?,
            type_: intersect!(type_, TypeFilter::intersect)?,
        })
    }
}

impl CustomValidator<ObjectFilter> for Validator {
    fn check(&self, filter: &ObjectFilter) -> Result<(), InputValueError<ObjectFilter>> {
        match filter {
            ObjectFilter {
                owner_kind: Some(kind @ (OwnerKind::Address | OwnerKind::Object)),
                owner: None,
                type_: _,
            } => Err(InputValueError::custom(format!(
                "{kind} owner kind requires an `owner` to be specified"
            ))),

            ObjectFilter {
                owner_kind: Some(kind @ (OwnerKind::Shared | OwnerKind::Immutable)),
                owner: Some(_),
                type_: _,
            } => Err(InputValueError::custom(format!(
                "Unexpected `owner` for {kind} owner kind",
            ))),

            ObjectFilter {
                owner_kind: Some(kind @ (OwnerKind::Shared | OwnerKind::Immutable)),
                owner: None,
                type_: None,
            } => Err(InputValueError::custom(format!(
                "{kind} owner kind requires a `type` to be specified"
            ))),

            ObjectFilter {
                owner_kind: None,
                owner: None,
                type_: None,
            } => {
                if self.allow_empty {
                    Ok(())
                } else {
                    Err(InputValueError::custom("No `ObjectFilter` specified"))
                }
            }

            // Valid address/object owner filter
            ObjectFilter {
                owner_kind: None | Some(OwnerKind::Address | OwnerKind::Object),
                owner: Some(_),
                type_: _,
            } => Ok(()),

            // Valid shared/immutable owner filter
            ObjectFilter {
                owner_kind: Some(OwnerKind::Shared | OwnerKind::Immutable),
                owner: None,
                type_: Some(_),
            } => Ok(()),

            // Valid type-only filter
            ObjectFilter {
                owner_kind: None,
                owner: None,
                type_: Some(_),
            } => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_intersection() {
        let f = ObjectFilter {
            owner_kind: Some(OwnerKind::Object),
            owner: Some("0x1".parse().unwrap()),
            type_: Some("0x2::coin".parse().unwrap()),
        };

        assert_eq!(ObjectFilter::default().intersect(f.clone()).unwrap(), f);
        assert_eq!(f.clone().intersect(ObjectFilter::default()).unwrap(), f);
    }

    #[test]
    fn test_owner_kind_intersection() {
        let a = ObjectFilter {
            owner_kind: Some(OwnerKind::Address),
            owner: None,
            type_: None,
        };

        let o = ObjectFilter {
            owner_kind: Some(OwnerKind::Object),
            owner: None,
            type_: None,
        };

        // Same owner_kind should intersect successfully
        assert_eq!(a.clone().intersect(a.clone()).unwrap(), a);
        assert_eq!(o.clone().intersect(o.clone()).unwrap(), o);

        // Different owner_kind should return None
        assert!(a.clone().intersect(o.clone()).is_none());
        assert!(o.intersect(a).is_none());
    }

    #[test]
    fn test_owner_intersection() {
        let a1 = ObjectFilter {
            owner_kind: None,
            owner: Some("0x1".parse().unwrap()),
            type_: None,
        };

        let a2 = ObjectFilter {
            owner_kind: None,
            owner: Some("0x2".parse().unwrap()),
            type_: None,
        };

        // Same owner should intersect successfully
        assert_eq!(a1.clone().intersect(a1.clone()).unwrap(), a1);
        assert_eq!(a2.clone().intersect(a2.clone()).unwrap(), a2);

        // Different owner_kind should return None
        assert!(a1.clone().intersect(a2.clone()).is_none());
        assert!(a2.intersect(a1).is_none());
    }

    #[test]
    fn test_type_filter_intersection() {
        let coin = ObjectFilter {
            owner_kind: None,
            owner: None,
            type_: Some("0x2::coin::Coin".parse().unwrap()),
        };

        let sui = ObjectFilter {
            owner_kind: None,
            owner: None,
            type_: Some("0x2::coin::Coin<0x2::sui::SUI>".parse().unwrap()),
        };

        let token = ObjectFilter {
            owner_kind: None,
            owner: None,
            type_: Some("0x2::token::Token".parse().unwrap()),
        };

        // Compatible type filters intersect to become the more specific filter, regardless of
        // intersection order.
        assert_eq!(coin.clone().intersect(sui.clone()).unwrap(), sui);
        assert_eq!(sui.clone().intersect(coin.clone()).unwrap(), sui);

        // Incompatible filters don't intersect
        assert!(coin.clone().intersect(token.clone()).is_none());
        assert!(token.intersect(coin).is_none())
    }

    #[test]
    fn test_combined_intersection() {
        let a_coin = ObjectFilter {
            owner_kind: Some(OwnerKind::Address),
            owner: Some("0x1".parse().unwrap()),
            type_: Some("0x2::coin::Coin".parse().unwrap()),
        };

        let a_sui = ObjectFilter {
            owner_kind: Some(OwnerKind::Address),
            owner: Some("0x1".parse().unwrap()),
            type_: Some("0x2::coin::Coin<0x2::sui::SUI>".parse().unwrap()),
        };

        let o_coin = ObjectFilter {
            owner_kind: Some(OwnerKind::Object),
            owner: Some("0x1".parse().unwrap()),
            type_: Some("0x2::coin::Coin".parse().unwrap()),
        };

        assert_eq!(a_coin.clone().intersect(a_sui.clone()).unwrap(), a_sui);
        assert!(a_coin.clone().intersect(o_coin.clone()).is_none());
    }
}
