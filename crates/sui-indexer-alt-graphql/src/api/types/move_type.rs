// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use move_core_types::language_storage::TypeTag;
use sui_types::{base_types::MoveObjectType, type_input::TypeInput};

/// Represents concrete types (no type parameters, no references).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MoveType {
    pub native: TypeInput,
}

/// Represents concrete types (no type parameters, no references).
#[Object]
impl MoveType {
    /// Flat representation of the type signature, as a displayable string.
    async fn repr(&self) -> Option<String> {
        Some(self.native.to_canonical_string(/* with_prefix */ true))
    }
}

impl From<MoveObjectType> for MoveType {
    fn from(obj: MoveObjectType) -> Self {
        let tag: TypeTag = obj.into();
        Self { native: tag.into() }
    }
}

impl From<TypeTag> for MoveType {
    fn from(tag: TypeTag) -> Self {
        Self { native: tag.into() }
    }
}

impl From<TypeInput> for MoveType {
    fn from(native: TypeInput) -> Self {
        Self { native }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_move_type_from_type_tag() {
        let tag = TypeTag::from_str("u64").unwrap();
        let move_type = MoveType::from(tag);
        assert_eq!(move_type.native.to_canonical_string(true), "u64");
    }

    #[test]
    fn test_move_type_from_type_input() {
        let input = TypeInput::U64;
        let move_type = MoveType::from(input);
        assert_eq!(move_type.native.to_canonical_string(true), "u64");
    }

    #[test]
    fn test_complex_type() {
        let tag = TypeTag::from_str("vector<0x42::foo::Bar<address, u32>>").unwrap();
        let move_type = MoveType::from(tag);
        assert_eq!(move_type.native.to_canonical_string(true), "vector<0x0000000000000000000000000000000000000000000000000000000000000042::foo::Bar<address,u32>>");
    }
}
