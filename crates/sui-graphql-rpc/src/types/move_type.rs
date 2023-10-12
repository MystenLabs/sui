// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

/**
 *
 * type MoveType {
  # Scalar representation of the type instantiation (type and type
  # parameters)
  repr: String!
  typeName: MoveTypeName!
  typeParameters: [MoveType]
}

type MoveTypeName {
  # Fully qualified type name.  Primitive types have no `moduleId`, or
  # `struct`.
  moduleId: MoveModuleId
  name: String!
  struct: MoveStructDecl!
}
 *
 */

#[derive(SimpleObject)]
pub(crate) struct MoveType {
    pub repr: String,
}
