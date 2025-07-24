// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use bincode::{enc::Encoder, error::EncodeError, serde::BorrowCompat, Encode};
use move_core_types::language_storage::StructTag;
use sui_indexer_alt_framework::types::{
    base_types::SuiAddress, parse_sui_address, parse_sui_module_id, parse_sui_struct_tag,
};

/// Structured form of a type filter that could be just a package, a module, an uninstantiated
/// type, or a fully qualified type with generics.
pub(crate) enum TypeFilter {
    Package(SuiAddress),
    Module(SuiAddress, String),
    Type(StructTag),
}

#[derive(thiserror::Error, Debug)]
#[error("{0}")]
pub(crate) struct Error(&'static str);

/// The encoding of each of `TypeFilter`'s variants represents a different key prefix for a RocksDB
/// table that includes types in its keys. This means that this encoding is one-way -- it is not
/// possible to decode the variant solely from the encoded bytes.
impl Encode for TypeFilter {
    fn encode<E: Encoder>(&self, e: &mut E) -> Result<(), EncodeError> {
        match self {
            TypeFilter::Package(package) => {
                BorrowCompat(package).encode(e)?;
            }

            TypeFilter::Module(package, module) => {
                BorrowCompat(package).encode(e)?;
                module.encode(e)?;
            }

            // If there are no type parameters, don't encode the type param vector at all, so that
            // we can find all different instantiations of the same type.
            TypeFilter::Type(tag) if tag.type_params.is_empty() => {
                BorrowCompat(tag.address).encode(e)?;
                tag.module.as_str().encode(e)?;
                tag.name.as_str().encode(e)?;
            }

            TypeFilter::Type(tag) => {
                BorrowCompat(tag).encode(e)?;
            }
        }
        Ok(())
    }
}

impl FromStr for TypeFilter {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        if let Ok(tag) = parse_sui_struct_tag(s) {
            Ok(TypeFilter::Type(tag))
        } else if let Ok(module) = parse_sui_module_id(s) {
            Ok(TypeFilter::Module(
                SuiAddress::from(*module.address()),
                module.name().to_string(),
            ))
        } else if let Ok(package) = parse_sui_address(s) {
            Ok(TypeFilter::Package(package))
        } else {
            Err(Error("package[::module[::name[<type, ...>]]]"))
        }
    }
}
