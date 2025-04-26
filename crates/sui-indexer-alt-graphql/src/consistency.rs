// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::connection::CursorType;
use serde::{Deserialize, Serialize};

use crate::api::scalars::cursor::JsonCursor;

/// Trait for cursors that fix results to a particular checkpoint snapshot.
pub(crate) trait Checkpointed: CursorType {
    fn checkpoint_viewed_at(&self) -> u64;
}

/// A consistent cursor pointing into an ordered collection with an index.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub(crate) struct Indexed {
    #[serde(rename = "i")]
    pub(crate) ix: usize,
    #[serde(rename = "c")]
    pub(crate) checkpoint_viewed_at: u64,
}

impl Checkpointed for JsonCursor<Indexed> {
    fn checkpoint_viewed_at(&self) -> u64 {
        self.checkpoint_viewed_at
    }
}
