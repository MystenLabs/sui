// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

// TODO(DVX-1373): Implement MoveCallCommand
/// A call to a Move function.
#[derive(SimpleObject, Clone)]
pub struct MoveCallCommand {
    /// Placeholder field
    #[graphql(name = "_")]
    pub dummy: Option<bool>,
}
