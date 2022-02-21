// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;

/*

An authority asynchronously creates blocks from its sequence of 
certificates / effects. Then both the sequence of certificates 
/ effects are transmitted to listeners (as a transaction digest)
as well as blocks.

The architecture is as follows:
- The authority store notifies through the Sender that a new 
  certificate / effect has been sequenced, at a specific sequence
  number.
- The sender sends this information through a channel to the Manager,
  that decides whether a new block should be made. This is based on
  time elapsed as well as current size of block. If so a new block 
  is created.
- The authority manager also holds the sending ends of a number of 
  channels that eventually go to clients that registered interest
  in receiving all updates from the authority. When a new item is
  sequenced of a block created this is sent out to them.

*/


/// Either a freshly sequenced transaction hash or a block
pub struct UpdateItem { }

pub struct Sender { }
pub struct Manager { }

impl Sender {
    /// Send a new event to the block manager
    pub fn sequenced_item() { }
}

impl Manager {
    /// Starts the manager service / tokio task
    pub fn start_service() { }

    /// Register a sending channel used to send streaming
    /// updates to clients.
    pub fn register_listener() { }
}


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
