// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::block::BlockRef;

/// Specifies one consensus commit.
/// It is stored on disk, so it does not contain blocks which are stored individually.
#[allow(unused)]
#[derive(Deserialize, Serialize)]
pub(crate) struct Commit {
    /// Index of the commit.
    /// First commit after genesis has an index of 1, then every next commit has an index incremented by 1.
    pub index: u64,
    /// A reference to the the commit leader.
    pub leader: BlockRef,
    /// Refs to committed blocks, in the commit order.
    pub blocks: Vec<BlockRef>,
}
