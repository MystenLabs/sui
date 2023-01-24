// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module struct_metadata::struct_metadata{
  use std::string::{Self, String};
  use sui::tx_context::{Self, TxContext};
  use sui::object::{Self, UID};
  use sui::transfer;
  
  struct Dummy  has key{
    id: UID,
    number: u64,
    description: String,
  }

  fun init(ctx: &mut TxContext){
    let id = object::new(ctx);
    let dummy = Dummy{id, number: 1, description: string::utf8(b"Hello")};
    transfer::transfer(dummy, tx_context::sender(ctx))
  }
}