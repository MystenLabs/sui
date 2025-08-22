// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use async_graphql::Enum;
use sui_indexer_alt_reader::consistent_reader::proto;

/// Filter on who owns an object.
#[derive(Enum, Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum OwnerKind {
    /// Object is owned by an address.
    Address,

    /// Object is a child of another object (e.g. a dynamic field or dynamic object field).
    Object,

    /// Object is shared among multiple owners.
    Shared,

    /// Object is frozen.
    Immutable,
}

impl From<OwnerKind> for proto::owner::OwnerKind {
    fn from(kind: OwnerKind) -> Self {
        match kind {
            OwnerKind::Address => proto::owner::OwnerKind::Address,
            OwnerKind::Object => proto::owner::OwnerKind::Object,
            OwnerKind::Shared => proto::owner::OwnerKind::Shared,
            OwnerKind::Immutable => proto::owner::OwnerKind::Immutable,
        }
    }
}

impl fmt::Display for OwnerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OwnerKind::Address => write!(f, "'ADDRESS'"),
            OwnerKind::Object => write!(f, "'OBJECT'"),
            OwnerKind::Shared => write!(f, "'SHARED'"),
            OwnerKind::Immutable => write!(f, "'IMMUTABLE'"),
        }
    }
}
