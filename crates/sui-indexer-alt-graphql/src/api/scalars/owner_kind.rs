// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use async_graphql::Enum;

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
