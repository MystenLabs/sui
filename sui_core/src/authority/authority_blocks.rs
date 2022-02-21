// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;

pub struct AuthorityBlock {
    /// The total number of items executed by this authority.
    total_size: usize,

    /// The number of items in the previous block.
    previous_total_size: usize,
    // TODO: Add the following information:
    // - Authenticator of previous block (digest)
    // - Authenticator of this block header + contents (digest)
    // - Signature on block + authenticators
    // - Structures to facilitate sync, eg. IBLT or Merkle Tree.
    // - Maybe: a timestamp (wall clock time)?
}
