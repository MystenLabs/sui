// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module cms::genesis {

  // Sui imports.
  use sui::transfer;
  use sui::package::{Self};
  use sui::object::{Self, UID};
  use sui::tx_context::{sender, TxContext};

  // Manager capability assigned to whoever deploys the contract
  // AdminCap is transferrable in case the owner needs to change addresses.
  struct AdminCap has key, store { 
    id: UID 
  }

  // OTW to create the publisher
  struct GENESIS has drop {}

  struct SharedItem has key { 
    id: UID
    }

  fun init(otw: GENESIS, ctx: &mut TxContext) { 

    // Claim the Publisher for the Package
    let publisher = package::claim(otw, ctx);

    // Transfer the Publisher to the sender
    transfer::public_transfer(publisher, sender(ctx));

    // Create a shared object
    transfer::share_object(SharedItem {
      id: object::new(ctx)
    });

    // Generate 20 Admin Caps, for parallelization of transactions
    let i = 0;
    while (i <= 20) {
      // Transfer Admin Cap to sender
      transfer::public_transfer(AdminCap {
        id: object::new(ctx)
      }, sender(ctx));
      i = i + 1;
    }
  }
}
