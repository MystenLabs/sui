// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

#[derive(SimpleObject)]
pub(crate) struct MoveType {
    pub repr: String,
    // typeName: MoveTypeName!
    // typeParameters: [MoveType]
}
