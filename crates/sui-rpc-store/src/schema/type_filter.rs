// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Structured Move-type filters that double as RocksDB key
//! prefixes for CFs whose keys carry a `StructTag` BCS-encoded
//! inline (currently [`super::object_by_owner`] and
//! [`super::object_by_type`]).
//!
//! Each variant produces a different byte prefix:
//!
//! - [`TypeFilter::Package`] — match every Move type defined in a
//!   package address.
//! - [`TypeFilter::Module`] — match every type in a package's
//!   named module.
//! - [`TypeFilter::Type`] — match a specific Move type, with two
//!   sub-cases handled by the encoding (see below).
//!
//! Encoding is *one-way*: the bytes are designed to compare equal
//! to the leading bytes of `bcs::to_bytes(&StructTag)` for any
//! matching tag, so a `TypeFilter` cannot be reliably decoded back
//! from its bytes.
//!
//! ## The empty-`type_params` case
//!
//! The BCS encoding of `StructTag` always ends in
//! `uleb128(type_params.len()) || params`. If
//! [`TypeFilter::Type`] is given a tag whose `type_params` is
//! empty, naive prefix encoding would pin the type-params length
//! to zero, so generic types like `Coin<T>` (which always carry
//! parameters) would never match. The encode impl special-cases
//! that branch and skips the length byte entirely, so an
//! empty-params tag matches every instantiation of the named
//! type. A tag with non-empty `type_params` matches only the
//! exact pinned instantiation.

use bytes::BufMut;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use sui_consistent_store::Encode;
use sui_consistent_store::error::EncodeError;
use sui_types::base_types::SuiAddress;

use crate::schema::primitives::write_uleb128;

/// Structured form of a type-prefix filter. See the
/// [module docs](self) for the encoding contract.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeFilter {
    /// Match every Move type defined in the package at this
    /// address.
    Package(SuiAddress),
    /// Match every Move type defined in this package's named
    /// module.
    Module {
        package: SuiAddress,
        module: Identifier,
    },
    /// Match a Move type. If `type_params` is empty the encoding
    /// omits the params-length byte so the filter matches every
    /// instantiation (e.g. `Coin<SUI>`, `Coin<USDC>`); otherwise
    /// it pins the full instantiation.
    Type(StructTag),
}

impl Encode for TypeFilter {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        match self {
            TypeFilter::Package(package) => {
                buf.put_slice(package.as_ref());
            }
            TypeFilter::Module { package, module } => {
                buf.put_slice(package.as_ref());
                write_identifier(module, buf);
            }
            // Empty `type_params` means "match every instantiation
            // of the named type". Skip the params-length byte so
            // the encoding stops before the variable-length
            // params section of any concrete `StructTag` BCS.
            TypeFilter::Type(tag) if tag.type_params.is_empty() => {
                buf.put_slice(tag.address.as_ref());
                write_identifier(&tag.module, buf);
                write_identifier(&tag.name, buf);
            }
            TypeFilter::Type(tag) => {
                let bytes = bcs::to_bytes(tag)
                    .map_err(|e| EncodeError::with_source("bcs encode StructTag", e))?;
                buf.put_slice(&bytes);
            }
        }
        Ok(())
    }
}

/// BCS-compatible encoding of an `Identifier`: uleb128 length
/// followed by its UTF-8 bytes.
fn write_identifier<B: BufMut>(id: &Identifier, buf: &mut B) {
    let bytes = id.as_str().as_bytes();
    write_uleb128(bytes.len() as u32, buf);
    buf.put_slice(bytes);
}

#[cfg(test)]
mod tests {
    use move_core_types::account_address::AccountAddress;
    use move_core_types::language_storage::TypeTag;

    use super::*;

    fn addr(byte: u8) -> AccountAddress {
        AccountAddress::new([byte; 32])
    }

    fn ident(s: &str) -> Identifier {
        Identifier::new(s).unwrap()
    }

    fn tag(addr_byte: u8, module: &str, name: &str, type_params: Vec<TypeTag>) -> StructTag {
        StructTag {
            address: addr(addr_byte),
            module: ident(module),
            name: ident(name),
            type_params,
        }
    }

    /// Every `TypeFilter` variant must encode to a real byte
    /// prefix of the BCS encoding of any `StructTag` it should
    /// match.
    fn assert_is_prefix_of(filter: &TypeFilter, tag: &StructTag) {
        let filter_bytes = filter.encode().unwrap();
        let tag_bytes = bcs::to_bytes(tag).unwrap();
        assert!(
            tag_bytes.starts_with(&filter_bytes),
            "filter bytes {:?} not a prefix of tag bytes {:?}",
            filter_bytes,
            tag_bytes,
        );
    }

    fn assert_not_prefix_of(filter: &TypeFilter, tag: &StructTag) {
        let filter_bytes = filter.encode().unwrap();
        let tag_bytes = bcs::to_bytes(tag).unwrap();
        assert!(
            !tag_bytes.starts_with(&filter_bytes),
            "filter bytes {:?} unexpectedly prefix of tag bytes {:?}",
            filter_bytes,
            tag_bytes,
        );
    }

    #[test]
    fn package_matches_any_tag_at_address() {
        let filter = TypeFilter::Package(SuiAddress::from(addr(2)));
        for t in [
            tag(2, "sui", "SUI", vec![]),
            tag(2, "coin", "Coin", vec![TypeTag::U64]),
            tag(2, "other", "Thing", vec![TypeTag::Address]),
        ] {
            assert_is_prefix_of(&filter, &t);
        }
    }

    #[test]
    fn module_matches_tags_in_named_module() {
        let filter = TypeFilter::Module {
            package: SuiAddress::from(addr(2)),
            module: ident("coin"),
        };
        for t in [
            tag(2, "coin", "Coin", vec![TypeTag::U64]),
            tag(2, "coin", "TreasuryCap", vec![]),
        ] {
            assert_is_prefix_of(&filter, &t);
        }
    }

    #[test]
    fn type_with_empty_params_matches_every_instantiation() {
        let filter = TypeFilter::Type(tag(2, "coin", "Coin", vec![]));
        for t in [
            tag(2, "coin", "Coin", vec![TypeTag::U64]),
            tag(
                2,
                "coin",
                "Coin",
                vec![TypeTag::Struct(Box::new(tag(2, "sui", "SUI", vec![])))],
            ),
            tag(2, "coin", "Coin", vec![]),
        ] {
            assert_is_prefix_of(&filter, &t);
        }
    }

    #[test]
    fn type_with_pinned_params_matches_only_that_instantiation() {
        let pinned = tag(2, "coin", "Coin", vec![TypeTag::U64]);
        let filter = TypeFilter::Type(pinned.clone());
        assert_is_prefix_of(&filter, &pinned);

        // A different instantiation must not match.
        let other = tag(
            2,
            "coin",
            "Coin",
            vec![TypeTag::Struct(Box::new(tag(2, "sui", "SUI", vec![])))],
        );
        assert_not_prefix_of(&filter, &other);
    }
}
