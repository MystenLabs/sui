// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::block::BlockRef;

/// Specifies one consensus commit.
/// It is stored on disk, so it does not contain blocks which are stored individually.
#[allow(unused)]
#[derive(Deserialize, Serialize)]
pub(crate) struct Commit {
    /// A reference to the anchor / leader of the commit.
    pub anchor: BlockRef,
    /// All committed blocks in the commit order.
    pub blocks: Vec<BlockRef>,
    /// Height of the commit.
    /// First commit after genesis has a height of 1, then every next commit has a height incremented by 1.
    pub height: u64,
}
